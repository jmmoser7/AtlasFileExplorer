//! Connector wires (keymap wave 2b, cluster B): edge grips, the Grasshopper
//! wire-drag grammar (add / Shift add / Ctrl detach / Ctrl+Shift move-all),
//! connector painting through the path-mesh cache, label editing, and the
//! derived-AABB sync that keeps `Node.rect` fresh for marquee/hit systems.
//!
//! See `docs/keymap/specs/connectors.md`. Geometry is derived, never stored
//! (`slate_doc::connector_route`); one gesture = one journaled step.
const WIRE_BLISTER_SECONDS: f32 = 0.14;
mod drag;
mod grip_hits;
pub(crate) use grip_hits::{grip_hit, port_disc_visible, wire_ports_fit};
pub use grip_hits::{grip_hit_out_radius, grip_hit_radius};

mod canvas_settings;
mod creation;

use super::board::{rgba32, BoardXf};
use super::{board_path, SlateApp};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    connector_drawn_stroke, ConnectorBezier, ConnectorEnd, ConnectorNode, Dash, Node, NodeKind,
    Scene, SceneCmd, Side, Stroke, StrokeCap, StrokeJoin, WidthProfile, WireDisplay, WorldRect,
};
use slate_doc::wire::{
    connector_aabb_routed, connector_route_in_scene, filleted_polyline, retreat_off_hosts,
    scene_wire_hosts, scene_wire_obstacles, ConnectorPath, OrthoLane, PathCmd, WireRouting,
    ORTHO_CORNER_RADIUS,
};
#[cfg(test)]
use slate_doc::WireHost;
use slate_doc::{connector_anchor_on, NodeId};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use vector_ink::kurbo::BezPath;

/// Painted grip radius at zoom 1. Screen size is this times zoom.
const GRIP_RADIUS: f32 = 3.0;
/// Press-hit radius on the node side of a grip at zoom ≥ 1, and the full
/// disk for a selected connector's endpoint dots (screen px).
pub const GRIP_HIT_PX: f32 = 8.0;
/// Outward press-hit radius at zoom ≥ 1: 500% larger than [`GRIP_HIT_PX`]
/// (six times). The extra reach is the outward half-plane only — inside
/// the node the hit stays the inner radius.
pub const GRIP_HIT_OUT_PX: f32 = GRIP_HIT_PX * 6.0;
/// Painted port discs smaller than this (screen px) are dropped and are
/// not hittable. Shared with the flow-port painter.
pub(crate) const PORT_LOD_PX: f32 = 1.5;
/// Snap radius while dragging a wire (screen px) to a grip or edge.
pub const WIRE_SNAP_PX: f32 = 14.0;
/// Connector stroke pick width (click select / right-click).
pub const CONNECTOR_PICK_PX: f32 = 8.0;
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

fn port_point(node: &Node, side: Side, t: f32) -> Pos2 {
    let p = connector_anchor_on(node, side, t);
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
    for p in path.points() {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
    }
    format!("{stroke:?}{display:?}").hash(&mut h);
    bucket.hash(&mut h);
    h.finish()
}

/// Stroke hit-test for connectors (used by the shared point pick). Skips
/// connectors whose anchored node is hidden — they are not painted either.
pub fn hit_connector_routed(
    scene: &Scene,
    id: NodeId,
    conn: &ConnectorNode,
    wx: f32,
    wy: f32,
    zoom: f32,
    routing: WireRouting,
) -> bool {
    if wire_blocked_by_node(scene, wx, wy, zoom) {
        return false;
    }
    let Some(path) = connector_route_in_scene(
        scene,
        Some(id),
        &conn.a,
        &conn.b,
        conn.effective_routing(routing),
    ) else {
        return false;
    };
    let (path, stroke) = drawn_connector(scene, path, conn);
    let kurbo = connector_path_kurbo(&path);
    let style = board_path::stroke_style_world(&stroke, zoom);
    let slop = CONNECTOR_PICK_PX / zoom.max(0.05);
    vector_ink::hit_stroke(&kurbo, &style, [wx, wy], slop)
}

/// The world point of one connector end (anchored ends resolve through the
/// current rects; hidden anchors still resolve for interaction purposes).
fn end_point(scene: &Scene, end: &ConnectorEnd) -> Option<Pos2> {
    match end {
        ConnectorEnd::Anchored { node, side, t } => {
            scene.node(*node).map(|n| port_point(n, *side, *t))
        }
        ConnectorEnd::Free { point } => Some(Pos2::new(point[0], point[1])),
    }
}

/// The pointer is on a node (or within the wire's own pick slop of one).
/// That node wins; the wire underneath must not.
fn wire_blocked_by_node(scene: &Scene, wx: f32, wy: f32, zoom: f32) -> bool {
    let slop = CONNECTOR_PICK_PX / zoom.max(0.05);
    scene.nodes.iter().any(|n| {
        if n.hidden || n.is_frame() || matches!(n.kind, NodeKind::Connector(_)) {
            return false;
        }
        let r = n.rect;
        WorldRect::new(r.x - slop, r.y - slop, r.w + slop * 2.0, r.h + slop * 2.0).contains_rotated(
            wx,
            wy,
            n.rotation_deg,
        )
    })
}

/// Painted stroke (twice the stored width) and the same curve pulled just
/// outside each anchored host so the stroke does not cross that face.
pub(crate) fn drawn_connector(
    scene: &Scene,
    path: ConnectorPath,
    conn: &ConnectorNode,
) -> (ConnectorPath, Stroke) {
    let stroke = connector_drawn_stroke(conn.stroke);
    let path = retreat_off_hosts(
        path,
        &conn.a,
        &conn.b,
        scene_wire_hosts(scene),
        stroke.width * 0.5,
    );
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

    pub(crate) fn connector_path_visible(
        &self,
        id: NodeId,
        conn: &ConnectorNode,
    ) -> Option<ConnectorPath> {
        connector_route_in_scene(
            &self.doc().scene,
            Some(id),
            &conn.a,
            &conn.b,
            conn.effective_routing(self.board_wire_routing),
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
        self.doc().scene.nodes.iter().any(|m| {
            m.id != owner
                && !m.hidden
                && !matches!(m.kind, NodeKind::Connector(_))
                && m.rect.contains_rotated(world.x, world.y, m.rotation_deg)
                && !m.rect.contains_rotated(port[0], port[1], m.rotation_deg)
                && self.wire_host(m).is_area()
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
        let hit_px = grip_hit_radius(xf.z);
        let hit_out = grip_hit_out_radius(xf.z);
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden || matches!(n.kind, NodeKind::Connector(_)) {
                continue;
            }
            if self.polygon_rotate_at(n, screen, xf).is_some() {
                return None;
            }
            let host = self.wire_host(n);
            if !host.offers_ports() || !wire_ports_fit(n.rect, xf.z) {
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
                    if let Some(id) = self.agent_manual_context_at(screen, xf) {
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
            let designed = if host.is_flow() {
                super::board_flow::PORT_RADIUS
            } else {
                GRIP_RADIUS
            };
            // Stacked ports overlap once zoomed out; the nearest one wins.
            // A disc the painter dropped (below the LOD) is not a target.
            let hovered = port_disc_visible(designed, xf.z).then(|| {
                host.ports()
                    .into_iter()
                    .filter(|port| {
                        let g = xf.w2s(Pos2::new(port.point[0], port.point[1]));
                        let outward = host.outward(port.side, port.t);
                        (screen.distance(g) <= hit_px
                            || !self.wire_extension_yields_to_transform(n, screen, xf))
                            && grip_hit(
                                screen,
                                g,
                                Vec2::new(outward[0], outward[1]),
                                hit_px,
                                hit_out,
                            )
                    })
                    .min_by(|a, b| {
                        let d = |p: &slate_doc::WirePort| {
                            xf.w2s(Pos2::new(p.point[0], p.point[1])).distance(screen)
                        };
                        d(a).total_cmp(&d(b))
                    })
            });
            if let Some(port) = hovered.flatten() {
                if !self.body_claims_press(n.id, w, port.point) {
                    return Some((n.id, port.side, port.t));
                }
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
        if let Some((node, side, _)) = self.wire_grip_at(p, xf) {
            self.wire_grips = Some(GripHover {
                node,
                hovered: Some(side),
            });
        }
    }

    pub(crate) fn paint_wire_grips(&self, painter: &egui::Painter, xf: &BoardXf) {
        let key = egui::Id::new("wire-blister-last");
        if let Some(grips) = self.wire_grips {
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
        let edge = xf.w2s(port_point(node, side, 0.5));
        let r = atlas_shell::canvas_scale::px(GRIP_RADIUS, xf.z);
        let normal = self.wire_host(node).outward(side, 0.5);
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
                    let Some(p) = end_point(&self.doc().scene, end) else {
                        continue;
                    };
                    if xf.w2s(p).distance(screen) <= grip_hit_radius(xf.z) {
                        if conn.crosstalk.is_some() {
                            self.toast("A crosstalk wire follows its messages; its ends stay put.");
                            return None;
                        }
                        if conn.binding.as_ref().is_some_and(|b| b.consumed) {
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
                if is_on_grip(&c.a, node_id, side) {
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
            let scene = &self.doc().scene;
            let nearest = ends
                .into_iter()
                .min_by(|a, b| {
                    let d = |item: &(NodeId, bool, Node)| {
                        let NodeKind::Connector(c) = &item.2.kind else {
                            return f32::INFINITY;
                        };
                        let end = if item.1 { &c.b } else { &c.a };
                        end_point(scene, end)
                            .map(|p| p.distance(world))
                            .unwrap_or(f32::INFINITY)
                    };
                    d(a).total_cmp(&d(b))
                })
                .expect("non-empty");
            if let NodeKind::Connector(c) = &nearest.2.kind {
                if c.binding.as_ref().is_some_and(|b| b.consumed) {
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

    // ----- painting -----

    /// Wire-drag preview: rubber-band bezier (solid when snapped), the snap
    /// highlight ring, and the modifier glyph (+ add / − detach).
    pub(crate) fn paint_wire_drag(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        wd: &WireDrag,
        mods: egui::Modifiers,
    ) {
        let palette = self.palette();
        if let WireMode::Add { from } = &wd.mode {
            let a = ConnectorEnd::Anchored {
                node: from.0,
                side: from.1,
                t: from.2,
            };
            let b = match wd.snap {
                Some((node, side, t)) => ConnectorEnd::Anchored { node, side, t },
                None => ConnectorEnd::Free {
                    point: [wd.cursor.x, wd.cursor.y],
                },
            };
            let scene = &self.doc().scene;
            if let Some(path) =
                connector_route_in_scene(scene, None, &a, &b, self.board_wire_routing)
            {
                let crosstalk = self.is_crosstalk_port(*from);
                let mut stroke = connector_drawn_stroke(if crosstalk {
                    slate_doc::crosstalk::wire_stroke()
                } else {
                    self.default_wire_stroke()
                });
                if !crosstalk {
                    stroke.color = super::board::to_rgba(palette.wire);
                }
                let path =
                    retreat_off_hosts(path, &a, &b, scene_wire_hosts(scene), stroke.width * 0.5);
                let color =
                    rgba32(stroke.color).gamma_multiply(if wd.snap.is_some() { 1.0 } else { 0.55 });
                paint_route_preview(
                    painter,
                    xf,
                    &path,
                    EStroke::new(atlas_shell::canvas_scale::px(stroke.width, xf.z), color),
                );
            }
        }
        // Snap highlight.
        if let Some((node, side, t)) = wd.snap {
            if let Some(n) = self.doc().scene.node(node) {
                let p = connector_anchor_on(n, side, t);
                let s = xf.w2s(Pos2::new(p[0], p[1]));
                painter.circle_stroke(s, 8.0, EStroke::new(2.0_f32, palette.accent));
            }
        }
        // Modifier glyph near the pointer.
        let glyph = if mods.ctrl && !mods.shift {
            Some("−")
        } else if mods.shift && !mods.ctrl {
            Some("+")
        } else {
            None
        };
        if let Some(g) = glyph {
            let at = xf.w2s(wd.cursor) + Vec2::new(14.0, -14.0);
            painter.circle_filled(at, 8.0, palette.card);
            painter.text(
                at,
                Align2::CENTER_CENTER,
                g,
                FontId::proportional(13.0),
                palette.ink,
            );
        }
    }

    /// Paint one connector node: tessellated through the path-mesh cache
    /// (geometry hash in the key), Faint = 40% opacity, filled-triangle
    /// arrowheads sized like the artifact, label centered at the midpoint.
    pub(crate) fn paint_connector(
        &mut self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        conn: &ConnectorNode,
    ) {
        // Hidden-anchor rule: unresolvable connectors are skipped entirely
        // (matches the artifact writer).
        let Some(path) = self.connector_path_visible(node.id, conn) else {
            return;
        };
        if conn.crosstalk.is_some() && self.crosstalk_duplicate(node.id) {
            return;
        }
        let (path, mut stroke) = drawn_connector(&self.doc().scene, path, conn);
        stroke.color = conn.paint_color(self.theme_wire_rgba());
        let opacity = (node.opacity
            * match conn.display {
                WireDisplay::Faint => FAINT_OPACITY,
                WireDisplay::Default => 1.0,
            })
        .clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(opacity);
        let base = fade(rgba32(stroke.color));

        if !stroke.is_none() {
            let bucket = board_path::zoom_bucket(xf.z);
            let key = connector_cache_key(
                &path,
                self.board_wire_routing,
                &stroke,
                conn.display,
                bucket,
            );
            let style = board_path::stroke_style_world(&stroke, xf.z);
            let feather = board_path::FEATHER_PX / xf.z.max(0.05);
            let kurbo = connector_path_kurbo(&path);
            let cached = self.path_mesh_cache.get_or_tessellate(node.id, key, || {
                vector_ink::stroke_mesh(&kurbo, &style, feather, board_path::curve_tolerance(xf.z))
            });
            let mesh = board_path::ink_mesh_to_epaint(&cached, xf, base, |c| c);
            painter.add(egui::Shape::mesh(mesh));
        }

        // Arrowheads: filled triangles, tip at the endpoint, base back along
        // the tangent into the curve; size matches the artifact.
        let arrow = |tip: [f32; 2], into: [f32; 2]| {
            let [a, b, c] = slate_doc::geom::arrow_head(tip, into, stroke.width);
            painter.add(egui::Shape::convex_polygon(
                vec![
                    xf.w2s(Pos2::new(a[0], a[1])),
                    xf.w2s(Pos2::new(b[0], b[1])),
                    xf.w2s(Pos2::new(c[0], c[1])),
                ],
                base,
                EStroke::NONE,
            ));
        };
        if conn.arrow_a {
            arrow(path.start(), path.start_dir());
        }
        if conn.arrow_b {
            arrow(path.end(), path.end_dir());
        }

        // Label at the curve midpoint (skipped while its inline edit is up).
        if self
            .wire_label_edit
            .as_ref()
            .is_some_and(|(id, _)| *id == node.id)
        {
            return;
        }
        if let Some(label) = conn.label.as_deref().filter(|l| !l.is_empty()) {
            let size = atlas_shell::canvas_scale::px(CONNECTOR_LABEL_SIZE, xf.z);
            if atlas_shell::canvas_text::legible(size) {
                let m = path.midpoint();
                atlas_shell::canvas_text::text(
                    painter,
                    xf.w2s(Pos2::new(m[0], m[1])),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::proportional(size),
                    base,
                );
            }
        }
    }

    /// Selection adornment for a connector: curve highlight + endpoint dots
    /// (draggable — detach), instead of the rect outline/handles.
    pub(crate) fn paint_connector_selection(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
    ) {
        let node = self
            .shape_properties
            .preview
            .iter()
            .find(|n| n.id == node.id)
            .unwrap_or(node);
        let NodeKind::Connector(conn) = &node.kind else {
            return;
        };
        let Some(path) = self.connector_path_visible(node.id, conn) else {
            return;
        };
        let anchors = [path.start(), path.end()];
        let (path, _) = drawn_connector(&self.doc().scene, path, conn);
        let palette = self.palette();
        paint_route_preview(
            painter,
            xf,
            &path,
            EStroke::new(atlas_shell::canvas_scale::px(1.5, xf.z), palette.select),
        );
        let r = atlas_shell::canvas_scale::px(4.5, xf.z);
        let ends = anchors.map(|p| xf.w2s(Pos2::new(p[0], p[1])));
        for p in ends {
            painter.circle_filled(p, r, palette.bg);
            painter.circle_stroke(
                p,
                r,
                EStroke::new(atlas_shell::canvas_scale::px(2.0, xf.z), palette.select),
            );
        }
    }

    // ----- derived rect sync -----

    /// Keep connector `Node.rect`s equal to their derived curve AABB so
    /// marquee/hit systems keep working. Runs only when the scene content
    /// generation moved (journal commits / undo / redo / tab switch), never
    /// per frame for all connectors (Art. II). Derived-cache write: not a
    /// journaled mutation and does not dirty the workbook by itself.
    pub(crate) fn sync_connector_rects(&mut self) {
        if self.connector_sync_gen == self.scene_gen {
            return;
        }
        self.connector_sync_gen = self.scene_gen;
        let routing = self.board_wire_routing;
        let scene = &self.doc().scene;
        if !scene
            .nodes
            .iter()
            .any(|n| matches!(n.kind, NodeKind::Connector(_)))
        {
            return;
        }
        let obstacles = scene_wire_obstacles(scene);
        let lanes = slate_doc::scene_ortho_lanes(scene);
        let updates: Vec<(NodeId, WorldRect)> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                let NodeKind::Connector(c) = &n.kind else {
                    return None;
                };
                let aabb = connector_aabb_routed(
                    c,
                    |id| scene.node(id).map(|n| self.wire_host(n)),
                    routing,
                    &obstacles,
                    lanes.get(&n.id).copied().unwrap_or_default(),
                )?;
                (aabb != n.rect).then_some((n.id, aabb))
            })
            .collect();
        if updates.is_empty() {
            return;
        }
        let tab = self.tab_mut();
        for (id, rect) in updates {
            if let Some(n) = tab.doc.scene.node_mut(id) {
                n.rect = rect;
            }
        }
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

pub(crate) fn paint_route_preview(
    painter: &egui::Painter,
    xf: &BoardXf,
    path: &ConnectorPath,
    stroke: EStroke,
) {
    match path {
        ConnectorPath::Bezier(bez) => {
            let pts = [
                xf.w2s(Pos2::new(bez.p0[0], bez.p0[1])),
                xf.w2s(Pos2::new(bez.c1[0], bez.c1[1])),
                xf.w2s(Pos2::new(bez.c2[0], bez.c2[1])),
                xf.w2s(Pos2::new(bez.p3[0], bez.p3[1])),
            ];
            painter.add(egui::Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    pts,
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
        }
        ConnectorPath::Orthogonal(pts) => {
            let screen: Vec<Pos2> = pts.iter().map(|p| xf.w2s(Pos2::new(p[0], p[1]))).collect();
            atlas_shell::dock::rounded_route(
                painter,
                &screen,
                atlas_shell::canvas_scale::px(ORTHO_CORNER_RADIUS, xf.z),
                stroke,
            );
        }
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
            cap_start: None,
            cap_end: None,
            texture: Default::default(),
        }
    }

    #[test]
    fn grip_hit_enlarges_only_outward() {
        let grip = Pos2::new(100.0, 100.0);
        let up = Vec2::new(0.0, -1.0);
        let (hit, out) = (GRIP_HIT_PX, GRIP_HIT_OUT_PX);
        assert!(grip_hit(grip, grip, up, hit, out));
        assert!(grip_hit(grip + Vec2::new(0.0, 6.0), grip, up, hit, out));
        assert!(!grip_hit(grip + Vec2::new(0.0, 30.0), grip, up, hit, out));
        assert!(grip_hit(grip + Vec2::new(0.0, -30.0), grip, up, hit, out));
        assert!(!grip_hit(grip + Vec2::new(0.0, -50.0), grip, up, hit, out));
        assert!(!grip_hit(grip + Vec2::new(30.0, 0.0), grip, up, hit, out));
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
        assert!((grip_hit_out_radius(zoom) - grown * 6.0).abs() < 1e-3);
    }

    #[test]
    fn ports_below_the_lod_are_not_visible() {
        assert!(port_disc_visible(GRIP_RADIUS, 1.0));
        assert!(!port_disc_visible(GRIP_RADIUS, 0.2));
        assert!(port_disc_visible(crate::app::board_flow::PORT_RADIUS, 0.4));
        assert!(!port_disc_visible(crate::app::board_flow::PORT_RADIUS, 0.2));
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
