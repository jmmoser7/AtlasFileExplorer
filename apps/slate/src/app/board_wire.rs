//! Connector wires (keymap wave 2b, cluster B): edge grips, the Grasshopper
//! wire-drag grammar (add / Shift add / Ctrl detach / Ctrl+Shift move-all),
//! connector painting through the path-mesh cache, label editing, and the
//! derived-AABB sync that keeps `Node.rect` fresh for marquee/hit systems.
//!
//! See `docs/keymap/specs/connectors.md`. Geometry is derived, never stored
//! (`slate_doc::connector_route`); one gesture = one journaled step.

use super::board::{rgba32, BoardXf};
use super::{board_path, SlateApp};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    ConnectorBezier, ConnectorEnd, ConnectorNode, Dash, Node, NodeKind, Scene, SceneCmd, Side,
    Stroke, StrokeCap, StrokeJoin, WidthProfile, WireDisplay, WorldRect,
};
use slate_doc::wire::{
    connector_aabb_routed, connector_route_in_scene, filleted_polyline, scene_wire_obstacles,
    ConnectorPath, OrthoLane, PathCmd, WireRouting, ORTHO_CORNER_RADIUS,
};
use slate_doc::{connector_anchor_on, NodeId, WireHost};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use vector_ink::kurbo::BezPath;

/// Press-hit / hover-preview radius on a grip dot (screen px slop).
pub const GRIP_HIT_PX: f32 = 8.0;
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

/// Wire released on empty canvas: the palette opens there and the placed
/// node auto-connects to its nearest side; dismissing cancels the wire.
#[derive(Clone, Copy)]
pub struct PendingWire {
    pub from: (NodeId, Side, f32),
}

// ---------- pure geometry helpers ----------

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
    let Some(path) = connector_route_in_scene(scene, Some(id), &conn.a, &conn.b, routing) else {
        return false;
    };
    let kurbo = connector_path_kurbo(&path);
    let style = board_path::stroke_style_world(&conn.stroke, zoom);
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

fn is_on_grip(end: &ConnectorEnd, node: NodeId, side: Side) -> bool {
    matches!(end, ConnectorEnd::Anchored { node: n, side: s, .. } if *n == node && *s == side)
}

// ---------- SlateApp: wires ----------

impl SlateApp {
    fn default_wire_stroke(&self) -> Stroke {
        Stroke {
            width: 2.0,
            color: self.board_colors.fg,
            dash: Dash::Solid,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            profile: WidthProfile::Uniform,
        }
    }

    pub(crate) fn persist_wire_routing(&mut self) {
        self.settings.board_wire_routing = self.board_wire_routing;
        self.settings.save();
        self.connector_sync_gen = 0;
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
            self.board_wire_routing,
        )
    }

    // ----- grips -----

    /// Grip under `screen`, independent of hover state. Topmost node wins;
    /// a body under the pointer occludes grips behind it. Used by both the
    /// hover preview and press-to-wire so a drag that has already left the
    /// dot still starts a wire from the press origin (resize must not win).
    pub(crate) fn wire_grip_at(&self, screen: Pos2, xf: &BoardXf) -> Option<(NodeId, Side, f32)> {
        let w = xf.s2w(screen);
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden || matches!(n.kind, NodeKind::Connector(_)) {
                continue;
            }
            let host = WireHost::from_node(n);
            let hovered = host.ports().into_iter().find(|port| {
                let g = xf.w2s(Pos2::new(port.point[0], port.point[1]));
                g.distance(screen) <= GRIP_HIT_PX
            });
            if let Some(port) = hovered {
                return Some((n.id, port.side, port.t));
            }
            if host.is_area() && n.rect.contains_rotated(w.x, w.y, n.rotation_deg) {
                return None;
            }
        }
        None
    }

    /// Per-frame grip hover: with the Select tool, only the grip whose
    /// midpoint is within [`GRIP_HIT_PX`] of the pointer previews (locked
    /// nodes included — wires may anchor to them). An edge between grips
    /// is inert. A node body under the pointer occludes grips behind it.
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
        let Some(grips) = self.wire_grips else { return };
        let Some(side) = grips.hovered else { return };
        let Some(n) = self.doc().scene.node(grips.node) else {
            return;
        };
        let palette = self.palette();
        let g = xf.w2s(port_point(n, side, 0.5));
        let r = atlas_shell::canvas_scale::px(6.0, xf.z);
        painter.circle_filled(g, r, palette.bg);
        painter.circle_stroke(
            g,
            r,
            EStroke::new(atlas_shell::canvas_scale::px(2.0, xf.z), palette.accent),
        );
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
                    if xf.w2s(p).distance(screen) <= GRIP_HIT_PX {
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

        let mode = if mods.ctrl && mods.shift && !ends.is_empty() {
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

    /// Snap target for a live wire drag: topmost visible non-connector node
    /// (excluding `exclude`) whose grip or edge is within 14 px screen.
    fn wire_snap_target(
        &self,
        world: Pos2,
        exclude: Option<NodeId>,
    ) -> Option<(NodeId, Side, f32)> {
        let z = self.tab().cam.z.max(0.05);
        let snap_w = WIRE_SNAP_PX / z;
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden || matches!(n.kind, NodeKind::Connector(_)) || Some(n.id) == exclude {
                continue;
            }
            let host = WireHost::from_node(n);
            if let Some(port) = host
                .ports()
                .into_iter()
                .find(|port| Pos2::new(port.point[0], port.point[1]).distance(world) <= snap_w)
            {
                return Some((n.id, port.side, port.t));
            }
            let snap = host.snap([world.x, world.y]);
            if snap.dist <= snap_w {
                return Some((n.id, snap.side, snap.t));
            }
        }
        None
    }

    /// Live wire drag: track the (ortho-adjusted) cursor, resolve the snap
    /// target, and let detached ends follow (live scene mutation; the net
    /// effect journals on release, per the board's gesture convention).
    pub(crate) fn wire_drag_update(&mut self, wd: &mut WireDrag, world: Pos2, shift: bool) {
        let mut cursor = world;
        // Ortho on wire drags uses the F8 toggle only: Shift already means
        // "add" in the wire grammar (deliberate deviation, documented).
        if self.board_ortho {
            let origin = match &wd.mode {
                WireMode::Add { from } => self
                    .doc()
                    .scene
                    .node(from.0)
                    .map(|n| port_point(n, from.1, from.2)),
                _ => None,
            };
            if let Some(o) = origin {
                cursor = super::board_snap::ortho_snap_point(o, world);
                self.ortho_feedback = Some((o, super::board_snap::ortho_axis(world - o)));
            }
        }
        let _ = shift;
        wd.cursor = cursor;
        let exclude = match &wd.mode {
            WireMode::Add { from } => Some(from.0),
            _ => None,
        };
        wd.snap = self.wire_snap_target(cursor, exclude);
        if wd.snap.is_none() {
            let from = match &wd.mode {
                WireMode::Add { from } => self
                    .doc()
                    .scene
                    .node(from.0)
                    .map(|n| port_point(n, from.1, from.2)),
                _ => None,
            };
            let skip: Vec<NodeId> = exclude.into_iter().collect();
            cursor = self.resolve_point_snap(cursor, &skip, from, false, from.is_some());
            wd.cursor = cursor;
        }

        // Detached ends follow the cursor (or preview-anchor onto the snap).
        let live_end = |snap: Option<(NodeId, Side, f32)>, cursor: Pos2| match snap {
            Some((node, side, t)) => ConnectorEnd::Anchored { node, side, t },
            None => ConnectorEnd::Free {
                point: [cursor.x, cursor.y],
            },
        };
        match &wd.mode {
            WireMode::Detach { conn, end_b, .. } => {
                let (conn, end_b) = (*conn, *end_b);
                let end = live_end(wd.snap, cursor);
                if let Some(NodeKind::Connector(c)) =
                    self.doc_mut().scene.node_mut(conn).map(|n| &mut n.kind)
                {
                    if end_b {
                        c.b = end;
                    } else {
                        c.a = end;
                    }
                }
            }
            WireMode::MoveAll { items } => {
                let items: Vec<(NodeId, bool)> = items.iter().map(|i| (i.0, i.1)).collect();
                let end = live_end(wd.snap, cursor);
                for (id, end_b) in items {
                    if let Some(NodeKind::Connector(c)) =
                        self.doc_mut().scene.node_mut(id).map(|n| &mut n.kind)
                    {
                        if end_b {
                            c.b = end;
                        } else {
                            c.a = end;
                        }
                    }
                }
            }
            WireMode::Add { .. } => {}
        }
    }

    /// Release: journal the net effect (Add / Patch / Patch group), or open
    /// the palette for the connect-to-placed flow.
    pub(crate) fn finish_wire_drag(&mut self, wd: WireDrag) {
        match wd.mode {
            WireMode::Add { from } => match wd.snap {
                Some((node, side, t)) => {
                    self.add_connector(
                        ConnectorEnd::Anchored {
                            node: from.0,
                            side: from.1,
                            t: from.2,
                        },
                        ConnectorEnd::Anchored { node, side, t },
                    );
                }
                None => {
                    // Releasing back on the source node cancels quietly.
                    let on_source = self
                        .doc()
                        .scene
                        .node(from.0)
                        .is_some_and(|n| n.rect.contains(wd.cursor.x, wd.cursor.y));
                    if on_source {
                        return;
                    }
                    // Blueprint flow: palette at the release point,
                    // placeables ranked first; placing auto-connects.
                    self.wire_pending = Some(PendingWire { from });
                    let screen = self.board_xf().w2s(wd.cursor);
                    self.open_board_palette(screen, wd.cursor);
                }
            },
            WireMode::Detach { conn, before, .. } => {
                if let Some(after) = self.doc().scene.node(conn).cloned() {
                    if after != before {
                        self.tab_mut().journal.record(vec![SceneCmd::Patch {
                            before: Box::new(before),
                            after: Box::new(after),
                        }]);
                        self.tab_mut().dirty = true;
                        self.note_scene_change();
                        self.push_history(
                            atlas_commands::CommandId("board.wire.detach"),
                            Some(
                                if wd.snap.is_some() {
                                    "rewired"
                                } else {
                                    "freed"
                                }
                                .into(),
                            ),
                        );
                    }
                }
            }
            WireMode::MoveAll { items } => {
                if wd.snap.is_none() {
                    // Release on empty cancels: restore every end.
                    self.restore_wire_nodes(&items);
                    return;
                }
                let cmds: Vec<SceneCmd> = items
                    .iter()
                    .filter_map(|(id, _, before)| {
                        let after = self.doc().scene.node(*id)?.clone();
                        (after != *before).then(|| SceneCmd::Patch {
                            before: Box::new(before.clone()),
                            after: Box::new(after),
                        })
                    })
                    .collect();
                if !cmds.is_empty() {
                    let n = cmds.len();
                    self.tab_mut().journal.record(cmds);
                    self.tab_mut().dirty = true;
                    self.note_scene_change();
                    self.push_history(
                        atlas_commands::CommandId("board.wire.move_all"),
                        Some(format!("{n} wire(s)")),
                    );
                }
            }
        }
    }

    /// Esc during a wire drag: restore live-mutated ends, journal nothing.
    pub(crate) fn cancel_wire_drag(&mut self, wd: WireDrag) {
        match wd.mode {
            WireMode::Add { .. } => {}
            WireMode::Detach { before, conn, .. } => {
                self.restore_wire_nodes(&[(conn, false, before)]);
            }
            WireMode::MoveAll { items } => self.restore_wire_nodes(&items),
        }
    }

    fn restore_wire_nodes(&mut self, items: &[(NodeId, bool, Node)]) {
        let tab = self.tab_mut();
        for (id, _, before) in items {
            if let Some(n) = tab.doc.scene.node_mut(*id) {
                *n = before.clone();
            }
        }
    }

    /// Journaled connector Add (stroke = fg default, no arrows).
    pub(crate) fn add_connector(&mut self, a: ConnectorEnd, b: ConnectorEnd) -> Option<NodeId> {
        let stroke = self.default_wire_stroke();
        let conn = ConnectorNode {
            a,
            b,
            stroke,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: WireDisplay::Default,
        };
        let scene = &self.doc().scene;
        let obstacles = scene_wire_obstacles(scene);
        let rect = connector_aabb_routed(
            &conn,
            |id| scene.node(id).map(WireHost::from_node),
            self.board_wire_routing,
            &obstacles,
            OrthoLane::default(),
        )
        .unwrap_or(WorldRect::new(0.0, 0.0, 1.0, 1.0));
        let node = self
            .doc_mut()
            .scene
            .build_node(rect, NodeKind::Connector(conn));
        let id = node.id;
        let ids = self.add_nodes(vec![node]);
        if ids.is_empty() {
            return None;
        }
        self.push_history(
            atlas_commands::CommandId("board.wire.add"),
            Some("connected".into()),
        );
        Some(id)
    }

    /// Palette follow-up: a node placed while a wire was pending
    /// auto-connects from the stored grip to the placed node's nearest side.
    pub(crate) fn resolve_pending_wire(&mut self, placed: NodeId) {
        let Some(pending) = self.wire_pending.take() else {
            return;
        };
        let Some(target) = self.doc().scene.node(placed).cloned() else {
            return;
        };
        let from_pt = self
            .doc()
            .scene
            .node(pending.from.0)
            .map(|n| port_point(n, pending.from.1, pending.from.2));
        let Some(from_pt) = from_pt else { return };
        let host = WireHost::from_node(&target);
        let port = host
            .ports()
            .into_iter()
            .min_by(|a, b| {
                Pos2::new(a.point[0], a.point[1])
                    .distance(from_pt)
                    .total_cmp(&Pos2::new(b.point[0], b.point[1]).distance(from_pt))
            })
            .expect("every host has ports");
        self.add_connector(
            ConnectorEnd::Anchored {
                node: pending.from.0,
                side: pending.from.1,
                t: pending.from.2,
            },
            ConnectorEnd::Anchored {
                node: placed,
                side: port.side,
                t: port.t,
            },
        );
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
                let color = rgba32(self.board_colors.fg).gamma_multiply(if wd.snap.is_some() {
                    1.0
                } else {
                    0.55
                });
                paint_route_preview(painter, xf, &path, EStroke::new(2.0_f32, color));
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
        let opacity = (node.opacity
            * match conn.display {
                WireDisplay::Faint => FAINT_OPACITY,
                WireDisplay::Default => 1.0,
            })
        .clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(opacity);
        let base = fade(rgba32(conn.stroke.color));

        if !conn.stroke.is_none() {
            let bucket = board_path::zoom_bucket(xf.z);
            let key = connector_cache_key(
                &path,
                self.board_wire_routing,
                &conn.stroke,
                conn.display,
                bucket,
            );
            let style = board_path::stroke_style_world(&conn.stroke, xf.z);
            let feather = board_path::FEATHER_PX / xf.z.max(0.05);
            let kurbo = connector_path_kurbo(&path);
            let cached = self.path_mesh_cache.get_or_tessellate(node.id, key, || {
                vector_ink::stroke_mesh(&kurbo, &style, feather, 0.25)
            });
            let mesh = board_path::ink_mesh_to_epaint(&cached, xf, base, |c| c);
            painter.add(egui::Shape::mesh(mesh));
        }

        // Arrowheads: filled triangles, tip at the endpoint, base back along
        // the tangent into the curve; size matches the artifact.
        let arrow_len = (conn.stroke.width * 4.0).max(10.0);
        let arrow = |tip: [f32; 2], into: [f32; 2]| {
            let base_pt = [tip[0] + into[0] * arrow_len, tip[1] + into[1] * arrow_len];
            let half = arrow_len * 0.4;
            let perp = [-into[1], into[0]];
            let b1 = [base_pt[0] + perp[0] * half, base_pt[1] + perp[1] * half];
            let b2 = [base_pt[0] - perp[0] * half, base_pt[1] - perp[1] * half];
            painter.add(egui::Shape::convex_polygon(
                vec![
                    xf.w2s(Pos2::new(tip[0], tip[1])),
                    xf.w2s(Pos2::new(b1[0], b1[1])),
                    xf.w2s(Pos2::new(b2[0], b2[1])),
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
        let NodeKind::Connector(conn) = &node.kind else {
            return;
        };
        let Some(path) = self.connector_path_visible(node.id, conn) else {
            return;
        };
        let palette = self.palette();
        paint_route_preview(
            painter,
            xf,
            &path,
            EStroke::new(atlas_shell::canvas_scale::px(1.5, xf.z), palette.select),
        );
        let r = atlas_shell::canvas_scale::px(4.5, xf.z);
        let ends = [
            xf.w2s(Pos2::new(path.start()[0], path.start()[1])),
            xf.w2s(Pos2::new(path.end()[0], path.end()[1])),
        ];
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
                    |id| scene.node(id).map(WireHost::from_node),
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

fn paint_route_preview(
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
        }
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
