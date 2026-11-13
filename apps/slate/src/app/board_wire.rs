//! Connector wires (keymap wave 2b, cluster B): edge grips, the Grasshopper
//! wire-drag grammar (add / Shift add / Ctrl detach / Ctrl+Shift move-all),
//! connector painting through the path-mesh cache, label editing, and the
//! derived-AABB sync that keeps `Node.rect` fresh for marquee/hit systems.
//!
//! See `docs/keymap/specs/connectors.md`. Geometry is derived, never stored
//! (`slate_doc::connector_route`); one gesture = one journaled step.
const WIRE_BLISTER_SECONDS: f32 = 0.14;
mod drag;
mod grasshopper;
mod grip_hits;
pub use grip_hits::grip_hit_radius;
pub(crate) use grip_hits::wire_ports_fit;

mod canvas_settings;
mod creation;
mod painting;
pub(crate) use painting::{paint_route_preview, WireRouteCache};

use super::board::{rgba32, BoardXf};
use super::{board_path, SlateApp};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    connector_drawn_stroke, ConnectorBezier, ConnectorEnd, ConnectorNode, Dash, Node, NodeKind,
    Scene, SceneCmd, Side, Stroke, StrokeCap, StrokeJoin, WidthProfile, WireDisplay, WorldRect,
};
use slate_doc::wire::{
    filleted_polyline, retreat_off_hosts, ConnectorPath, PathCmd, SceneWireRouting, WireRouting,
    ORTHO_CORNER_RADIUS,
};
use slate_doc::WireHost;
use slate_doc::{ItemId, NodeId, SlateDoc};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use vector_ink::kurbo::BezPath;

/// Press-hit radius on the node side of a grip at zoom ≥ 1, and the full
/// disk for a selected connector's endpoint dots (screen px).
pub const GRIP_HIT_PX: f32 = 8.0;
/// Snap radius while dragging a wire (screen px) to a grip or edge.
pub const WIRE_SNAP_PX: f32 = 14.0;
/// Connector stroke pick width (click select / right-click).
pub const CONNECTOR_PICK_PX: f32 = 8.0;
/// Revision and reach of the authored routes represented by the Scene index.
pub(crate) struct ConnectorPickBounds {
    pub scene_gen: u64,
    pub item_paths_revision: u64,
    pub has_complex_body: bool,
    pub rotated_flow_hosts: Vec<NodeId>,
    pub dock_strips: Vec<NodeId>,
    pub max_reach: f32,
    image_item_paths: HashMap<ItemId, Option<PathBuf>>,
}

impl ConnectorPickBounds {
    fn referenced_paths_unchanged(&self, doc: &SlateDoc) -> bool {
        let mut current = HashMap::new();
        for item in &doc.items {
            if self.image_item_paths.contains_key(&item.id) {
                current.entry(item.id).or_insert(item.path.as_path());
            }
        }
        self.image_item_paths
            .iter()
            .all(|(id, path)| path.as_deref() == current.get(id).copied())
    }
}
/// Faint wires render at 40% opacity (matches the artifact writer).
const FAINT_OPACITY: f32 = 0.4;
/// Connector label font size in world units (matches the artifact's
/// `CONNECTOR_LABEL_SIZE`).
const CONNECTOR_LABEL_SIZE: f32 = 14.0;

/// Which grips are showing this frame: node + the grip under the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripHover {
    pub node: NodeId,
    pub hovered: Option<Side>,
    pub t: f32,
}

/// A live wire gesture (registered as `CancelLayer::ActiveOperation`).
#[allow(clippy::large_enum_variant)]
pub enum WireMode {
    /// Drag from a grip: rubber-band a new connector.
    Add { from: (NodeId, Side, f32) },
    /// Ctrl-drag: the nearest existing end follows the cursor.
    Detach {
        conn: NodeId,
        end_b: bool,
        before: Node,
    },
    /// Ctrl+Shift-drag: every end on the grip follows the cursor.
    MoveAll { items: Vec<(NodeId, bool, Node)> },
}

pub struct WireDrag {
    pub mode: WireMode,
    pub cursor: Pos2,
    pub snap: Option<(NodeId, Side, f32)>,
}

// ---------- pure geometry helpers ----------

#[cfg(test)]
pub(crate) fn grip_point(rect: WorldRect, side: Side) -> Pos2 {
    let p = WireHost::from_rect(rect).anchor(side, 0.5);
    Pos2::new(p[0], p[1])
}

/// Distance from a point to the rect outline (0 on the boundary; positive
/// inside and outside alike).
#[cfg(test)]
fn rect_edge_dist(rect: WorldRect, p: Pos2) -> f32 {
    let (l, r, t, b) = (rect.x, rect.x + rect.w, rect.y, rect.y + rect.h);
    if p.x >= l && p.x <= r && p.y >= t && p.y <= b {
        (p.x - l).min(r - p.x).min(p.y - t).min(b - p.y)
    } else {
        let dx = (l - p.x).max(p.x - r).max(0.0);
        let dy = (t - p.y).max(p.y - b).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }
}

pub(crate) fn connector_kurbo(bez: &ConnectorBezier) -> BezPath {
    let mut path = BezPath::new();
    path.move_to((bez.p0[0] as f64, bez.p0[1] as f64));
    path.curve_to(
        (bez.c1[0] as f64, bez.c1[1] as f64),
        (bez.c2[0] as f64, bez.c2[1] as f64),
        (bez.p3[0] as f64, bez.p3[1] as f64),
    );
    path
}

fn path_cmds_to_kurbo(cmds: &[PathCmd]) -> BezPath {
    let mut path = BezPath::new();
    for cmd in cmds {
        match *cmd {
            PathCmd::Move(p) => path.move_to((p[0] as f64, p[1] as f64)),
            PathCmd::Line(p) => path.line_to((p[0] as f64, p[1] as f64)),
            PathCmd::Cubic { c1, c2, to } => path.curve_to(
                (c1[0] as f64, c1[1] as f64),
                (c2[0] as f64, c2[1] as f64),
                (to[0] as f64, to[1] as f64),
            ),
        }
    }
    path
}

pub(crate) fn connector_path_kurbo(path: &ConnectorPath) -> BezPath {
    match path {
        ConnectorPath::Bezier(bez) => connector_kurbo(bez),
        ConnectorPath::Orthogonal(pts) => {
            path_cmds_to_kurbo(&filleted_polyline(pts, ORTHO_CORNER_RADIUS))
        }
    }
}

/// Mesh-cache key: the *geometry* + routing + stroke + display + zoom
/// bucket, so a moved endpoint or a style toggle invalidates the cache
/// (Art. II — tessellate on change only).
pub(crate) fn connector_cache_key(
    path: &ConnectorPath,
    routing: WireRouting,
    stroke: &Stroke,
    display: WireDisplay,
    bucket: i64,
) -> u64 {
    let mut h = DefaultHasher::new();
    std::mem::discriminant(&routing).hash(&mut h);
    std::mem::discriminant(path).hash(&mut h);
    let cubic;
    let points = match path {
        ConnectorPath::Bezier(b) => {
            cubic = [b.p0, b.c1, b.c2, b.p3];
            &cubic[..]
        }
        ConnectorPath::Orthogonal(points) => points.as_slice(),
    };
    for p in points {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
    }
    board_path::hash_stroke(&mut h, stroke);
    std::mem::discriminant(&display).hash(&mut h);
    bucket.hash(&mut h);
    h.finish()
}

/// Exact stroke pick for a route through the current host poses.
pub(crate) fn hit_connector(
    path: ConnectorPath,
    conn: &ConnectorNode,
    point: [f32; 2],
    zoom: f32,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
) -> bool {
    let (path, stroke) = drawn_connector_with_hosts(path, conn, host_of);
    let kurbo = connector_path_kurbo(&path);
    let style = board_path::stroke_style_world(&stroke, zoom);
    let slop = CONNECTOR_PICK_PX / zoom.max(atlas_core::display::SLATE_BOARD.min);
    vector_ink::hit_stroke(&kurbo, &style, point, slop)
}

/// The pointer is on a node (or within the wire's own pick slop of one).
/// That node wins; the wire underneath must not.
pub(crate) fn wire_blocked_by_node(
    scene: &Scene,
    wx: f32,
    wy: f32,
    zoom: f32,
    accepts: &dyn Fn(&Node) -> bool,
) -> bool {
    let slop = CONNECTOR_PICK_PX / zoom.max(atlas_core::display::SLATE_BOARD.min);
    let pad = slop * std::f32::consts::SQRT_2;
    scene
        .query_rect(WorldRect::new(wx - pad, wy - pad, pad * 2.0, pad * 2.0))
        .into_iter()
        .filter_map(|id| scene.node(id))
        .any(|n| {
            if n.hidden || n.is_frame() || matches!(n.kind, NodeKind::Connector(_)) || !accepts(n) {
                return false;
            }
            let r = n.rect;
            WorldRect::new(r.x - slop, r.y - slop, r.w + slop * 2.0, r.h + slop * 2.0)
                .contains_rotated(wx, wy, n.rotation_deg)
        })
}

fn connector_pick_reach(conn: &ConnectorNode) -> f32 {
    let stroke = connector_drawn_stroke(conn.stroke);
    let taper = match stroke.profile {
        WidthProfile::Uniform => 1.0,
        WidthProfile::Taper { start, end } => start.abs().max(end.abs()),
        WidthProfile::Ends { tip } => tip.abs().max(1.0),
    };
    // Half width for the host retreat, then the widest stroke's hit radius.
    stroke.width * 0.5 * (1.0 + taper)
}

#[cfg(test)]
/// Painted stroke (twice the stored width) and the same curve pulled just
/// outside each anchored host so the stroke does not cross that face.
pub(crate) fn drawn_connector(
    scene: &Scene,
    path: ConnectorPath,
    conn: &ConnectorNode,
) -> (ConnectorPath, Stroke) {
    drawn_connector_with_hosts(path, conn, slate_doc::wire::scene_wire_hosts(scene))
}

fn drawn_connector_with_hosts(
    path: ConnectorPath,
    conn: &ConnectorNode,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
) -> (ConnectorPath, Stroke) {
    let stroke = connector_drawn_stroke(conn.stroke);
    let path = retreat_off_hosts(path, &conn.a, &conn.b, host_of, stroke.width * 0.5);
    (path, stroke)
}

fn is_on_grip(end: &ConnectorEnd, node: NodeId, side: Side) -> bool {
    matches!(end, ConnectorEnd::Anchored { node: n, side: s, .. } if *n == node && *s == side)
}

// ---------- SlateApp: wires ----------

impl SlateApp {
    fn default_wire_stroke(&self) -> Stroke {
        Stroke {
            width: 2.0,
            color: slate_doc::scene::WIRE_LEGACY_INK_LIGHT,
            dash: Dash::Solid,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            profile: WidthProfile::Uniform,
            softness: 0.0,
            stamp: false,
            tween_from: None,
            gaussian_blur: 0.0,
            arrow_end: false,
            arrow_start: false,
            arrow_sizes: [None; 2],
            cap_start: None,
            cap_end: None,
            texture: Default::default(),
        }
    }

    pub(crate) fn persist_wire_routing(&mut self) {
        self.settings.board_wire_routing = self.board_wire_routing;
        self.settings.save();
        self.connector_sync_gen = 0;
    }

    /// The connectors a routing command acts on: `{"wires":[…]}` (a
    /// crosstalk capsule names its conversation's wires), else the selected
    /// connectors. Empty means the board's default for new wires.
    pub(crate) fn wire_routing_targets(&self, detail: Option<&str>) -> Vec<NodeId> {
        #[derive(serde::Deserialize)]
        struct Targets {
            wires: Vec<NodeId>,
        }
        let named = detail.and_then(|d| serde_json::from_str::<Targets>(d).ok());
        let ids: Vec<NodeId> = match named {
            Some(t) => t.wires,
            None => self.board_sel.iter().copied().collect(),
        };
        ids.into_iter()
            .filter(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .is_some_and(|n| matches!(n.kind, NodeKind::Connector(_)))
            })
            .collect()
    }

    pub(crate) fn set_wire_routing(&mut self, routing: WireRouting, ids: Vec<NodeId>) -> bool {
        if ids.is_empty() {
            self.board_wire_routing = routing;
            self.persist_wire_routing();
            return true;
        }
        self.shape_property_command(
            serde_json::to_string(&super::board_properties::PropertyRequest {
                ids,
                edits: vec![super::board_properties::Property::WireRouting(routing)],
                points: Vec::new(),
            })
            .ok()
            .as_deref(),
        )
    }

    // ----- grips -----

    /// A multi-selection claims presses inside its bounding box: that press
    /// moves the group, so no port on it may start a wire.
    pub(crate) fn selection_bounds_claim_press(&self, world: Pos2) -> bool {
        self.board_sel.len() >= 2
            && self
                .board_group_bounds()
                .is_some_and(|gb| gb.contains(world.x, world.y))
    }

    /// Another node's body holds the press but not `owner`'s port at
    /// `port`, so the press belongs to that body. A slide frame or other
    /// container around both does not block.
    fn body_claims_press(&self, owner: NodeId, world: Pos2, port: [f32; 2]) -> bool {
        let scene = &self.doc().scene;
        let Some(index) = scene.index_of(owner) else {
            return false;
        };
        let z = self.board_xf().z;
        scene.query_point(world.x, world.y).into_iter().any(|id| {
            let Some(i) = scene.index_of(id) else {
                return false;
            };
            let m = &scene.nodes[i];
            m.id != owner
                && !m.hidden
                && !m.is_frame()
                && !matches!(m.kind, NodeKind::Connector(_))
                && m.rect.contains_rotated(world.x, world.y, m.rotation_deg)
                && !m.rect.contains_rotated(port[0], port[1], m.rotation_deg)
                && self.wire_host(m).is_area()
                && (i > index
                    || !self.doc().outline_queries().node_outlines_overlap(
                        owner,
                        m.id,
                        0.25 / z.max(0.001),
                    ))
        })
    }

    fn theme_wire_rgba(&self) -> slate_doc::scene::Rgba {
        super::board::to_rgba(self.palette().wire)
    }

    /// Shared topmost port hit: bodies and polygon rotation halos occlude ports.
    pub(crate) fn wire_grip_at(&self, screen: Pos2, xf: &BoardXf) -> Option<(NodeId, Side, f32)> {
        let w = xf.s2w(screen);
        if self.selection_bounds_claim_press(w) {
            return None;
        }
        let scene = &self.doc().scene;
        let manual_context = self.agent_manual_context_at(screen, xf);
        // The last painted manual-context dot can be outside its card.
        let local = manual_context
            .is_none()
            .then(|| self.wire_grip_candidates(w, xf));
        let indexed = local
            .iter()
            .flatten()
            .rev()
            .filter_map(|id| scene.node(*id));
        let full = local
            .is_none()
            .then(|| scene.nodes.iter().rev())
            .into_iter()
            .flatten();
        for n in indexed.chain(full) {
            if n.hidden
                || matches!(n.kind, NodeKind::Connector(_))
                || self.board_selected_frame_owner(n.id).is_some()
            {
                continue;
            }
            if self.polygon_rotate_at(n, screen, xf).is_some() {
                return None;
            }
            let host = self.wire_input_host(n);
            if self.portal_capture_owns_viewport(n.id)
                || !host.offers_ports()
                || !wire_ports_fit(n.rect, xf.z)
            {
                if self.wire_extension_yields_to_transform(n, screen, xf) {
                    return None;
                }
                if n.rect.contains_rotated(w.x, w.y, n.rotation_deg) {
                    return None;
                }
                continue;
            }
            if let NodeKind::Portal(portal) = &n.kind {
                if portal.kind == slate_doc::scene::PortalKind::Agent {
                    if let Some(side) = self.crosstalk_port_at(n, screen, xf) {
                        if !self.body_claims_press(n.id, w, host.anchor(side, 0.5)) {
                            return Some((n.id, side, 0.5));
                        }
                    }
                    if let Some(id) = manual_context {
                        let port = self
                            .doc()
                            .scene
                            .node(id)
                            .map(|card| self.wire_host(card).anchor(Side::Left, 0.5));
                        if port.is_some_and(|port| !self.body_claims_press(id, w, port)) {
                            return Some((id, Side::Left, 0.5));
                        }
                    }
                    if !host.is_flow() {
                        if host.is_area() && n.rect.contains_rotated(w.x, w.y, n.rotation_deg) {
                            return None;
                        }
                        continue;
                    }
                }
            }
            // Reject distant pointer fields before asking for actual overlap.
            let hovered = self
                .presented_wire_ports(n)
                .into_iter()
                .filter(|port| {
                    let outward = host.outward(port.side, port.t);
                    self.semantic_wire_pick(n, screen, xf, port.point, outward)
                })
                .min_by(|a, b| {
                    let d = |p: &slate_doc::WirePort| {
                        xf.w2s(Pos2::new(p.point[0], p.point[1])).distance(screen)
                    };
                    d(a).total_cmp(&d(b))
                });
            if let Some(port) = hovered {
                if self.node_controls_buried(n, xf) {
                    continue;
                }
                if (self.wire_cursor_inside(n, w, xf) || !self.wire_host_overlaps_lower(n, xf))
                    && !self.body_claims_press(n.id, w, port.point)
                {
                    return Some((n.id, port.side, port.t));
                }
            }
            if self.wire_extension_yields_to_transform(n, screen, xf) {
                return None;
            }
            if host.is_area() && n.rect.contains_rotated(w.x, w.y, n.rotation_deg) {
                return None;
            }
        }
        None
    }

    /// Per-frame grip hover (Select tool): only the grip whose midpoint is within
    /// the grip hit of the pointer previews, locked nodes included (wires may anchor
    /// to them). An edge between grips is inert; a body occludes grips behind it.
    pub(crate) fn update_wire_grips(&mut self, pointer: Option<Pos2>, xf: &BoardXf) {
        self.wire_grips = None;
        let Some(p) = pointer else { return };
        if let Some((node, side, t)) = self.wire_grip_at(p, xf) {
            self.wire_grips = Some(GripHover {
                node,
                hovered: Some(side),
                t,
            });
        }
    }

    pub(crate) fn paint_wire_grips(&self, painter: &egui::Painter, xf: &BoardXf) {
        let key = egui::Id::new("wire-blister-last");
        if let Some(grips) = self
            .wire_grips
            .filter(|g| !self.portal_capture_owns_viewport(g.node))
        {
            painter.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            painter.ctx().data_mut(|d| d.insert_temp(key, grips));
        }
        let Some(grips) = painter.ctx().data(|d| d.get_temp::<GripHover>(key)) else {
            return;
        };
        let Some(side) = grips.hovered else {
            return;
        };
        let Some(node) = self.doc().scene.node(grips.node) else {
            return;
        };
        if self.portal_capture_owns_viewport(node.id) {
            return;
        }
        if self.grasshopper_grip(node.id, side, grips.t).is_some()
            || (!self.wire_input_ports(node).is_empty()
                && self.source_agent_inputs_visible(node.id))
            || slate_doc::agent_chat::agent(node)
                .is_some_and(|a| a.generator.input_source_kind.is_some())
        {
            return; // The shared typed-port owner paints these graph sites.
        }
        let visible = self
            .wire_grips
            .is_some_and(|g| g.node == grips.node && g.hovered == Some(side));
        let progress = painter.ctx().animate_bool_with_time(
            egui::Id::new(("wire-blister", grips.node.0, side)),
            visible,
            WIRE_BLISTER_SECONDS,
        );
        if progress <= 0.001 {
            return;
        }
        let edge = xf.w2s(self.wire_port_point(node, side, grips.t));
        let Some(presentation) =
            atlas_shell::selection_tools::hit_regions::PortPresentation::ordinary(
                node.rect.w.min(node.rect.h) * xf.z,
                xf.z,
            )
        else {
            return;
        };
        let r = presentation.radius;
        let normal = self.wire_host(node).outward(side, grips.t);
        let center = edge + egui::vec2(normal[0], normal[1]) * (r * (progress - 1.0));
        painter.circle_filled(center, r, self.palette().accent.gamma_multiply(0.45));
    }

    // ----- gesture begin / update / end -----

    /// Wire-drag start checks, called from `begin_gesture` (Select tool,
    /// before resize): endpoint dots of a selected connector first
    /// (FigJam-style detach), then the edge grip under the press origin.
    /// Hit-tests `screen` directly so a drag that has left the preview
    /// dot still starts a wire. Returns `None` when the press is not a
    /// wire gesture.
    pub(crate) fn try_begin_wire_drag(
        &mut self,
        screen: Pos2,
        world: Pos2,
        mods: egui::Modifiers,
    ) -> Option<WireDrag> {
        let xf = self.board_xf();
        // Dragging an endpoint dot of the selected connector = detach.
        if self.board_sel.len() == 1 {
            let id = *self.board_sel.iter().next().unwrap();
            if let Some(NodeKind::Connector(conn)) =
                self.doc().scene.node(id).map(|n| n.kind.clone())
            {
                for (end, end_b) in [(&conn.a, false), (&conn.b, true)] {
                    let Some(p) = self.wire_end_point(end) else {
                        continue;
                    };
                    if xf.w2s(p).distance(screen) <= grip_hit_radius(xf.z) {
                        if conn.grasshopper.is_some() {
                            self.toast("Delete the Grasshopper wire to disconnect its ports.");
                            return None;
                        }
                        if conn.crosstalk.is_some() {
                            self.toast("A crosstalk wire follows its messages; its ends stay put.");
                            return None;
                        }
                        if let Some(target) = slate_doc::agent_inputs::generator_memory_target(
                            &self.doc().scene,
                            &conn,
                        ) {
                            if !slate_doc::agent_inputs::generator_history_is_terminal(
                                &self.doc().scene,
                                target,
                            ) {
                                self.toast("Only the final generator's incoming history can be disconnected.");
                                return None;
                            }
                        }
                        if conn.binding.as_ref().is_some_and(|b| b.consumed)
                            && slate_doc::agent_inputs::generator_memory_target(
                                &self.doc().scene,
                                &conn,
                            )
                            .is_none()
                        {
                            self.toast("This context was already sent. It stays attached.");
                            return None;
                        }
                        let before = self.doc().scene.node(id)?.clone();
                        return Some(WireDrag {
                            mode: WireMode::Detach {
                                conn: id,
                                end_b,
                                before,
                            },
                            cursor: world,
                            snap: None,
                        });
                    }
                }
            }
        }

        // Press origin on a side-midpoint grip — not the live hover cache.
        let (node_id, side, t) = self.wire_grip_at(screen, &xf)?;
        let from = (node_id, side, t);
        if self.grasshopper_grip(node_id, side, t).is_some() {
            return Some(WireDrag {
                mode: WireMode::Add { from },
                cursor: world,
                snap: None,
            });
        }

        // Ends currently anchored to this grip (node + side).
        let ends: Vec<(NodeId, bool, Node)> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| {
                let NodeKind::Connector(c) = &n.kind else {
                    return None;
                };
                if c.grasshopper.is_some() {
                    None
                } else if is_on_grip(&c.a, node_id, side) {
                    Some((n.id, false, n.clone()))
                } else if is_on_grip(&c.b, node_id, side) {
                    Some((n.id, true, n.clone()))
                } else {
                    None
                }
            })
            .collect();

        let mode = if self.is_crosstalk_port(from) {
            WireMode::Add { from }
        } else if mods.ctrl && mods.shift && !ends.is_empty() {
            WireMode::MoveAll { items: ends }
        } else if mods.ctrl && !mods.shift && !ends.is_empty() {
            // Detach the end nearest the press.
            let nearest = ends
                .into_iter()
                .min_by(|a, b| {
                    let d = |item: &(NodeId, bool, Node)| {
                        let NodeKind::Connector(c) = &item.2.kind else {
                            return f32::INFINITY;
                        };
                        let end = if item.1 { &c.b } else { &c.a };
                        self.wire_end_point(end)
                            .map(|p| p.distance(world))
                            .unwrap_or(f32::INFINITY)
                    };
                    d(a).total_cmp(&d(b))
                })
                .expect("non-empty");
            if let NodeKind::Connector(c) = &nearest.2.kind {
                if let Some(target) =
                    slate_doc::agent_inputs::generator_memory_target(&self.doc().scene, c)
                {
                    if !slate_doc::agent_inputs::generator_history_is_terminal(
                        &self.doc().scene,
                        target,
                    ) {
                        self.toast(
                            "Only the final generator's incoming history can be disconnected.",
                        );
                        return None;
                    }
                }
                if c.binding.as_ref().is_some_and(|b| b.consumed)
                    && slate_doc::agent_inputs::generator_memory_target(&self.doc().scene, c)
                        .is_none()
                {
                    self.toast("This context was already sent. It stays attached.");
                    return None;
                }
            }
            WireMode::Detach {
                conn: nearest.0,
                end_b: nearest.1,
                before: nearest.2,
            }
        } else {
            // Plain and Shift+drag both ADD (whiteboard additive default).
            WireMode::Add { from }
        };
        Some(WireDrag {
            mode,
            cursor: world,
            snap: None,
        })
    }

    // ----- derived rect sync -----

    pub(crate) fn current_connector_pick_bounds(&self) -> Option<&ConnectorPickBounds> {
        self.connector_pick_bounds.as_ref().filter(|bounds| {
            self.connector_sync_gen == self.scene_gen
                && bounds.scene_gen == self.doc().scene.scene_gen()
                && bounds.item_paths_revision == self.doc().item_paths_revision()
        })
    }

    /// Keep connector `Node.rect`s equal to their derived curve AABB so
    /// marquee/hit systems keep working. Runs only when the scene content
    /// generation moved (journal commits / undo / redo / tab switch), never
    /// per frame for all connectors (Art. II). Derived-cache write: not a
    /// journaled mutation and does not dirty the workbook by itself.
    pub(crate) fn sync_connector_rects(&mut self) {
        let item_paths_revision = self.doc().item_paths_revision();
        if self.connector_sync_gen == self.scene_gen
            && self.connector_pick_bounds.as_ref().is_some_and(|bounds| {
                bounds.item_paths_revision == item_paths_revision
                    || (bounds.scene_gen == self.doc().scene.scene_gen()
                        && bounds.referenced_paths_unchanged(self.doc()))
            })
        {
            if let Some(bounds) = self.connector_pick_bounds.as_mut() {
                bounds.item_paths_revision = item_paths_revision;
            }
            return;
        }
        let _span = atlas_core::session_log::span("slate.board.connector_sync");
        self.connector_sync_gen = self.scene_gen;
        let routing = self.board_wire_routing;
        let scene = &self.doc().scene;
        let routes = SceneWireRouting::default();
        let mut max_reach = 0.0_f32;
        let mut has_complex_body = false;
        let mut rotated_flow_hosts = Vec::new();
        let mut dock_strips = Vec::new();
        let mut image_item_paths = HashMap::new();
        let mut updates = Vec::new();
        for node in &scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                if let NodeKind::Image(image) = &node.kind {
                    image_item_paths.entry(image.item).or_insert(None);
                }
                if matches!(node.kind, NodeKind::DockStrip(_)) {
                    dock_strips.push(node.id);
                }
                has_complex_body |= matches!(node.kind, NodeKind::Shape(_))
                    || (node.is_frame() && node.rotation_deg != 0.0);
                if node.rotation_deg != 0.0
                    && matches!(
                        node.kind,
                        NodeKind::Image(_) | NodeKind::Portal(_) | NodeKind::Text(_)
                    )
                {
                    rotated_flow_hosts.push(node.id);
                }
                continue;
            };
            max_reach = max_reach.max(connector_pick_reach(conn));
            let ends = self.current_grasshopper_ends(conn);
            let path = routes.route(
                scene,
                Some(node.id),
                &ends[0],
                &ends[1],
                conn.effective_routing(routing),
                |id| scene.node(id).map(|host| self.wire_host(host)),
            );
            if let Some(rect) = path
                .map(|path| path.aabb())
                .filter(|rect| *rect != node.rect)
            {
                updates.push((node.id, rect));
            }
        }
        // Linked paths affect wire roles only for placed image nodes. An
        // imported result that has yet to be placed cannot move these sites.
        // Keep missing references too, in case an import makes one resolve.
        for item in &self.doc().items {
            if let Some(path @ None) = image_item_paths.get_mut(&item.id) {
                *path = Some(item.path.clone());
            }
        }
        self.tab_mut()
            .doc
            .scene
            .set_derived_connector_rects(updates);
        self.connector_pick_bounds = Some(ConnectorPickBounds {
            scene_gen: self.doc().scene.scene_gen(),
            item_paths_revision,
            has_complex_body,
            rotated_flow_hosts,
            dock_strips,
            max_reach,
            image_item_paths,
        });
    }

    // ----- label editing -----

    pub(crate) fn open_wire_label_edit(&mut self, id: NodeId) {
        // A crosswire's label is its chip, derived from the chain.
        if self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::crosstalk::crosstalk)
            .is_some()
        {
            return;
        }
        let Some(NodeKind::Connector(conn)) = self.doc().scene.node(id).map(|n| n.kind.clone())
        else {
            return;
        };
        self.wire_label_edit = Some((id, conn.label.unwrap_or_default()));
    }

    /// Inline label editor at the curve midpoint. Commit on Enter / Esc /
    /// click-away; one journaled Patch.
    pub(crate) fn wire_label_overlay(&mut self, ctx: &egui::Context, xf: &BoardXf) {
        let Some((id, mut buf)) = self.wire_label_edit.clone() else {
            return;
        };
        let Some(NodeKind::Connector(conn)) = self.doc().scene.node(id).map(|n| n.kind.clone())
        else {
            self.wire_label_edit = None;
            return;
        };
        let Some(path) = self.connector_path_visible(id, &conn) else {
            self.wire_label_edit = None;
            return;
        };
        let m = path.midpoint();
        let mid = xf.w2s(Pos2::new(m[0], m[1]));
        let mut commit = false;
        egui::Area::new(egui::Id::new(("slate_wire_label", id.0)))
            .fixed_pos(mid - Vec2::new(80.0, 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut buf)
                            .hint_text("Label…")
                            .desired_width(140.0),
                    );
                    resp.request_focus();
                    if resp.changed() {
                        self.wire_label_edit = Some((id, buf.clone()));
                    }
                    if ui.input(|i| {
                        i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape)
                    }) {
                        commit = true;
                    }
                    if resp.lost_focus() {
                        commit = true;
                    }
                });
            });
        if commit {
            self.commit_wire_label();
        }
    }

    pub(crate) fn commit_wire_label(&mut self) {
        let Some((id, buf)) = self.wire_label_edit.take() else {
            return;
        };
        let label = {
            let t = buf.trim();
            (!t.is_empty()).then(|| t.to_string())
        };
        self.patch_nodes(&[id], move |n| {
            if let NodeKind::Connector(c) = &mut n.kind {
                c.label = label.clone();
            }
        });
        self.last_board_edit = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bez(p0: [f32; 2], p3: [f32; 2]) -> ConnectorBezier {
        ConnectorBezier {
            p0,
            c1: [p0[0] + 10.0, p0[1]],
            c2: [p3[0] - 10.0, p3[1]],
            p3,
        }
    }

    fn stroke() -> Stroke {
        Stroke {
            width: 2.0,
            color: slate_doc::scene::Rgba::BLACK,
            dash: Dash::Solid,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            profile: WidthProfile::Uniform,
            softness: 0.0,
            stamp: false,
            tween_from: None,
            gaussian_blur: 0.0,
            arrow_end: false,
            arrow_start: false,
            arrow_sizes: [None; 2],
            cap_start: None,
            cap_end: None,
            texture: Default::default(),
        }
    }

    #[test]
    fn grip_hit_radius_grows_only_while_zoomed_out() {
        assert_eq!(grip_hit_radius(1.0), GRIP_HIT_PX);
        assert_eq!(grip_hit_radius(4.0), GRIP_HIT_PX);
        let zoom = 0.25;
        let grown = grip_hit_radius(zoom);
        assert!((grown - (GRIP_HIT_PX * zoom.powf(-0.35)).min(14.0)).abs() < 1e-4);
        assert!(grown > GRIP_HIT_PX && grown <= 14.0);
        assert!((grip_hit_radius(0.01) - 14.0).abs() < 1e-4);
    }

    #[test]
    fn cache_key_is_stable_and_geometry_sensitive() {
        let a = ConnectorPath::Bezier(bez([0.0, 0.0], [100.0, 0.0]));
        let k1 = connector_cache_key(&a, WireRouting::Bezier, &stroke(), WireDisplay::Default, 8);
        let k2 = connector_cache_key(&a, WireRouting::Bezier, &stroke(), WireDisplay::Default, 8);
        assert_eq!(k1, k2, "same geometry → same key (cache hit)");

        // An endpoint node's rect moved → different endpoint → new key.
        let moved = ConnectorPath::Bezier(bez([0.0, 0.0], [120.0, 5.0]));
        assert_ne!(
            k1,
            connector_cache_key(
                &moved,
                WireRouting::Bezier,
                &stroke(),
                WireDisplay::Default,
                8
            )
        );
        // Faint, zoom bucket, and routing also key the tessellation.
        assert_ne!(
            k1,
            connector_cache_key(&a, WireRouting::Bezier, &stroke(), WireDisplay::Faint, 8)
        );
        assert_ne!(
            k1,
            connector_cache_key(&a, WireRouting::Bezier, &stroke(), WireDisplay::Default, 9)
        );
        assert_ne!(
            k1,
            connector_cache_key(
                &a,
                WireRouting::Orthogonal,
                &stroke(),
                WireDisplay::Default,
                8
            )
        );
    }

    #[test]
    fn nearest_side_projects_fraction() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        let snap = WireHost::from_rect(rect).snap([25.0, -4.0]);
        assert_eq!(snap.side, Side::Top);
        assert!((snap.t - 0.25).abs() < 1e-4);
        assert!((snap.dist - 4.0).abs() < 1e-4);
        let snap = WireHost::from_rect(rect).snap([103.0, 25.0]);
        assert_eq!(snap.side, Side::Right);
        assert!((snap.t - 0.5).abs() < 1e-4);
    }

    #[test]
    fn rect_edge_distance_inside_and_out() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        assert!((rect_edge_dist(rect, Pos2::new(3.0, 25.0)) - 3.0).abs() < 1e-4);
        assert!((rect_edge_dist(rect, Pos2::new(-4.0, 25.0)) - 4.0).abs() < 1e-4);
        assert!((rect_edge_dist(rect, Pos2::new(50.0, 25.0)) - 25.0).abs() < 1e-4);
    }
}
