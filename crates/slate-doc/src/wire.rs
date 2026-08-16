//! Derived connector geometry: the bezier span and an obstacle-aware
//! orthogonal (PCB-trace) router.
//!
//! Geometry is never stored — both interpreters (the egui board painter and
//! the artifact writer) call [`connector_route`] with the current node rects
//! and the session [`WireRouting`]. Orthogonal paths wrap host AABBs instead
//! of crossing them. Equal-length ties prefer the right-hand path, then the
//! bottom path (screen axes: +x right, +y down).

use crate::scene::{
    connector_aabb, connector_bezier, ConnectorBezier, ConnectorEnd, ConnectorNode, NodeKind,
    Scene, Side, WorldRect,
};
use crate::NodeId;
use serde::{Deserialize, Serialize};

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

/// Visible non-connector node rects — the obstacle set for orthogonal routing.
pub fn scene_wire_obstacles(scene: &Scene) -> Vec<(NodeId, WorldRect)> {
    scene
        .nodes
        .iter()
        .filter(|n| !n.hidden && !matches!(n.kind, NodeKind::Connector(_)))
        .map(|n| (n.id, n.rect))
        .collect()
}

/// Derived curve for the current routing. `None` when an anchored node is
/// missing from `rect_of`.
pub fn connector_route(
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    rect_of: impl Fn(NodeId) -> Option<WorldRect>,
    routing: WireRouting,
    obstacles: &[(NodeId, WorldRect)],
) -> Option<ConnectorPath> {
    match routing {
        WireRouting::Bezier => connector_bezier(a, b, rect_of).map(ConnectorPath::Bezier),
        WireRouting::Orthogonal => {
            connector_ortho_path(a, b, rect_of, obstacles).map(ConnectorPath::Orthogonal)
        }
    }
}

/// AABB of the derived route (connector `Node.rect` stays equal to this).
pub fn connector_aabb_routed(
    conn: &ConnectorNode,
    rect_of: impl Fn(NodeId) -> Option<WorldRect>,
    routing: WireRouting,
    obstacles: &[(NodeId, WorldRect)],
) -> Option<WorldRect> {
    match routing {
        WireRouting::Bezier => connector_aabb(conn, rect_of),
        WireRouting::Orthogonal => connector_ortho_path(&conn.a, &conn.b, rect_of, obstacles)
            .map(|pts| polyline_aabb(&pts)),
    }
}

/// Axis-aligned shortest path that leaves each anchored end perpendicular
/// to its side and wraps host interiors. Tie-break: right, then bottom.
pub fn connector_ortho_path(
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    rect_of: impl Fn(NodeId) -> Option<WorldRect>,
    obstacles: &[(NodeId, WorldRect)],
) -> Option<Vec<[f32; 2]>> {
    let (p0, side_a) = resolve_end(a, &rect_of)?;
    let (p3, side_b) = resolve_end(b, &rect_of)?;

    let host_ids = [end_host(a), end_host(b)];
    let hosts: Vec<WorldRect> = host_ids
        .into_iter()
        .flatten()
        .filter_map(|id| rect_of(id))
        .collect();
    let others: Vec<WorldRect> = obstacles
        .iter()
        .filter(|(id, _)| !host_ids.iter().flatten().any(|h| *h == *id))
        .map(|(_, r)| *r)
        .collect();

    let solids = host_solids(&hosts, &others);
    let s_stub = escape_stub(p0, side_a, &solids);
    let e_stub = escape_stub(p3, side_b, &solids);

    let mut best: Option<(Cost, Vec<[f32; 2]>)> = None;
    let consider = |best: &mut Option<(Cost, Vec<[f32; 2]>)>, raw: Vec<[f32; 2]>| {
        let pts = collapse_ortho(&raw);
        if pts.len() < 2 {
            return;
        }
        if polyline_hits_interior(&pts, &solids) {
            return;
        }
        let cost = path_cost(&pts);
        match best {
            None => *best = Some((cost, pts)),
            Some((c, _)) if cost < *c => *best = Some((cost, pts)),
            _ => {}
        }
    };

    for cand in candidate_paths(p0, s_stub, p3, e_stub, &hosts) {
        consider(&mut best, cand);
    }

    if best.is_none() {
        if let Some(pts) = hanan_route(s_stub, e_stub, &solids) {
            let mut full = vec![p0];
            full.extend(pts);
            full.push(p3);
            consider(&mut best, full);
        }
    }

    if let Some((_, pts)) = best {
        return Some(pts);
    }

    // Last resort: the shortest candidate even if it clips — a visible
    // wire beats a missing one. Prefer right / bottom among equals.
    let mut fallback: Option<(Cost, Vec<[f32; 2]>)> = None;
    for cand in candidate_paths(p0, s_stub, p3, e_stub, &hosts) {
        let pts = collapse_ortho(&cand);
        if pts.len() < 2 {
            continue;
        }
        let cost = path_cost(&pts);
        match fallback {
            None => fallback = Some((cost, pts)),
            Some((c, _)) if cost < c => fallback = Some((cost, pts)),
            _ => {}
        }
    }
    fallback.map(|(_, pts)| pts)
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
    rect_of: &impl Fn(NodeId) -> Option<WorldRect>,
) -> Option<([f32; 2], Option<Side>)> {
    match end {
        ConnectorEnd::Anchored { node, side, t } => {
            let rect = rect_of(*node)?;
            Some((
                crate::scene::connector_anchor_point(rect, *side, *t),
                Some(*side),
            ))
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

fn host_solids(hosts: &[WorldRect], others: &[WorldRect]) -> Vec<WorldRect> {
    let mut solids = Vec::with_capacity(hosts.len() + others.len());
    solids.extend_from_slice(hosts);
    for r in others {
        solids.push(inflate(*r, ORTHO_CLEARANCE));
    }
    solids
}

fn escape_stub(anchor: [f32; 2], side: Option<Side>, solids: &[WorldRect]) -> [f32; 2] {
    let Some(side) = side else {
        return anchor;
    };
    let n = side.normal();
    let mut d = ORTHO_CLEARANCE;
    while d >= 1.0 {
        let p = [anchor[0] + n[0] * d, anchor[1] + n[1] * d];
        if !solids.iter().any(|o| strictly_inside(*o, p)) {
            return p;
        }
        d *= 0.5;
    }
    for i in 1..=24 {
        let d = i as f32;
        let p = [anchor[0] + n[0] * d, anchor[1] + n[1] * d];
        if !solids.iter().any(|o| strictly_inside(*o, p)) {
            return p;
        }
    }
    [anchor[0] + n[0], anchor[1] + n[1]]
}

// ---------- candidates ----------

fn candidate_paths(
    s_anchor: [f32; 2],
    s_stub: [f32; 2],
    e_anchor: [f32; 2],
    e_stub: [f32; 2],
    hosts: &[WorldRect],
) -> Vec<Vec<[f32; 2]>> {
    let mut out = Vec::new();
    // HVH / VHV between stubs.
    out.push(vec![
        s_anchor,
        s_stub,
        [e_stub[0], s_stub[1]],
        e_stub,
        e_anchor,
    ]);
    out.push(vec![
        s_anchor,
        s_stub,
        [s_stub[0], e_stub[1]],
        e_stub,
        e_anchor,
    ]);

    let hull = routing_hull(s_stub, e_stub, hosts);
    let left = hull.x;
    let right = hull.x + hull.w;
    let top = hull.y;
    let bottom = hull.y + hull.h;

    // Four wraps around the local hull — the stacked bottom→node-above
    // case uses the right or left lane, then the stub-y of the upper host.
    out.push(vec![
        s_anchor,
        s_stub,
        [right, s_stub[1]],
        [right, e_stub[1]],
        e_stub,
        e_anchor,
    ]);
    out.push(vec![
        s_anchor,
        s_stub,
        [left, s_stub[1]],
        [left, e_stub[1]],
        e_stub,
        e_anchor,
    ]);
    out.push(vec![
        s_anchor,
        s_stub,
        [s_stub[0], bottom],
        [e_stub[0], bottom],
        e_stub,
        e_anchor,
    ]);
    out.push(vec![
        s_anchor,
        s_stub,
        [s_stub[0], top],
        [e_stub[0], top],
        e_stub,
        e_anchor,
    ]);

    out
}

fn routing_hull(s: [f32; 2], e: [f32; 2], hosts: &[WorldRect]) -> WorldRect {
    let mut min_x = s[0].min(e[0]);
    let mut max_x = s[0].max(e[0]);
    let mut min_y = s[1].min(e[1]);
    let mut max_y = s[1].max(e[1]);
    for r in hosts {
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

// ---------- Hanan grid ----------

fn hanan_route(start: [f32; 2], end: [f32; 2], solids: &[WorldRect]) -> Option<Vec<[f32; 2]>> {
    if dist2(start, end) < EPS * EPS {
        return Some(vec![start, end]);
    }
    let mut xs = vec![start[0], end[0]];
    let mut ys = vec![start[1], end[1]];
    for r in solids {
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
        solids.iter().any(|o| strictly_inside(*o, p))
    };
    let edge_ok = |a: [f32; 2], b: [f32; 2]| -> bool {
        !solids.iter().any(|o| aa_segment_crosses_interior(a, b, *o))
    };

    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct Cost {
        length: u64,
        left: u64,
        up: u64,
        first: u8,
    }
    impl Cost {
        fn step(self, dx: f32, dy: f32) -> Self {
            let len = dx.abs() + dy.abs();
            let mut c = self;
            c.length = c.length.saturating_add(quant(len));
            if dx < -EPS {
                c.left = c.left.saturating_add(quant(-dx));
            }
            if dy < -EPS {
                c.up = c.up.saturating_add(quant(-dy));
            }
            if c.first == 255 && len > EPS {
                c.first = dir_code(dx, dy);
            }
            c
        }
    }

    let n = nx * ny;
    let mut dist = vec![
        Cost {
            length: u64::MAX,
            left: 0,
            up: 0,
            first: 255,
        };
        n
    ];
    let mut prev = vec![None; n];
    let zero = Cost {
        length: 0,
        left: 0,
        up: 0,
        first: 255,
    };
    dist[start_i] = zero;

    // 4-neighbor Dijkstra. Neighbor order (R, D, L, U) is the tie-break
    // when costs compare equal — first parent wins.
    let mut heap = std::collections::BinaryHeap::new();
    heap.push(std::cmp::Reverse((zero, start_i)));
    let deltas = [(1isize, 0isize), (0, 1), (-1, 0), (0, -1)];

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
    length: u64,
    left: u64,
    up: u64,
    first: u8,
}

fn path_cost(pts: &[[f32; 2]]) -> Cost {
    let mut c = Cost {
        length: 0,
        left: 0,
        up: 0,
        first: 255,
    };
    for w in pts.windows(2) {
        let dx = w[1][0] - w[0][0];
        let dy = w[1][1] - w[0][1];
        let len = dx.abs() + dy.abs();
        c.length = c.length.saturating_add(quant(len));
        if dx < -EPS {
            c.left = c.left.saturating_add(quant(-dx));
        }
        if dy < -EPS {
            c.up = c.up.saturating_add(quant(-dy));
        }
        if c.first == 255 && len > EPS {
            c.first = dir_code(dx, dy);
        }
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

fn aa_segment_crosses_interior(a: [f32; 2], b: [f32; 2], r: WorldRect) -> bool {
    if (a[1] - b[1]).abs() <= EPS {
        let y = a[1];
        if y <= r.y + EPS || y >= r.y + r.h - EPS {
            return false;
        }
        let x0 = a[0].min(b[0]);
        let x1 = a[0].max(b[0]);
        return x0 < r.x + r.w - EPS && x1 > r.x + EPS;
    }
    if (a[0] - b[0]).abs() <= EPS {
        let x = a[0];
        if x <= r.x + EPS || x >= r.x + r.w - EPS {
            return false;
        }
        let y0 = a[1].min(b[1]);
        let y1 = a[1].max(b[1]);
        return y0 < r.y + r.h - EPS && y1 > r.y + EPS;
    }
    false
}

fn polyline_hits_interior(pts: &[[f32; 2]], solids: &[WorldRect]) -> bool {
    pts.windows(2).any(|w| {
        solids
            .iter()
            .any(|o| aa_segment_crosses_interior(w[0], w[1], *o))
    })
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

    fn rect_of(a: WorldRect, b: WorldRect) -> impl Fn(NodeId) -> Option<WorldRect> {
        move |id| match id.0 {
            1 => Some(a),
            2 => Some(b),
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

    fn obstacles(a: WorldRect, b: WorldRect) -> Vec<(NodeId, WorldRect)> {
        vec![(NodeId(1), a), (NodeId(2), b)]
    }

    fn interior_hits(pts: &[[f32; 2]], hosts: &[WorldRect]) -> bool {
        polyline_hits_interior(pts, hosts)
    }

    #[test]
    fn stacked_bottom_to_bottom_wraps_hosts() {
        // B directly above A. Wire from A's base to B's base must leave A
        // downward and wrap around — never climb through either rect.
        let a = WorldRect::new(0.0, 100.0, 80.0, 60.0);
        let b = WorldRect::new(0.0, 0.0, 80.0, 60.0);
        let (ea, eb) = bottom_to_bottom(a, b);
        let pts = connector_ortho_path(&ea, &eb, rect_of(a, b), &obstacles(a, b)).unwrap();
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
        let pts = connector_ortho_path(&a, &b, |_| None, &[]).unwrap();
        // HVH (right then down) wins over VHV (down then right).
        assert_eq!(pts, vec![[0.0, 0.0], [40.0, 0.0], [40.0, 40.0]]);
    }

    #[test]
    fn equal_wrap_around_block_prefers_bottom() {
        // Horizontal run with a centered blocker: top and bottom wraps are
        // the same length. Tie-break picks the bottom path.
        let start = ConnectorEnd::Free { point: [0.0, 0.0] };
        let end = ConnectorEnd::Free { point: [40.0, 0.0] };
        let block = WorldRect::new(16.0, -20.0, 8.0, 40.0);
        let pts = connector_ortho_path(&start, &end, |_| None, &[(NodeId(9), block)]).unwrap();
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
        let path = connector_route(&a, &b, |_| None, WireRouting::Bezier, &[]).unwrap();
        assert!(matches!(path, ConnectorPath::Bezier(_)));
    }

    #[test]
    fn fillet_inserts_cubics_at_corners() {
        let pts = [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0]];
        let cmds = filleted_polyline(&pts, 6.0);
        assert!(cmds.iter().any(|c| matches!(c, PathCmd::Cubic { .. })));
    }
}
