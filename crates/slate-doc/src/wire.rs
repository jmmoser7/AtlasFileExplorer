//! Derived connector geometry: the bezier span and an obstacle-aware
//! orthogonal (PCB-trace) router.
//!
//! Geometry is never stored — both interpreters (the egui board painter and
//! the artifact writer) call [`connector_route`] with the current [`WireHost`]
//! pose of each anchored node and the session [`WireRouting`]. Orthogonal
//! paths take a 50/50 three-leg jive when that corridor is clear, then an
//! L, then a wrap; stairs and the Hanan fallback only run when a host
//! actually blocks the simple path. Equal-length ties prefer the
//! right-hand path, then the bottom path (screen axes: +x right, +y down).

use crate::scene::{
    connector_bezier_from_dirs, ConnectorBezier, ConnectorEnd, ConnectorNode, Node, NodeKind,
    Scene, ShapeKind, Side, WorldRect,
};
use crate::wire_host::WireHost;
use crate::NodeId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// How board wires are drawn. Session preference (Document Settings), not a
/// journaled scene property — same class as the grid and object snaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireRouting {
    #[default]
    Bezier,
    Orthogonal,
}

impl WireRouting {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bezier => "Bezier",
            Self::Orthogonal => "Orthogonal",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Bezier => Self::Orthogonal,
            Self::Orthogonal => Self::Bezier,
        }
    }
}

/// Corner fillet for orthogonal wires, in world units (P0.9 — scales with zoom).
pub const ORTHO_CORNER_RADIUS: f32 = 8.0;
/// Preferred gutter past a host edge, in world units.
pub const ORTHO_CLEARANCE: f32 = 12.0;
/// Parallel-rail spacing for bundled orthogonal wires (File Atlas nested rails).
pub const ORTHO_RAIL_GAP: f32 = 10.0;
/// Fan spacing along a shared port edge.
pub const ORTHO_EXIT_GAP: f32 = 8.0;
/// Dest must clear the port centre by this many world units before it
/// claims a fan side. Inside the band the wire stays on the port mid
/// (no sign flip while a dest is dragged across its sibling).
const LANE_LOCK: f32 = 12.0;

/// One node's contribution to the orthogonal obstacle set. Collision uses
/// the oriented silhouette (`rect` rotated by `rotation_deg`) — a box, or
/// the oval when `ellipse` is set — not the world AABB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WireObstacle {
    pub id: NodeId,
    pub rect: WorldRect,
    pub rotation_deg: f32,
    pub ellipse: bool,
}

impl WireObstacle {
    pub fn from_node(n: &Node) -> Self {
        Self {
            id: n.id,
            rect: n.rect,
            rotation_deg: n.rotation_deg,
            ellipse: matches!(&n.kind, NodeKind::Shape(s) if s.shape == ShapeKind::Ellipse),
        }
    }
}

/// Derived bundle offsets for one connector. Not journaled — recomputed
/// from the current scene so sibling wires stay spaced (P1.wire.rails).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct OrthoLane {
    /// Slide the start attach along the port edge (world units).
    pub start_along: f32,
    /// Slide the end attach along the port edge.
    pub end_along: f32,
    /// Mid-span rail offset from the preferred midpoint jive.
    pub rail: f32,
}

const EPS: f32 = 0.35;
const GRID_CAP: usize = 2_500;

/// One command of a filleted polyline (shared by both interpreters).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCmd {
    Move([f32; 2]),
    Line([f32; 2]),
    Cubic {
        c1: [f32; 2],
        c2: [f32; 2],
        to: [f32; 2],
    },
}

/// Derived wire: either the existing cubic or an orthogonal polyline.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectorPath {
    Bezier(ConnectorBezier),
    Orthogonal(Vec<[f32; 2]>),
}

impl ConnectorPath {
    pub fn start(&self) -> [f32; 2] {
        match self {
            Self::Bezier(b) => b.p0,
            Self::Orthogonal(pts) => pts.first().copied().unwrap_or([0.0, 0.0]),
        }
    }

    pub fn end(&self) -> [f32; 2] {
        match self {
            Self::Bezier(b) => b.p3,
            Self::Orthogonal(pts) => pts.last().copied().unwrap_or([0.0, 0.0]),
        }
    }

    /// Unit tangent pointing *into* the curve at the start.
    pub fn start_dir(&self) -> [f32; 2] {
        match self {
            Self::Bezier(b) => b.start_dir(),
            Self::Orthogonal(pts) => first_dir(pts),
        }
    }

    /// Unit tangent pointing *into* the curve at the end (from the endpoint
    /// toward the interior). Arrowheads at `b` point along the negation.
    pub fn end_dir(&self) -> [f32; 2] {
        match self {
            Self::Bezier(b) => b.end_dir(),
            Self::Orthogonal(pts) => last_dir_inward(pts),
        }
    }

    pub fn midpoint(&self) -> [f32; 2] {
        match self {
            Self::Bezier(b) => b.midpoint(),
            Self::Orthogonal(pts) => polyline_midpoint(pts),
        }
    }

    pub fn aabb(&self) -> WorldRect {
        match self {
            Self::Bezier(b) => b.aabb(),
            Self::Orthogonal(pts) => polyline_aabb(pts),
        }
    }

    pub fn points(&self) -> Vec<[f32; 2]> {
        match self {
            Self::Bezier(b) => vec![b.p0, b.c1, b.c2, b.p3],
            Self::Orthogonal(pts) => pts.clone(),
        }
    }
}

/// Visible non-connector hosts — the obstacle set for orthogonal routing.
pub fn scene_wire_obstacles(scene: &Scene) -> Vec<WireObstacle> {
    scene
        .nodes
        .iter()
        .filter(|n| !n.hidden && !matches!(n.kind, NodeKind::Connector(_)))
        .map(WireObstacle::from_node)
        .collect()
}

/// File Atlas nested-rail assignment: wires that share a port fan along
/// that edge and take parallel mid-span rails so they do not stack.
///
/// Each dest claims the fan side it already sits on (the sign of its
/// projection on the port tangent). That is the no-crossover rule —
/// an upper dest never takes the lower rail. A dead zone around the
/// port centre keeps the sign from flipping while a dest is dragged
/// across the mid. Connector id only breaks a true tie.
pub fn scene_ortho_lanes(scene: &Scene) -> HashMap<NodeId, OrthoLane> {
    struct Item {
        id: NodeId,
        a: ConnectorEnd,
        b: ConnectorEnd,
    }
    let items: Vec<Item> = scene
        .nodes
        .iter()
        .filter(|n| !n.hidden)
        .filter_map(|n| match &n.kind {
            NodeKind::Connector(c) => Some(Item {
                id: n.id,
                a: c.a,
                b: c.b,
            }),
            _ => None,
        })
        .collect();

    let mut lanes: HashMap<NodeId, OrthoLane> =
        items.iter().map(|i| (i.id, OrthoLane::default())).collect();

    let mut ports: HashMap<(NodeId, Side), Vec<(NodeId, bool, [f32; 2])>> = HashMap::new();
    for it in &items {
        for (end, end_b) in [(&it.a, false), (&it.b, true)] {
            let ConnectorEnd::Anchored { node, side, t } = *end else {
                continue;
            };
            if scene.node(node).is_none_or(|n| n.hidden) {
                continue;
            }
            let other = if end_b { it.a } else { it.b };
            let other_pt = end_world(scene, &other);
            let _ = t;
            ports
                .entry((node, side))
                .or_default()
                .push((it.id, end_b, other_pt));
        }
    }

    struct RailPick {
        size: usize,
        rail: f32,
        node: u64,
        side: u8,
    }
    let mut rail_from: HashMap<NodeId, RailPick> = HashMap::new();
    for ((node, side), group) in ports {
        if group.len() < 2 {
            continue;
        }
        let Some(host) = scene.node(node) else {
            continue;
        };
        let h = WireHost::from_node(host);
        let out = h.outward(side, 0.5);
        let tan = [out[1], -out[0]];
        let origin = h.anchor(side, 0.5);
        let edge_len = if side.is_box() {
            len(sub(h.anchor(side, 1.0), h.anchor(side, 0.0)))
        } else {
            48.0
        };
        let max_along = (edge_len * 0.5 - 6.0).max(ORTHO_EXIT_GAP);
        let gap = ORTHO_EXIT_GAP.min(max_along / (group.len() as f32 * 0.5).max(1.0));
        let size = group.len();
        let key = (node.0, side_ord(side));

        let mut pos: Vec<(f32, NodeId, bool)> = Vec::new();
        let mut neg: Vec<(f32, NodeId, bool)> = Vec::new();
        for (cid, end_b, other) in &group {
            let dot = (other[0] - origin[0]) * tan[0] + (other[1] - origin[1]) * tan[1];
            if dot > LANE_LOCK {
                pos.push((dot, *cid, *end_b));
            } else if dot < -LANE_LOCK {
                neg.push((-dot, *cid, *end_b));
            }
            // |dot| ≤ LANE_LOCK: leave on the port mid. No sign to flip.
        }
        // Longest run first (File Atlas) so nested rails stay crossing-free;
        // id is the tie-break when two dests share a breadth.
        for list in [&mut pos, &mut neg] {
            list.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1 .0.cmp(&b.1 .0)));
        }

        let assign = |lanes: &mut HashMap<NodeId, OrthoLane>,
                      rail_from: &mut HashMap<NodeId, RailPick>,
                      list: &[(f32, NodeId, bool)],
                      sign: f32| {
            for (r, &(_, cid, end_b)) in list.iter().enumerate() {
                let along = sign * (r as f32 + 1.0) * gap;
                let lane = lanes.entry(cid).or_default();
                if end_b {
                    lane.end_along = along;
                } else {
                    lane.start_along = along;
                }
                let pick = RailPick {
                    size,
                    rail: sign * (r as f32 + 1.0) * ORTHO_RAIL_GAP,
                    node: key.0,
                    side: key.1,
                };
                let take = match rail_from.get(&cid) {
                    Some(old) => {
                        pick.size > old.size
                            || (pick.size == old.size
                                && (pick.node, pick.side) < (old.node, old.side))
                    }
                    None => true,
                };
                if take {
                    rail_from.insert(cid, pick);
                }
            }
        };
        assign(&mut lanes, &mut rail_from, &pos, 1.0);
        assign(&mut lanes, &mut rail_from, &neg, -1.0);
    }
    for (cid, pick) in rail_from {
        lanes.entry(cid).or_default().rail = pick.rail;
    }
    lanes
}

fn side_ord(side: Side) -> u8 {
    match side {
        Side::Top => 0,
        Side::Right => 1,
        Side::Bottom => 2,
        Side::Left => 3,
        Side::Start => 4,
        Side::Mid => 5,
        Side::End => 6,
    }
}

fn end_world(scene: &Scene, end: &ConnectorEnd) -> [f32; 2] {
    match end {
        ConnectorEnd::Anchored { node, side, t } => scene
            .node(*node)
            .map(|n| WireHost::from_node(n).anchor(*side, *t))
            .unwrap_or([0.0, 0.0]),
        ConnectorEnd::Free { point } => *point,
    }
}

/// Visible-node host lookup for both interpreters.
pub fn scene_wire_hosts(scene: &Scene) -> impl Fn(NodeId) -> Option<WireHost> + '_ {
    |id| {
        scene
            .node(id)
            .filter(|n| !n.hidden)
            .map(WireHost::from_node)
    }
}

/// Derived curve for the current routing. `None` when an anchored node is
/// missing from `host_of`.
pub fn connector_route(
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
    routing: WireRouting,
    obstacles: &[WireObstacle],
    lane: OrthoLane,
) -> Option<ConnectorPath> {
    match routing {
        WireRouting::Bezier => connector_bezier_hosted(a, b, host_of).map(ConnectorPath::Bezier),
        WireRouting::Orthogonal => {
            connector_ortho_path(a, b, host_of, obstacles, lane).map(ConnectorPath::Orthogonal)
        }
    }
}

/// Scene-aware route: looks up hosts, obstacles, and the bundle lane for
/// `connector_id` so sibling orthogonal wires stay spaced.
pub fn connector_route_in_scene(
    scene: &Scene,
    connector_id: Option<NodeId>,
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    routing: WireRouting,
) -> Option<ConnectorPath> {
    let obstacles = scene_wire_obstacles(scene);
    let lane = match (routing, connector_id) {
        (WireRouting::Orthogonal, Some(id)) => scene_ortho_lanes(scene)
            .get(&id)
            .copied()
            .unwrap_or_default(),
        _ => OrthoLane::default(),
    };
    connector_route(a, b, scene_wire_hosts(scene), routing, &obstacles, lane)
}

fn connector_bezier_hosted(
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
) -> Option<ConnectorBezier> {
    let (p0, dir_a) = resolve_end(a, &host_of)?;
    let (p3, dir_b) = resolve_end(b, &host_of)?;
    let chord = sub(p3, p0);
    Some(connector_bezier_from_dirs(
        p0,
        dir_a.unwrap_or(chord),
        p3,
        dir_b.unwrap_or([-chord[0], -chord[1]]),
    ))
}

/// AABB of the derived route (connector `Node.rect` stays equal to this).
pub fn connector_aabb_routed(
    conn: &ConnectorNode,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
    routing: WireRouting,
    obstacles: &[WireObstacle],
    lane: OrthoLane,
) -> Option<WorldRect> {
    connector_route(&conn.a, &conn.b, host_of, routing, obstacles, lane).map(|p| p.aabb())
}

/// Orthogonal path that leaves each end along the host outward (true
/// normal, so a rotated edge does not immediately re-enter), then takes
/// the first clear tier: 50/50 three-leg, L, wrap, stair, Hanan.
/// Tie-break inside a tier: right, then bottom. `lane` offsets siblings.
pub fn connector_ortho_path(
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
    obstacles: &[WireObstacle],
    lane: OrthoLane,
) -> Option<Vec<[f32; 2]>> {
    let (p0_raw, dir_a) = resolve_end(a, &host_of)?;
    let (p3_raw, dir_b) = resolve_end(b, &host_of)?;
    let p0 = fan_anchor(a, p0_raw, dir_a, lane.start_along, &host_of);
    let p3 = fan_anchor(b, p3_raw, dir_b, lane.end_along, &host_of);

    let host_ids = [end_host(a), end_host(b)];
    let hosts: Vec<Solid> = host_ids
        .into_iter()
        .flatten()
        .filter_map(|id| {
            host_of(id).map(|h| Solid {
                rect: h.rect,
                rot: h.rotation_deg,
                ellipse: h.is_ellipse(),
            })
        })
        .collect();
    // Do not inflate bystanders: a 12px halo around the node the pointer
    // is approaching is what made the preview stair around a clear gap.
    // Hosts of this wire are excluded; everyone else is their silhouette.
    let others: Vec<Solid> = obstacles
        .iter()
        .filter(|o| !host_ids.iter().flatten().any(|h| *h == o.id))
        .map(|o| Solid {
            rect: o.rect,
            rot: o.rotation_deg,
            ellipse: o.ellipse,
        })
        .collect();

    let solids = host_solids(&hosts, &others);
    let s_stub = escape_stub(p0, dir_a, &solids);
    let e_stub = escape_stub(p3, dir_b, &solids);
    let tiers = path_tiers(p0, s_stub, p3, e_stub, &solids, lane.rail, dir_a, dir_b);

    for tier in &tiers {
        let pts = if tier.first_wins {
            first_clear(&tier.cands, &solids)
        } else {
            pick_clear(&tier.cands, &solids)
        };
        if let Some(pts) = pts {
            return Some(pts);
        }
    }

    if let Some(pts) = hanan_route(s_stub, e_stub, &solids) {
        let mut full = vec![p0];
        full.extend(pts);
        full.push(p3);
        let pts = collapse_ortho(&full);
        if pts.len() >= 2 && !polyline_hits_interior(&pts, &solids) {
            return Some(pts);
        }
    }

    // Last resort: the preferred 50/50 even if it clips — a visible
    // wire beats a missing one.
    tiers
        .into_iter()
        .find_map(|t| t.cands.into_iter().next())
        .map(|raw| collapse_ortho(&raw))
        .filter(|pts| pts.len() >= 2)
}

/// Fillet every corner of an orthogonal polyline (File Atlas PCB-trace style).
pub fn filleted_polyline(pts: &[[f32; 2]], radius: f32) -> Vec<PathCmd> {
    if pts.len() < 2 {
        return Vec::new();
    }
    let mut cmds = vec![PathCmd::Move(pts[0])];
    if pts.len() == 2 {
        cmds.push(PathCmd::Line(pts[1]));
        return cmds;
    }
    let mut cursor = pts[0];
    for i in 1..pts.len() {
        let cur = pts[i];
        if i + 1 < pts.len() {
            let next = pts[i + 1];
            let in_v = sub(cur, cursor);
            let out_v = sub(next, cur);
            let in_len = len(in_v);
            let out_len = len(out_v);
            let r = radius.min(in_len * 0.5).min(out_len * 0.5);
            if r < 0.5 || in_len < 0.5 || out_len < 0.5 {
                if in_len >= 0.5 {
                    cmds.push(PathCmd::Line(cur));
                }
                cursor = cur;
                continue;
            }
            let a = add(cur, scale(norm(in_v), -r));
            let b = add(cur, scale(norm(out_v), r));
            cmds.push(PathCmd::Line(a));
            cmds.push(PathCmd::Cubic {
                c1: cur,
                c2: cur,
                to: b,
            });
            cursor = b;
        } else {
            cmds.push(PathCmd::Line(cur));
        }
    }
    cmds
}

/// Nearest point on an orthogonal polyline (osnap Near / Perp).
pub fn nearest_on_polyline(pts: &[[f32; 2]], p: [f32; 2]) -> Option<[f32; 2]> {
    if pts.len() < 2 {
        return pts.first().copied();
    }
    let mut best: Option<(f32, [f32; 2])> = None;
    for w in pts.windows(2) {
        let q = project_seg(w[0], w[1], p);
        let d = dist2(q, p);
        match best {
            None => best = Some((d, q)),
            Some((bd, _)) if d < bd => best = Some((d, q)),
            _ => {}
        }
    }
    best.map(|(_, q)| q)
}

// ---------- resolve / stubs ----------

fn resolve_end(
    end: &ConnectorEnd,
    host_of: &impl Fn(NodeId) -> Option<WireHost>,
) -> Option<([f32; 2], Option<[f32; 2]>)> {
    match end {
        ConnectorEnd::Anchored { node, side, t } => {
            let host = host_of(*node)?;
            Some((host.anchor(*side, *t), Some(host.outward(*side, *t))))
        }
        ConnectorEnd::Free { point } => Some((*point, None)),
    }
}

fn end_host(end: &ConnectorEnd) -> Option<NodeId> {
    match end {
        ConnectorEnd::Anchored { node, .. } => Some(*node),
        ConnectorEnd::Free { .. } => None,
    }
}

#[derive(Clone, Copy)]
struct Solid {
    rect: WorldRect,
    rot: f32,
    ellipse: bool,
}

fn host_solids(hosts: &[Solid], others: &[Solid]) -> Vec<Solid> {
    let mut solids = Vec::with_capacity(hosts.len() + others.len());
    solids.extend_from_slice(hosts);
    solids.extend_from_slice(others);
    solids
}

fn solid_aabb(s: &Solid) -> WorldRect {
    if s.rot.abs() < f32::EPSILON {
        return s.rect;
    }
    let cs = s.rect.corners_rotated(s.rot);
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in cs {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    WorldRect::new(
        min_x,
        min_y,
        (max_x - min_x).max(1.0),
        (max_y - min_y).max(1.0),
    )
}

fn fan_anchor(
    end: &ConnectorEnd,
    p: [f32; 2],
    dir: Option<[f32; 2]>,
    along: f32,
    host_of: &impl Fn(NodeId) -> Option<WireHost>,
) -> [f32; 2] {
    if along.abs() < EPS {
        return p;
    }
    let tan = match dir {
        Some(d) => {
            let n = norm(d);
            [n[1], -n[0]]
        }
        None => [1.0, 0.0],
    };
    let q = [p[0] + tan[0] * along, p[1] + tan[1] * along];
    let ConnectorEnd::Anchored { node, side, .. } = *end else {
        return q;
    };
    let Some(host) = host_of(node) else {
        return q;
    };
    if !side.is_box() || host.is_open() {
        return q;
    }
    let a = host.anchor(side, 0.0);
    let b = host.anchor(side, 1.0);
    project_seg(a, b, q)
}

fn escape_stub(anchor: [f32; 2], dir: Option<[f32; 2]>, solids: &[Solid]) -> [f32; 2] {
    let Some(dir) = dir else {
        return anchor;
    };
    // Leave along the true outward so a rotated edge does not immediately
    // re-enter the OBB. The first segment may be diagonal (allowed).
    let n = norm(dir);
    if len(n) < EPS {
        return anchor;
    }
    let mut d = ORTHO_CLEARANCE;
    while d >= 1.0 {
        let p = [anchor[0] + n[0] * d, anchor[1] + n[1] * d];
        if !solids.iter().any(|o| strictly_inside_solid(o, p)) {
            return p;
        }
        d *= 0.5;
    }
    for i in 1..=48 {
        let d = i as f32;
        let p = [anchor[0] + n[0] * d, anchor[1] + n[1] * d];
        if !solids.iter().any(|o| strictly_inside_solid(o, p)) {
            return p;
        }
    }
    [
        anchor[0] + n[0] * ORTHO_CLEARANCE,
        anchor[1] + n[1] * ORTHO_CLEARANCE,
    ]
}

// ---------- candidates ----------

/// World units of axis dominance before we flip the 50/50 orientation.
/// Stops the path twitching when dx ≈ dy during a drag.
const AXIS_LOCK: f32 = 8.0;

struct PathTier {
    /// Take the first clear candidate (dominance order). Used for the
    /// 50/50 pair so a vertical run does not flip to HVH on a cost tie.
    first_wins: bool,
    cands: Vec<Vec<[f32; 2]>>,
}

/// Ordered families. The first family with a clear member wins — stairs
/// never compete with a legal 50/50 on cost (they share Manhattan length).
fn path_tiers(
    s_anchor: [f32; 2],
    s_stub: [f32; 2],
    e_anchor: [f32; 2],
    e_stub: [f32; 2],
    solids: &[Solid],
    rail: f32,
    dir_a: Option<[f32; 2]>,
    dir_b: Option<[f32; 2]>,
) -> Vec<PathTier> {
    let mid_x = trunk_between((s_stub[0] + e_stub[0]) * 0.5 + rail, s_stub[0], e_stub[0]);
    let mid_y = trunk_between((s_stub[1] + e_stub[1]) * 0.5 + rail, s_stub[1], e_stub[1]);
    let vhv = vec![
        s_anchor,
        s_stub,
        [s_stub[0], mid_y],
        [e_stub[0], mid_y],
        e_stub,
        e_anchor,
    ];
    let hvh = vec![
        s_anchor,
        s_stub,
        [mid_x, s_stub[1]],
        [mid_x, e_stub[1]],
        e_stub,
        e_anchor,
    ];
    let dx = (e_stub[0] - s_stub[0]).abs();
    let dy = (e_stub[1] - s_stub[1]).abs();
    let leave_vert =
        |d: Option<[f32; 2]>| d.map(|d| d[1].abs() > d[0].abs() + 0.1).unwrap_or(false);
    let leave_horz =
        |d: Option<[f32; 2]>| d.map(|d| d[0].abs() > d[1].abs() + 0.1).unwrap_or(false);
    // When both ends leave vertically, a vertical mid-trunk cannot
    // swallow the stubs — the path reads as a 5-leg stair. Prefer the
    // horizontal trunk so leave + approach collapse into three legs.
    let prefer_vhv = (leave_vert(dir_a) && leave_vert(dir_b))
        || (!(leave_horz(dir_a) && leave_horz(dir_b)) && dy > dx + AXIS_LOCK);
    let mid_jive = if prefer_vhv {
        vec![vhv, hvh]
    } else {
        vec![hvh, vhv]
    };

    let l_shapes = vec![
        vec![s_anchor, s_stub, [e_stub[0], s_stub[1]], e_stub, e_anchor],
        vec![s_anchor, s_stub, [s_stub[0], e_stub[1]], e_stub, e_anchor],
    ];

    let hull = routing_hull(s_stub, e_stub, solids);
    let left = hull.x;
    let right = hull.x + hull.w;
    let top = hull.y;
    let bottom = hull.y + hull.h;
    let wraps = vec![
        vec![
            s_anchor,
            s_stub,
            [right, s_stub[1]],
            [right, e_stub[1]],
            e_stub,
            e_anchor,
        ],
        vec![
            s_anchor,
            s_stub,
            [left, s_stub[1]],
            [left, e_stub[1]],
            e_stub,
            e_anchor,
        ],
        vec![
            s_anchor,
            s_stub,
            [s_stub[0], bottom],
            [e_stub[0], bottom],
            e_stub,
            e_anchor,
        ],
        vec![
            s_anchor,
            s_stub,
            [s_stub[0], top],
            [e_stub[0], top],
            e_stub,
            e_anchor,
        ],
    ];

    let y1 = s_stub[1] * 0.67 + e_stub[1] * 0.33 + rail;
    let y2 = s_stub[1] * 0.33 + e_stub[1] * 0.67 + rail;
    let x1 = s_stub[0] * 0.67 + e_stub[0] * 0.33 + rail;
    let x2 = s_stub[0] * 0.33 + e_stub[0] * 0.67 + rail;
    let stairs = vec![
        vec![
            s_anchor,
            s_stub,
            [s_stub[0], y1],
            [mid_x, y1],
            [mid_x, y2],
            [e_stub[0], y2],
            e_stub,
            e_anchor,
        ],
        vec![
            s_anchor,
            s_stub,
            [x1, s_stub[1]],
            [x1, mid_y],
            [x2, mid_y],
            [x2, e_stub[1]],
            e_stub,
            e_anchor,
        ],
    ];

    let mut tiers = Vec::new();
    if (s_stub[0] - e_stub[0]).abs() <= EPS || (s_stub[1] - e_stub[1]).abs() <= EPS {
        tiers.push(PathTier {
            first_wins: true,
            cands: vec![vec![s_anchor, s_stub, e_stub, e_anchor]],
        });
    }
    tiers.push(PathTier {
        first_wins: true,
        cands: mid_jive,
    });
    tiers.push(PathTier {
        first_wins: false,
        cands: l_shapes,
    });
    tiers.push(PathTier {
        first_wins: false,
        cands: wraps,
    });
    tiers.push(PathTier {
        first_wins: false,
        cands: stairs,
    });
    tiers
}

/// Keep a rail-shifted trunk between the stubs so a tight gap cannot
/// send the vertical (or horizontal) run back through the leave host.
fn trunk_between(mid: f32, a: f32, b: f32) -> f32 {
    let lo = a.min(b);
    let hi = a.max(b);
    let span = hi - lo;
    if span < 2.0 {
        return (a + b) * 0.5;
    }
    let keep = ORTHO_CLEARANCE.min(span * 0.25);
    mid.clamp(lo + keep, hi - keep)
}

fn first_clear(cands: &[Vec<[f32; 2]>], solids: &[Solid]) -> Option<Vec<[f32; 2]>> {
    for raw in cands {
        let pts = collapse_ortho(raw);
        if pts.len() >= 2 && !polyline_hits_interior(&pts, solids) {
            return Some(pts);
        }
    }
    None
}

fn pick_clear(cands: &[Vec<[f32; 2]>], solids: &[Solid]) -> Option<Vec<[f32; 2]>> {
    let mut best: Option<(Cost, Vec<[f32; 2]>)> = None;
    for raw in cands {
        let pts = collapse_ortho(raw);
        if pts.len() < 2 || polyline_hits_interior(&pts, solids) {
            continue;
        }
        let cost = path_cost(&pts);
        match &best {
            None => best = Some((cost, pts)),
            Some((c, _)) if cost < *c => best = Some((cost, pts)),
            _ => {}
        }
    }
    best.map(|(_, pts)| pts)
}

fn routing_hull(s: [f32; 2], e: [f32; 2], solids: &[Solid]) -> WorldRect {
    let mut min_x = s[0].min(e[0]);
    let mut max_x = s[0].max(e[0]);
    let mut min_y = s[1].min(e[1]);
    let mut max_y = s[1].max(e[1]);
    let corridor = inflate(
        WorldRect::new(
            min_x,
            min_y,
            (max_x - min_x).max(1.0),
            (max_y - min_y).max(1.0),
        ),
        ORTHO_CLEARANCE * 3.0,
    );
    for h in solids {
        let r = solid_aabb(h);
        if !rects_overlap(r, corridor) {
            continue;
        }
        min_x = min_x.min(r.x);
        max_x = max_x.max(r.x + r.w);
        min_y = min_y.min(r.y);
        max_y = max_y.max(r.y + r.h);
    }
    inflate(
        WorldRect::new(
            min_x,
            min_y,
            (max_x - min_x).max(1.0),
            (max_y - min_y).max(1.0),
        ),
        ORTHO_CLEARANCE,
    )
}

fn rects_overlap(a: WorldRect, b: WorldRect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

// ---------- Hanan grid ----------

fn hanan_route(start: [f32; 2], end: [f32; 2], solids: &[Solid]) -> Option<Vec<[f32; 2]>> {
    if dist2(start, end) < EPS * EPS {
        return Some(vec![start, end]);
    }
    let mut xs = vec![start[0], end[0]];
    let mut ys = vec![start[1], end[1]];
    for s in solids {
        let r = solid_aabb(s);
        xs.push(r.x);
        xs.push(r.x + r.w);
        ys.push(r.y);
        ys.push(r.y + r.h);
        xs.push(r.x - ORTHO_CLEARANCE);
        xs.push(r.x + r.w + ORTHO_CLEARANCE);
        ys.push(r.y - ORTHO_CLEARANCE);
        ys.push(r.y + r.h + ORTHO_CLEARANCE);
    }
    let xs = unique_coords(xs);
    let ys = unique_coords(ys);
    if xs.len() * ys.len() > GRID_CAP {
        return None;
    }
    let nx = xs.len();
    let ny = ys.len();
    let si = nearest_idx(&xs, start[0]);
    let sj = nearest_idx(&ys, start[1]);
    let ei = nearest_idx(&xs, end[0]);
    let ej = nearest_idx(&ys, end[1]);
    let start_i = si * ny + sj;
    let end_i = ei * ny + ej;

    let blocked = |i: usize, j: usize| -> bool {
        let p = [xs[i], ys[j]];
        if nearly(p, start) || nearly(p, end) {
            return false;
        }
        solids.iter().any(|o| strictly_inside_solid(o, p))
    };
    let edge_ok =
        |a: [f32; 2], b: [f32; 2]| -> bool { !solids.iter().any(|o| segment_hits_solid(a, b, o)) };

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct Cost {
        corners: u64,
        length: u64,
        diag: u64,
        left: u64,
        up: u64,
        first: u8,
        last: u8,
    }
    impl Cost {
        fn step(self, dx: f32, dy: f32) -> Self {
            let euclid = (dx * dx + dy * dy).sqrt();
            let diag = dx.abs() > EPS && dy.abs() > EPS;
            let mut c = self;
            // Diagonals are allowed when they help, but cost 1.5× so a
            // clear orthogonal run still wins (P1.wire.rails).
            let billed = if diag { euclid * 1.5 } else { euclid };
            c.length = c.length.saturating_add(quant(billed));
            if diag {
                c.diag = c.diag.saturating_add(1);
            }
            if dx < -EPS {
                c.left = c.left.saturating_add(quant(-dx));
            }
            if dy < -EPS {
                c.up = c.up.saturating_add(quant(-dy));
            }
            if euclid > EPS {
                let dir = dir_code(dx, dy);
                if c.first == 255 {
                    c.first = dir;
                }
                if c.last != 255 && c.last != dir {
                    c.corners = c.corners.saturating_add(1);
                }
                c.last = dir;
            }
            c
        }
    }

    let n = nx * ny;
    let mut dist = vec![
        Cost {
            corners: u64::MAX,
            length: u64::MAX,
            diag: 0,
            left: 0,
            up: 0,
            first: 255,
            last: 255,
        };
        n
    ];
    let mut prev = vec![None; n];
    let zero = Cost {
        corners: 0,
        length: 0,
        diag: 0,
        left: 0,
        up: 0,
        first: 255,
        last: 255,
    };
    dist[start_i] = zero;

    // 8-neighbor Dijkstra: ortho first, then 45°. Neighbor order is the
    // tie-break when costs compare equal — first parent wins.
    let mut heap = std::collections::BinaryHeap::new();
    heap.push(std::cmp::Reverse((zero, start_i)));
    let deltas = [
        (1isize, 0isize),
        (0, 1),
        (-1, 0),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];

    while let Some(std::cmp::Reverse((cost, idx))) = heap.pop() {
        if cost > dist[idx] {
            continue;
        }
        if idx == end_i {
            break;
        }
        let i = idx / ny;
        let j = idx % ny;
        let a = [xs[i], ys[j]];
        for (di, dj) in deltas {
            let ni = i as isize + di;
            let nj = j as isize + dj;
            if ni < 0 || nj < 0 || ni >= nx as isize || nj >= ny as isize {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            if blocked(ni, nj) {
                continue;
            }
            let b = [xs[ni], ys[nj]];
            if !edge_ok(a, b) {
                continue;
            }
            let nidx = ni * ny + nj;
            let next = cost.step(b[0] - a[0], b[1] - a[1]);
            if next < dist[nidx] {
                dist[nidx] = next;
                prev[nidx] = Some(idx);
                heap.push(std::cmp::Reverse((next, nidx)));
            }
        }
    }

    if dist[end_i].length == u64::MAX {
        return None;
    }
    let mut chain = Vec::new();
    let mut cur = Some(end_i);
    while let Some(idx) = cur {
        let i = idx / ny;
        let j = idx % ny;
        chain.push([xs[i], ys[j]]);
        if idx == start_i {
            break;
        }
        cur = prev[idx];
    }
    chain.reverse();
    if chain.first().copied() != Some(start) {
        chain.insert(0, start);
    }
    if chain.last().copied() != Some(end) {
        chain.push(end);
    }
    Some(collapse_ortho(&chain))
}

// ---------- geometry helpers ----------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Cost {
    corners: u64,
    length: u64,
    diag: u64,
    left: u64,
    up: u64,
    first: u8,
}

fn path_cost(pts: &[[f32; 2]]) -> Cost {
    let mut c = Cost {
        corners: 0,
        length: 0,
        diag: 0,
        left: 0,
        up: 0,
        first: 255,
    };
    let mut last_dir = 255u8;
    for w in pts.windows(2) {
        let dx = w[1][0] - w[0][0];
        let dy = w[1][1] - w[0][1];
        let euclid = (dx * dx + dy * dy).sqrt();
        if euclid <= EPS {
            continue;
        }
        let diag = dx.abs() > EPS && dy.abs() > EPS;
        let billed = if diag { euclid * 1.5 } else { euclid };
        c.length = c.length.saturating_add(quant(billed));
        if diag {
            c.diag = c.diag.saturating_add(1);
        }
        if dx < -EPS {
            c.left = c.left.saturating_add(quant(-dx));
        }
        if dy < -EPS {
            c.up = c.up.saturating_add(quant(-dy));
        }
        let dir = dir_code(dx, dy);
        if c.first == 255 {
            c.first = dir;
        }
        if last_dir != 255 && last_dir != dir {
            c.corners = c.corners.saturating_add(1);
        }
        last_dir = dir;
    }
    c
}

fn dir_code(dx: f32, dy: f32) -> u8 {
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            0
        } else {
            2
        }
    } else if dy >= 0.0 {
        1
    } else {
        3
    }
}

fn quant(v: f32) -> u64 {
    (v.max(0.0) * 100.0).round() as u64
}

fn inflate(r: WorldRect, g: f32) -> WorldRect {
    WorldRect::new(r.x - g, r.y - g, r.w + 2.0 * g, r.h + 2.0 * g)
}

fn strictly_inside(r: WorldRect, p: [f32; 2]) -> bool {
    p[0] > r.x + EPS && p[0] < r.x + r.w - EPS && p[1] > r.y + EPS && p[1] < r.y + r.h - EPS
}

fn unrotate_pt(p: [f32; 2], cx: f32, cy: f32, deg: f32) -> [f32; 2] {
    if deg.abs() < f32::EPSILON {
        return p;
    }
    let (sin, cos) = (-deg).to_radians().sin_cos();
    let dx = p[0] - cx;
    let dy = p[1] - cy;
    [cx + dx * cos - dy * sin, cy + dx * sin + dy * cos]
}

fn strictly_inside_solid(s: &Solid, p: [f32; 2]) -> bool {
    let (cx, cy) = s.rect.center();
    let q = unrotate_pt(p, cx, cy, s.rot);
    if s.ellipse {
        strictly_inside_ellipse(s.rect, q)
    } else {
        strictly_inside(s.rect, q)
    }
}

fn strictly_inside_ellipse(r: WorldRect, p: [f32; 2]) -> bool {
    let (cx, cy) = r.center();
    let rx = (r.w * 0.5 - EPS).max(EPS);
    let ry = (r.h * 0.5 - EPS).max(EPS);
    let nx = (p[0] - cx) / rx;
    let ny = (p[1] - cy) / ry;
    nx * nx + ny * ny < 1.0
}

fn segment_hits_aabb_interior(a: [f32; 2], b: [f32; 2], r: WorldRect) -> bool {
    let minx = r.x + EPS;
    let maxx = r.x + r.w - EPS;
    let miny = r.y + EPS;
    let maxy = r.y + r.h - EPS;
    if maxx <= minx || maxy <= miny {
        return false;
    }
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let mut t0 = 0.0f32;
    let mut t1 = 1.0f32;
    let mut clip = |p: f32, q: f32| -> bool {
        if p.abs() < 1e-9 {
            return q >= 0.0;
        }
        let t = q / p;
        if p < 0.0 {
            if t > t1 {
                return false;
            }
            if t > t0 {
                t0 = t;
            }
        } else {
            if t < t0 {
                return false;
            }
            if t < t1 {
                t1 = t;
            }
        }
        true
    };
    if !clip(-dx, a[0] - minx) {
        return false;
    }
    if !clip(dx, maxx - a[0]) {
        return false;
    }
    if !clip(-dy, a[1] - miny) {
        return false;
    }
    if !clip(dy, maxy - a[1]) {
        return false;
    }
    t1 > t0 + 1e-5
}

fn segment_hits_solid(a: [f32; 2], b: [f32; 2], s: &Solid) -> bool {
    let (cx, cy) = s.rect.center();
    let a = unrotate_pt(a, cx, cy, s.rot);
    let b = unrotate_pt(b, cx, cy, s.rot);
    if s.ellipse {
        segment_hits_ellipse_interior(a, b, s.rect)
    } else {
        segment_hits_aabb_interior(a, b, s.rect)
    }
}

fn segment_hits_ellipse_interior(a: [f32; 2], b: [f32; 2], r: WorldRect) -> bool {
    let (cx, cy) = r.center();
    let rx = (r.w * 0.5 - EPS).max(EPS);
    let ry = (r.h * 0.5 - EPS).max(EPS);
    let a = [(a[0] - cx) / rx, (a[1] - cy) / ry];
    let b = [(b[0] - cx) / rx, (b[1] - cy) / ry];
    if a[0] * a[0] + a[1] * a[1] < 1.0 || b[0] * b[0] + b[1] * b[1] < 1.0 {
        return true;
    }
    let ab = [b[0] - a[0], b[1] - a[1]];
    let den = ab[0] * ab[0] + ab[1] * ab[1];
    if den < 1e-12 {
        return false;
    }
    let t = (-(a[0] * ab[0] + a[1] * ab[1]) / den).clamp(0.0, 1.0);
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    q[0] * q[0] + q[1] * q[1] < 1.0
}

fn polyline_hits_interior(pts: &[[f32; 2]], solids: &[Solid]) -> bool {
    pts.windows(2)
        .any(|w| solids.iter().any(|o| segment_hits_solid(w[0], w[1], o)))
}

fn collapse_ortho(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut cleaned: Vec<[f32; 2]> = Vec::new();
    for &p in pts {
        if cleaned
            .last()
            .is_none_or(|q| (p[0] - q[0]).abs() > EPS || (p[1] - q[1]).abs() > EPS)
        {
            cleaned.push(p);
        }
    }
    if cleaned.len() < 3 {
        return cleaned;
    }
    let mut out = vec![cleaned[0]];
    for i in 1..cleaned.len() - 1 {
        let a = *out.last().unwrap();
        let b = cleaned[i];
        let c = cleaned[i + 1];
        let ab = sub(b, a);
        let bc = sub(c, b);
        let cross = ab[0] * bc[1] - ab[1] * bc[0];
        let dot = ab[0] * bc[0] + ab[1] * bc[1];
        if cross.abs() < 0.05 && dot >= -0.05 {
            continue;
        }
        out.push(b);
    }
    out.push(*cleaned.last().unwrap());
    out
}

fn unique_coords(mut v: Vec<f32>) -> Vec<f32> {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<f32> = Vec::new();
    for x in v {
        if out.last().is_none_or(|p| (x - *p).abs() > 0.25) {
            out.push(x);
        }
    }
    out
}

fn nearest_idx(xs: &[f32], x: f32) -> usize {
    xs.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - x).abs().partial_cmp(&(*b - x).abs()).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn polyline_aabb(pts: &[[f32; 2]]) -> WorldRect {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for p in pts {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    if !min_x.is_finite() {
        return WorldRect::new(0.0, 0.0, 1.0, 1.0);
    }
    WorldRect::new(
        min_x,
        min_y,
        (max_x - min_x).max(1.0),
        (max_y - min_y).max(1.0),
    )
}

fn polyline_midpoint(pts: &[[f32; 2]]) -> [f32; 2] {
    if pts.is_empty() {
        return [0.0, 0.0];
    }
    if pts.len() == 1 {
        return pts[0];
    }
    let mut total = 0.0f32;
    for w in pts.windows(2) {
        total += len(sub(w[1], w[0]));
    }
    if total < EPS {
        return pts[0];
    }
    let mut walk = total * 0.5;
    for w in pts.windows(2) {
        let d = len(sub(w[1], w[0]));
        if walk <= d {
            let t = if d < EPS { 0.0 } else { walk / d };
            return lerp(w[0], w[1], t);
        }
        walk -= d;
    }
    *pts.last().unwrap()
}

fn first_dir(pts: &[[f32; 2]]) -> [f32; 2] {
    for w in pts.windows(2) {
        let v = sub(w[1], w[0]);
        if len(v) > EPS {
            return norm(v);
        }
    }
    [0.0, 0.0]
}

fn last_dir_inward(pts: &[[f32; 2]]) -> [f32; 2] {
    for w in pts.windows(2).rev() {
        let v = sub(w[0], w[1]);
        if len(v) > EPS {
            return norm(v);
        }
    }
    [0.0, 0.0]
}

fn project_seg(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let ab = sub(b, a);
    let l2 = dist2(a, b);
    if l2 < EPS * EPS {
        return a;
    }
    let t = ((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2;
    lerp(a, b, t.clamp(0.0, 1.0))
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn scale(a: [f32; 2], s: f32) -> [f32; 2] {
    [a[0] * s, a[1] * s]
}
fn len(a: [f32; 2]) -> f32 {
    (a[0] * a[0] + a[1] * a[1]).sqrt()
}
fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let d = sub(a, b);
    d[0] * d[0] + d[1] * d[1]
}
fn norm(a: [f32; 2]) -> [f32; 2] {
    let l = len(a);
    if l < EPS {
        [0.0, 0.0]
    } else {
        [a[0] / l, a[1] / l]
    }
}
fn lerp(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
fn nearly(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() <= EPS && (a[1] - b[1]).abs() <= EPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Side;

    fn host_of(a: WorldRect, b: WorldRect) -> impl Fn(NodeId) -> Option<WireHost> {
        move |id| match id.0 {
            1 => Some(WireHost::from_rect(a)),
            2 => Some(WireHost::from_rect(b)),
            _ => None,
        }
    }

    fn bottom_to_bottom(_a: WorldRect, _b: WorldRect) -> (ConnectorEnd, ConnectorEnd) {
        (
            ConnectorEnd::Anchored {
                node: NodeId(1),
                side: Side::Bottom,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: NodeId(2),
                side: Side::Bottom,
                t: 0.5,
            },
        )
    }

    fn obstacles(a: WorldRect, b: WorldRect) -> Vec<WireObstacle> {
        vec![
            WireObstacle {
                id: NodeId(1),
                rect: a,
                rotation_deg: 0.0,
                ellipse: false,
            },
            WireObstacle {
                id: NodeId(2),
                rect: b,
                rotation_deg: 0.0,
                ellipse: false,
            },
        ]
    }

    fn as_solids(hosts: &[WorldRect]) -> Vec<Solid> {
        hosts
            .iter()
            .map(|r| Solid {
                rect: *r,
                rot: 0.0,
                ellipse: false,
            })
            .collect()
    }

    fn interior_hits(pts: &[[f32; 2]], hosts: &[WorldRect]) -> bool {
        polyline_hits_interior(pts, &as_solids(hosts))
    }

    #[test]
    fn stacked_bottom_to_bottom_wraps_hosts() {
        // B directly above A. Wire from A's base to B's base must leave A
        // downward and wrap around — never climb through either rect.
        let a = WorldRect::new(0.0, 100.0, 80.0, 60.0);
        let b = WorldRect::new(0.0, 0.0, 80.0, 60.0);
        let (ea, eb) = bottom_to_bottom(a, b);
        let pts = connector_ortho_path(
            &ea,
            &eb,
            host_of(a, b),
            &obstacles(a, b),
            OrthoLane::default(),
        )
        .unwrap();
        assert!(
            !interior_hits(&pts, &[a, b]),
            "ortho path crossed a host: {pts:?}"
        );
        assert_eq!(pts.first().copied(), Some([40.0, 160.0]));
        assert_eq!(pts.last().copied(), Some([40.0, 60.0]));
        // Must travel to the right of both hosts (tie-break prefers right).
        let max_x = pts.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        assert!(
            max_x > 80.0,
            "expected a wrap past the right edge, got max_x={max_x} {pts:?}"
        );
    }

    #[test]
    fn equal_manhattan_prefers_right_then_down() {
        let a = ConnectorEnd::Free { point: [0.0, 0.0] };
        let b = ConnectorEnd::Free {
            point: [40.0, 40.0],
        };
        let pts = connector_ortho_path(&a, &b, |_| None, &[], OrthoLane::default()).unwrap();
        // Equal travel: 50/50 HVH (vertical trunk at mid_x) — not an L and
        // not a stair. Right-first when the axes are tied.
        assert_eq!(
            pts,
            vec![[0.0, 0.0], [20.0, 0.0], [20.0, 40.0], [40.0, 40.0]]
        );
    }

    #[test]
    fn equal_wrap_around_block_prefers_bottom() {
        // Horizontal run with a centered blocker: top and bottom wraps are
        // the same length. Tie-break picks the bottom path.
        let start = ConnectorEnd::Free { point: [0.0, 0.0] };
        let end = ConnectorEnd::Free { point: [40.0, 0.0] };
        let block = WorldRect::new(16.0, -20.0, 8.0, 40.0);
        let pts = connector_ortho_path(
            &start,
            &end,
            |_| None,
            &[WireObstacle {
                id: NodeId(9),
                rect: block,
                rotation_deg: 0.0,
                ellipse: false,
            }],
            OrthoLane::default(),
        )
        .unwrap();
        assert!(
            !interior_hits(&pts, &[block]),
            "path hit the blocker: {pts:?}"
        );
        let max_y = pts.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let min_y = pts.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
        assert!(
            max_y > 0.5 && min_y > -8.0,
            "expected the bottom wrap, got {pts:?}"
        );
    }

    #[test]
    fn bezier_route_ignores_obstacles() {
        let a = ConnectorEnd::Free { point: [0.0, 0.0] };
        let b = ConnectorEnd::Free {
            point: [100.0, 0.0],
        };
        let path = connector_route(
            &a,
            &b,
            |_| None,
            WireRouting::Bezier,
            &[],
            OrthoLane::default(),
        )
        .unwrap();
        assert!(matches!(path, ConnectorPath::Bezier(_)));
    }

    #[test]
    fn fillet_inserts_cubics_at_corners() {
        let pts = [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0]];
        let cmds = filleted_polyline(&pts, 6.0);
        assert!(cmds.iter().any(|c| matches!(c, PathCmd::Cubic { .. })));
    }

    fn rect_node(id: u64, rect: WorldRect, rot: f32) -> crate::scene::Node {
        use crate::scene::{Node, NodeKind, ShapeKind, ShapeNode, Stroke};
        Node {
            id: NodeId(id),
            rect,
            rotation_deg: rot,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            kind: NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: None,
                stroke: Stroke::default(),
                corner: Default::default(),
                flip: false,
                path: None,
            }),
        }
    }

    fn conn_node(id: u64, a: ConnectorEnd, b: ConnectorEnd) -> crate::scene::Node {
        use crate::scene::{ConnectorNode, Node, NodeKind, Stroke, WireDisplay};
        Node {
            id: NodeId(id),
            rect: WorldRect::new(0.0, 0.0, 1.0, 1.0),
            rotation_deg: 0.0,
            opacity: 1.0,
            locked: false,
            hidden: false,
            group: None,
            clip: None,
            kind: NodeKind::Connector(ConnectorNode {
                a,
                b,
                stroke: Stroke::default(),
                arrow_a: false,
                arrow_b: false,
                label: None,
                display: WireDisplay::Default,
            }),
        }
    }

    #[test]
    fn sibling_wires_to_one_host_get_offset_rails() {
        let mut scene = crate::scene::Scene::default();
        scene.nodes = vec![
            rect_node(1, WorldRect::new(0.0, 0.0, 80.0, 40.0), 0.0),
            rect_node(2, WorldRect::new(120.0, 0.0, 80.0, 40.0), 0.0),
            rect_node(3, WorldRect::new(40.0, 160.0, 120.0, 50.0), 0.0),
            conn_node(
                10,
                ConnectorEnd::Anchored {
                    node: NodeId(1),
                    side: Side::Bottom,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: NodeId(3),
                    side: Side::Top,
                    t: 0.5,
                },
            ),
            conn_node(
                11,
                ConnectorEnd::Anchored {
                    node: NodeId(2),
                    side: Side::Bottom,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: NodeId(3),
                    side: Side::Top,
                    t: 0.5,
                },
            ),
        ];
        let lanes = scene_ortho_lanes(&scene);
        let la = lanes[&NodeId(10)];
        let lb = lanes[&NodeId(11)];
        assert!(
            (la.end_along - lb.end_along).abs() > 1.0 || (la.rail - lb.rail).abs() > 1.0,
            "shared dest must fan or offset rails: {la:?} {lb:?}"
        );
        let pa = connector_route_in_scene(
            &scene,
            Some(NodeId(10)),
            &ConnectorEnd::Anchored {
                node: NodeId(1),
                side: Side::Bottom,
                t: 0.5,
            },
            &ConnectorEnd::Anchored {
                node: NodeId(3),
                side: Side::Top,
                t: 0.5,
            },
            WireRouting::Orthogonal,
        )
        .unwrap();
        let pb = connector_route_in_scene(
            &scene,
            Some(NodeId(11)),
            &ConnectorEnd::Anchored {
                node: NodeId(2),
                side: Side::Bottom,
                t: 0.5,
            },
            &ConnectorEnd::Anchored {
                node: NodeId(3),
                side: Side::Top,
                t: 0.5,
            },
            WireRouting::Orthogonal,
        )
        .unwrap();
        assert_ne!(
            pa.points(),
            pb.points(),
            "bundled ortho wires must not stack: {pa:?} {pb:?}"
        );
    }

    #[test]
    fn stacked_dests_do_not_cross_on_a_shared_right_port() {
        // Upper dest gets the higher connector id — id-order would cross.
        let mut scene = crate::scene::Scene::default();
        scene.nodes = vec![
            rect_node(1, WorldRect::new(0.0, 80.0, 80.0, 80.0), 0.0),
            rect_node(2, WorldRect::new(220.0, 0.0, 80.0, 40.0), 0.0),
            rect_node(3, WorldRect::new(220.0, 200.0, 80.0, 40.0), 0.0),
            conn_node(
                20,
                ConnectorEnd::Anchored {
                    node: NodeId(1),
                    side: Side::Right,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: NodeId(2),
                    side: Side::Left,
                    t: 0.5,
                },
            ),
            conn_node(
                10,
                ConnectorEnd::Anchored {
                    node: NodeId(1),
                    side: Side::Right,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: NodeId(3),
                    side: Side::Left,
                    t: 0.5,
                },
            ),
        ];
        let lanes = scene_ortho_lanes(&scene);
        let to_upper = lanes[&NodeId(20)];
        let to_lower = lanes[&NodeId(10)];
        assert!(
            to_upper.start_along > 0.0,
            "upper dest must take the upward fan, not id-order: {to_upper:?}"
        );
        assert!(
            to_lower.start_along < 0.0,
            "lower dest must take the downward fan: {to_lower:?}"
        );
        assert!(
            to_upper.rail > 0.0 && to_lower.rail < 0.0,
            "rail sign follows the dest, not the connector id: {to_upper:?} {to_lower:?}"
        );
    }

    #[test]
    fn dest_inside_the_lock_band_stays_on_the_port_mid() {
        let mut scene = crate::scene::Scene::default();
        scene.nodes = vec![
            rect_node(1, WorldRect::new(0.0, 0.0, 80.0, 80.0), 0.0),
            rect_node(2, WorldRect::new(200.0, 16.0, 80.0, 40.0), 0.0),
            conn_node(
                10,
                ConnectorEnd::Anchored {
                    node: NodeId(1),
                    side: Side::Right,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: NodeId(2),
                    side: Side::Left,
                    t: 0.5,
                },
            ),
            conn_node(
                11,
                ConnectorEnd::Anchored {
                    node: NodeId(1),
                    side: Side::Right,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: NodeId(2),
                    side: Side::Top,
                    t: 0.5,
                },
            ),
        ];
        // Two wires share A's right, but dest 2's left is only 4px above
        // A's right-port centre (y=40 vs y=36) — inside LANE_LOCK.
        let lanes = scene_ortho_lanes(&scene);
        let near = lanes[&NodeId(10)];
        assert!(
            near.start_along.abs() < 0.5,
            "dest inside the lock band must not pick a fan side: {near:?}"
        );
    }

    #[test]
    fn rail_cannot_push_the_trunk_behind_the_stub() {
        let a = ConnectorEnd::Free { point: [0.0, 0.0] };
        let b = ConnectorEnd::Free {
            point: [24.0, 40.0],
        };
        let pts = connector_ortho_path(
            &a,
            &b,
            |_| None,
            &[],
            OrthoLane {
                rail: 80.0,
                ..OrthoLane::default()
            },
        )
        .unwrap();
        assert!(
            pts.iter().all(|p| p[0] >= -0.5 && p[0] <= 24.5),
            "rail must not send the trunk behind a stub: {pts:?}"
        );
    }

    #[test]
    fn rotated_host_is_not_crossed() {
        let a = WorldRect::new(0.0, 0.0, 100.0, 40.0);
        let b = WorldRect::new(80.0, 80.0, 100.0, 40.0);
        let ea = ConnectorEnd::Anchored {
            node: NodeId(1),
            side: Side::Bottom,
            t: 0.5,
        };
        let eb = ConnectorEnd::Anchored {
            node: NodeId(2),
            side: Side::Top,
            t: 0.5,
        };
        let host_of = |id: NodeId| match id.0 {
            1 => Some({
                let mut h = WireHost::from_rect(a);
                h.rotation_deg = 35.0;
                h
            }),
            2 => Some({
                let mut h = WireHost::from_rect(b);
                h.rotation_deg = 35.0;
                h
            }),
            _ => None,
        };
        let obs = vec![
            WireObstacle {
                id: NodeId(1),
                rect: a,
                rotation_deg: 35.0,
                ellipse: false,
            },
            WireObstacle {
                id: NodeId(2),
                rect: b,
                rotation_deg: 35.0,
                ellipse: false,
            },
        ];
        let pts = connector_ortho_path(&ea, &eb, host_of, &obs, OrthoLane::default()).unwrap();
        let solids = [
            Solid {
                rect: a,
                rot: 35.0,
                ellipse: false,
            },
            Solid {
                rect: b,
                rot: 35.0,
                ellipse: false,
            },
        ];
        assert!(
            !polyline_hits_interior(&pts, &solids),
            "rotated hosts were crossed: {pts:?}"
        );
    }

    #[test]
    fn clear_corridor_jives_near_the_midpoint() {
        let a = ConnectorEnd::Free { point: [0.0, 0.0] };
        let b = ConnectorEnd::Free { point: [0.0, 80.0] };
        let pts = connector_ortho_path(&a, &b, |_| None, &[], OrthoLane::default()).unwrap();
        // Vertical run — a single segment, or a midpoint jive that stays on x=0.
        assert!(
            pts.iter().all(|p| (p[0] - 0.0).abs() < 0.5),
            "clear vertical corridor should stay on the line: {pts:?}"
        );
    }

    fn corners(pts: &[[f32; 2]]) -> usize {
        pts.len().saturating_sub(2)
    }

    #[test]
    fn vertical_dominance_uses_a_horizontal_mid_trunk() {
        let a = ConnectorEnd::Free { point: [0.0, 0.0] };
        let b = ConnectorEnd::Free {
            point: [24.0, 80.0],
        };
        let pts = connector_ortho_path(&a, &b, |_| None, &[], OrthoLane::default()).unwrap();
        assert_eq!(
            pts,
            vec![[0.0, 0.0], [0.0, 40.0], [24.0, 40.0], [24.0, 80.0]],
            "50/50 V-H-V, not a stair: {pts:?}"
        );
        assert_eq!(corners(&pts), 2);
    }

    #[test]
    fn offset_ellipses_take_three_leg_mid_jive() {
        // Screenshot case: large oval bottom-left, small oval top-right.
        // The AABB of each oval is fat at the corners — a box collider
        // would reject the 50/50 and stair around it.
        let a = WorldRect::new(40.0, 220.0, 200.0, 100.0);
        let b = WorldRect::new(280.0, 40.0, 80.0, 36.0);
        let ea = ConnectorEnd::Anchored {
            node: NodeId(1),
            side: Side::Top,
            t: 0.5,
        };
        let eb = ConnectorEnd::Anchored {
            node: NodeId(2),
            side: Side::Bottom,
            t: 0.5,
        };
        let host_of = |id: NodeId| match id.0 {
            1 => Some(WireHost::from_ellipse(a)),
            2 => Some(WireHost::from_ellipse(b)),
            _ => None,
        };
        let obs = vec![
            WireObstacle {
                id: NodeId(1),
                rect: a,
                rotation_deg: 0.0,
                ellipse: true,
            },
            WireObstacle {
                id: NodeId(2),
                rect: b,
                rotation_deg: 0.0,
                ellipse: true,
            },
        ];
        let pts = connector_ortho_path(&ea, &eb, host_of, &obs, OrthoLane::default()).unwrap();
        assert_eq!(
            pts,
            vec![
                [140.0, 220.0],
                [140.0, 148.0],
                [320.0, 148.0],
                [320.0, 76.0]
            ],
            "top-to-bottom on a clear pair is V-H-V through mid_y, not a stair"
        );
        assert_eq!(corners(&pts), 2);
    }

    #[test]
    fn ellipse_aabb_corner_is_not_solid() {
        let r = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        let oval = Solid {
            rect: r,
            rot: 0.0,
            ellipse: true,
        };
        let boxy = Solid {
            rect: r,
            rot: 0.0,
            ellipse: false,
        };
        // Near the top-right AABB corner, outside the oval.
        let a = [190.0, 8.0];
        let b = [190.0, -20.0];
        assert!(
            segment_hits_solid(a, b, &boxy),
            "control: the box collider must see the corner"
        );
        assert!(
            !segment_hits_solid(a, b, &oval),
            "ellipse collider must ignore the AABB corner"
        );
    }
}
