//! Connector wires (keymap wave 2b, cluster B): edge grips, the Grasshopper
//! wire-drag grammar (add / Shift add / Ctrl detach / Ctrl+Shift move-all),
//! connector painting through the path-mesh cache, label editing, and the
//! derived-AABB sync that keeps `Node.rect` fresh for marquee/hit systems.
//!
//! See `docs/keymap/specs/connectors.md`. Geometry is derived, never stored
//! (`slate_doc::connector_bezier`); one gesture = one journaled step.

use super::board::{rgba32, BoardXf};
use super::{board_path, SlateApp};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Stroke as EStroke, Vec2};
use slate_doc::scene::{
    connector_anchor_point, connector_bezier, ConnectorBezier, ConnectorEnd, ConnectorNode, Dash,
    Node, NodeKind, Scene, SceneCmd, Side, Stroke, StrokeCap, StrokeJoin, WidthProfile,
    WireDisplay, WorldRect,
};
use slate_doc::NodeId;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use vector_ink::kurbo::BezPath;

/// Reveal the 4 side grips when the pointer is this close to a node edge.
pub const GRIP_REVEAL_PX: f32 = 8.0;
/// Press-hit radius on a grip dot.
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

const ALL_SIDES: [Side; 4] = [Side::Top, Side::Right, Side::Bottom, Side::Left];

/// Which grips are showing this frame: node + the grip under the pointer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripHover {
    pub node: NodeId,
    pub hovered: Option<Side>,
}

/// A live wire gesture (registered as `CancelLayer::ActiveOperation`).
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
    let p = connector_anchor_point(rect, side, 0.5);
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

/// Nearest side of `rect` to `p`, with the projected fraction along it.
fn nearest_side(rect: WorldRect, p: Pos2) -> (Side, f32, f32) {
    let mut best = (Side::Top, 0.5f32, f32::INFINITY);
    for side in ALL_SIDES {
        let (a, b) = match side {
            Side::Top => (
                Pos2::new(rect.x, rect.y),
                Pos2::new(rect.x + rect.w, rect.y),
            ),
            Side::Bottom => (
                Pos2::new(rect.x, rect.y + rect.h),
                Pos2::new(rect.x + rect.w, rect.y + rect.h),
            ),
            Side::Left => (
                Pos2::new(rect.x, rect.y),
                Pos2::new(rect.x, rect.y + rect.h),
            ),
            Side::Right => (
                Pos2::new(rect.x + rect.w, rect.y),
                Pos2::new(rect.x + rect.w, rect.y + rect.h),
            ),
        };
        let ab = b - a;
        let len2 = ab.length_sq().max(f32::EPSILON);
        let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
        let d = (p - (a + ab * t)).length();
        if d < best.2 {
            best = (side, t, d);
        }
    }
    best
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

/// Mesh-cache key: the *geometry* (all four control points) + stroke +
/// display + zoom bucket, so a moved endpoint node invalidates the cached
/// tessellation by construction (Art. II — tessellate on change only).
pub(crate) fn connector_cache_key(
    bez: &ConnectorBezier,
    stroke: &Stroke,
    display: WireDisplay,
    bucket: i64,
) -> u64 {
    let mut h = DefaultHasher::new();
    for p in [bez.p0, bez.c1, bez.c2, bez.p3] {
        p[0].to_bits().hash(&mut h);
        p[1].to_bits().hash(&mut h);
    }
    format!("{stroke:?}{display:?}").hash(&mut h);
    bucket.hash(&mut h);
    h.finish()
}

/// Stroke hit-test for connectors (used by the shared point pick). Skips
/// connectors whose anchored node is hidden — they are not painted either.
pub fn hit_connector(scene: &Scene, conn: &ConnectorNode, wx: f32, wy: f32, zoom: f32) -> bool {
    let rect_of = |id: NodeId| scene.node(id).filter(|n| !n.hidden).map(|n| n.rect);
    let Some(bez) = connector_bezier(&conn.a, &conn.b, rect_of) else {
        return false;
    };
    let path = connector_kurbo(&bez);
    let style = board_path::stroke_style_world(&conn.stroke, zoom);
    let slop = CONNECTOR_PICK_PX / zoom.max(0.05);
    vector_ink::hit_stroke(&path, &style, [wx, wy], slop)
}

/// The world point of one connector end (anchored ends resolve through the
/// current rects; hidden anchors still resolve for interaction purposes).
fn end_point(scene: &Scene, end: &ConnectorEnd) -> Option<Pos2> {
    match end {
        ConnectorEnd::Anchored { node, side, t } => scene.node(*node).map(|n| {
            let p = connector_anchor_point(n.rect, *side, *t);
            Pos2::new(p[0], p[1])
        }),
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

    /// The derived curve of a connector for painting/interacting (skipping
    /// hidden anchors, matching the artifact's hidden-anchor rule).
    pub(crate) fn connector_bez_visible(&self, conn: &ConnectorNode) -> Option<ConnectorBezier> {
        let scene = &self.doc().scene;
        connector_bezier(&conn.a, &conn.b, |id| {
            scene.node(id).filter(|n| !n.hidden).map(|n| n.rect)
        })
    }

    // ----- grips -----

    /// Per-frame grip hover: with the Select tool, the topmost visible
    /// non-connector node whose edge is within ~8 px of the pointer shows
    /// its 4 side grips (locked nodes included — wires may anchor to them).
    pub(crate) fn update_wire_grips(&mut self, pointer: Option<Pos2>, xf: &BoardXf) {
        self.wire_grips = None;
        let Some(p) = pointer else { return };
        let w = xf.s2w(p);
        let reveal = GRIP_REVEAL_PX / xf.z.max(0.05);
        for n in self.doc().scene.nodes.iter().rev() {
            if n.hidden || matches!(n.kind, NodeKind::Connector(_)) {
                continue;
            }
            if rect_edge_dist(n.rect, w) <= reveal {
                let hovered = ALL_SIDES.into_iter().find(|side| {
                    let g = xf.w2s(grip_point(n.rect, *side));
                    g.distance(p) <= GRIP_HIT_PX
                });
                self.wire_grips = Some(GripHover {
                    node: n.id,
                    hovered,
                });
                return;
            }
            // A node body fully under the pointer occludes edges behind it.
            if n.rect.contains_rotated(w.x, w.y, n.rotation_deg) {
                return;
            }
        }
    }

    pub(crate) fn paint_wire_grips(&self, painter: &egui::Painter, xf: &BoardXf) {
        let Some(grips) = self.wire_grips else { return };
        let Some(n) = self.doc().scene.node(grips.node) else {
            return;
        };
        let palette = self.palette();
        for side in ALL_SIDES {
            let g = xf.w2s(grip_point(n.rect, side));
            let hovered = grips.hovered == Some(side);
            let r = if hovered { 6.0 } else { 4.0 };
            painter.circle_filled(g, r, palette.bg);
            painter.circle_stroke(
                g,
                r,
                EStroke::new(if hovered { 2.0 } else { 1.4 }, palette.accent),
            );
        }
    }

    // ----- gesture begin / update / end -----

    /// Wire-drag start checks, called from `begin_gesture` (Select tool,
    /// after resize/group handles): endpoint dots of a selected connector
    /// first (FigJam-style detach), then edge grips with the modifier
    /// grammar. Returns `None` when the press is not a wire gesture.
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

        // Grip press on the currently gripped node.
        let grips = self.wire_grips?;
        let node = self.doc().scene.node(grips.node)?;
        let rect = node.rect;
        let side = ALL_SIDES
            .into_iter()
            .find(|side| xf.w2s(grip_point(rect, *side)).distance(screen) <= GRIP_HIT_PX)?;
        let from = (grips.node, side, 0.5f32);

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
                if is_on_grip(&c.a, grips.node, side) {
                    Some((n.id, false, n.clone()))
                } else if is_on_grip(&c.b, grips.node, side) {
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
            // Grips snap to t = 0.5 first.
            if let Some(side) = ALL_SIDES
                .into_iter()
                .find(|side| grip_point(n.rect, *side).distance(world) <= snap_w)
            {
                return Some((n.id, side, 0.5));
            }
            let (side, t, d) = nearest_side(n.rect, world);
            if d <= snap_w {
                return Some((n.id, side, t));
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
                    .map(|n| grip_point(n.rect, from.1)),
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
        let rect = slate_doc::scene::connector_aabb(&conn, |id| scene.node(id).map(|n| n.rect))
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
        let Some(target) = self.doc().scene.node(placed).map(|n| n.rect) else {
            return;
        };
        let from_pt = self
            .doc()
            .scene
            .node(pending.from.0)
            .map(|n| grip_point(n.rect, pending.from.1));
        let Some(from_pt) = from_pt else { return };
        // Nearest side of the placed node to the source grip.
        let side = ALL_SIDES
            .into_iter()
            .min_by(|a, b| {
                grip_point(target, *a)
                    .distance(from_pt)
                    .total_cmp(&grip_point(target, *b).distance(from_pt))
            })
            .expect("four sides");
        self.add_connector(
            ConnectorEnd::Anchored {
                node: pending.from.0,
                side: pending.from.1,
                t: pending.from.2,
            },
            ConnectorEnd::Anchored {
                node: placed,
                side,
                t: 0.5,
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
            if let Some(bez) = connector_bezier(&a, &b, |id| {
                scene.node(id).filter(|n| !n.hidden).map(|n| n.rect)
            }) {
                let color = rgba32(self.board_colors.fg).gamma_multiply(if wd.snap.is_some() {
                    1.0
                } else {
                    0.55
                });
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
                        EStroke::new(2.0, color),
                    ),
                ));
            }
        }
        // Snap highlight.
        if let Some((node, side, t)) = wd.snap {
            if let Some(n) = self.doc().scene.node(node) {
                let p = connector_anchor_point(n.rect, side, t);
                let s = xf.w2s(Pos2::new(p[0], p[1]));
                painter.circle_stroke(s, 8.0, EStroke::new(2.0, palette.accent));
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
        let Some(bez) = self.connector_bez_visible(conn) else {
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
            let key = connector_cache_key(&bez, &conn.stroke, conn.display, bucket);
            let style = board_path::stroke_style_world(&conn.stroke, xf.z);
            let feather = board_path::FEATHER_PX / xf.z.max(0.05);
            let path = connector_kurbo(&bez);
            let cached = self.path_mesh_cache.get_or_tessellate(node.id, key, || {
                vector_ink::stroke_mesh(&path, &style, feather, 0.25)
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
            arrow(bez.p0, bez.start_dir());
        }
        if conn.arrow_b {
            arrow(bez.p3, bez.end_dir());
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
            let m = bez.midpoint();
            painter.text(
                xf.w2s(Pos2::new(m[0], m[1])),
                Align2::CENTER_CENTER,
                label,
                FontId::proportional((CONNECTOR_LABEL_SIZE * xf.z).max(5.0)),
                base,
            );
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
        let Some(bez) = self.connector_bez_visible(conn) else {
            return;
        };
        let palette = self.palette();
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
                EStroke::new(1.5, palette.select),
            ),
        ));
        for p in [pts[0], pts[3]] {
            painter.circle_filled(p, 4.5, palette.bg);
            painter.circle_stroke(p, 4.5, EStroke::new(2.0, palette.select));
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
        let scene = &self.doc().scene;
        let updates: Vec<(NodeId, WorldRect)> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                let NodeKind::Connector(c) = &n.kind else {
                    return None;
                };
                let aabb =
                    slate_doc::scene::connector_aabb(c, |id| scene.node(id).map(|node| node.rect))?;
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
        let Some(bez) = self.connector_bez_visible(&conn) else {
            self.wire_label_edit = None;
            return;
        };
        let m = bez.midpoint();
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
        }
    }

    #[test]
    fn cache_key_is_stable_and_geometry_sensitive() {
        let a = bez([0.0, 0.0], [100.0, 0.0]);
        let k1 = connector_cache_key(&a, &stroke(), WireDisplay::Default, 8);
        let k2 = connector_cache_key(&a, &stroke(), WireDisplay::Default, 8);
        assert_eq!(k1, k2, "same geometry → same key (cache hit)");

        // An endpoint node's rect moved → different endpoint → new key.
        let moved = bez([0.0, 0.0], [120.0, 5.0]);
        assert_ne!(
            k1,
            connector_cache_key(&moved, &stroke(), WireDisplay::Default, 8)
        );
        // Faint and zoom bucket also key the tessellation.
        assert_ne!(
            k1,
            connector_cache_key(&a, &stroke(), WireDisplay::Faint, 8)
        );
        assert_ne!(
            k1,
            connector_cache_key(&a, &stroke(), WireDisplay::Default, 9)
        );
    }

    #[test]
    fn nearest_side_projects_fraction() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        let (side, t, d) = nearest_side(rect, Pos2::new(25.0, -4.0));
        assert_eq!(side, Side::Top);
        assert!((t - 0.25).abs() < 1e-4);
        assert!((d - 4.0).abs() < 1e-4);
        let (side, t, _) = nearest_side(rect, Pos2::new(103.0, 25.0));
        assert_eq!(side, Side::Right);
        assert!((t - 0.5).abs() < 1e-4);
    }

    #[test]
    fn rect_edge_distance_inside_and_out() {
        let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
        assert!((rect_edge_dist(rect, Pos2::new(3.0, 25.0)) - 3.0).abs() < 1e-4);
        assert!((rect_edge_dist(rect, Pos2::new(-4.0, 25.0)) - 4.0).abs() < 1e-4);
        assert!((rect_edge_dist(rect, Pos2::new(50.0, 25.0)) - 25.0).abs() < 1e-4);
    }
}
