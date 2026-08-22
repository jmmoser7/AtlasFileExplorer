//! Bounding-box chrome: Windows-style hover resize, optional rotate.
//!
//! Select-tool edges and corners are live without a prior selection. Portals
//! and simple lines do not rotate. Aspect-ratio keys stay in `board_snap`
//! (Shift / Ctrl) — this module only decides what the pointer is aimed at.

use super::board::{BoardDrag, BoardXf};
use super::{board_handles, SlateApp};
use eframe::egui::{self, Pos2};
use slate_doc::scene::{Node, NodeKind};
use slate_doc::NodeId;

impl SlateApp {
    /// Portals stay axis-aligned; connectors and simple lines have no bbox
    /// rotate. Everything else with a rectangular frame can rotate.
    pub(crate) fn node_allows_rotation(node: &Node) -> bool {
        !matches!(
            node.kind,
            NodeKind::Portal(_) | NodeKind::Connector(_) | NodeKind::DockStrip(_)
        ) && !Self::node_uses_curve_grips(node)
    }

    fn node_offers_bbox_transform(&self, node: &Node) -> bool {
        if node.hidden || matches!(node.kind, NodeKind::Connector(_)) {
            return false;
        }
        if Self::node_uses_curve_grips(node) {
            return false;
        }
        if node.locked && !self.board_sel.contains(&node.id) {
            return false;
        }
        true
    }

    fn selection_allows_rotation(&self) -> bool {
        self.board_sel.iter().any(|id| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(Self::node_allows_rotation)
        })
    }

    /// Topmost bounding-box chrome under `screen`. `None` node id = the
    /// multi-selection group box.
    pub(crate) fn transform_hit_at(
        &self,
        screen: Pos2,
    ) -> Option<(Option<NodeId>, board_handles::BoardHitTarget)> {
        let xf = self.board_xf();
        if self.board_sel.len() >= 2 && !self.selection_all_simple_lines() {
            if let Some(gb) = self.board_group_bounds() {
                let geom = board_handles::selection_geom(&xf, gb, 0.0);
                if let Some(hit) =
                    board_handles::hit_test_chrome(screen, &geom, self.selection_allows_rotation())
                {
                    return Some((None, hit));
                }
            }
        }
        for n in self.doc().scene.nodes.iter().rev() {
            if !self.node_offers_bbox_transform(n) {
                continue;
            }
            let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
            if let Some(hit) =
                board_handles::hit_test_chrome(screen, &geom, Self::node_allows_rotation(n))
            {
                return Some((Some(n.id), hit));
            }
        }
        None
    }

    /// Hover cursors only. Selection is not required. Edge / rotate hover
    /// never paints selection chrome (P1.node.transform).
    pub(crate) fn hover_transform_chrome(
        &mut self,
        pointer: Option<Pos2>,
        xf: &BoardXf,
        ctx: &egui::Context,
        wire_grip_hovered: bool,
    ) {
        self.board_hover_hit = None;
        self.board_hover_node = None;
        let Some(p) = pointer else { return };
        if self.pointer_on_portal_maximize(p, xf) {
            return;
        }
        let Some((node, hit)) = self.transform_hit_at(p) else {
            return;
        };
        self.board_hover_hit = Some(hit);
        self.board_hover_node = node;
        if wire_grip_hovered {
            return;
        }
        let geom = match node {
            Some(id) => self
                .doc()
                .scene
                .node(id)
                .map(|n| board_handles::selection_geom(xf, n.rect, n.rotation_deg)),
            None => self
                .board_group_bounds()
                .map(|gb| board_handles::selection_geom(xf, gb, 0.0)),
        };
        let Some(geom) = geom else { return };
        match hit {
            board_handles::BoardHitTarget::Resize(h) => {
                ctx.set_cursor_icon(board_handles::cursor_for_resize(h, &geom));
            }
            board_handles::BoardHitTarget::Rotate(_) => {
                ctx.set_cursor_icon(board_handles::cursor_for_rotate());
            }
            board_handles::BoardHitTarget::Body => {}
        }
    }

    /// Body under the pointer, excluding edge / rotate chrome and the
    /// current selection. Edge hover is cursor-only.
    pub(crate) fn hover_preview_target(&self, world: Option<Pos2>) -> Option<NodeId> {
        if matches!(
            self.board_hover_hit,
            Some(board_handles::BoardHitTarget::Resize(_))
                | Some(board_handles::BoardHitTarget::Rotate(_))
        ) {
            return None;
        }
        let w = world?;
        let id = super::board_path::board_pick_node(&self.doc().scene, w.x, w.y, self.tab().cam.z)?;
        if self.board_sel.contains(&id) {
            return None;
        }
        let n = self.doc().scene.node(id)?;
        if !self.node_offers_bbox_transform(n) {
            return None;
        }
        Some(id)
    }

    pub(crate) fn tick_hover_preview(
        &mut self,
        target: Option<NodeId>,
        dt: f32,
        ctx: &egui::Context,
    ) {
        let tokens = atlas_shell::tokens::current().board_preview;
        if let Some(id) = target {
            self.board_hover_glow.entry(id).or_insert(0.0);
        }
        let mut animating = false;
        self.board_hover_glow.retain(|id, progress| {
            let goal = if target == Some(*id) { 1.0 } else { 0.0 };
            let span = if goal > *progress {
                tokens.highlight_in
            } else {
                tokens.highlight_out
            };
            let next = if span < 0.001 {
                goal
            } else if goal > *progress {
                (*progress + dt / span).min(1.0)
            } else {
                (*progress - dt / span).max(0.0)
            };
            if (next - *progress).abs() > 1e-4 && next != goal {
                animating = true;
            }
            *progress = next;
            next > 0.001
        });
        if animating {
            ctx.request_repaint();
        }
    }

    pub(crate) fn paint_hover_preview(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        color: egui::Color32,
    ) {
        let tokens = atlas_shell::tokens::current().board_preview;
        for (id, progress) in &self.board_hover_glow {
            if *progress <= 0.001 {
                continue;
            }
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            let eased = ease_in_out_cubic(*progress);
            let outline = self.node_screen_outline(painter.ctx(), xf, n);
            painter.add(egui::Shape::closed_line(
                outline,
                egui::Stroke::new(
                    atlas_shell::canvas_scale::px(tokens.hover_line_weight, xf.z),
                    color.gamma_multiply(tokens.hover_opacity * eased),
                ),
            ));
        }
    }

    /// Press on bounding-box chrome — selected, group, or an unselected node.
    /// A wire-grip press is not a resize, even though it sits on the edge.
    pub(crate) fn begin_transform_drag(&mut self, screen: Pos2, world: Pos2) -> Option<BoardDrag> {
        let xf = self.board_xf();
        if self.pointer_on_portal_maximize(screen, &xf) {
            return None;
        }
        if self.wire_grip_at(screen, &xf).is_some() {
            return None;
        }
        let (node, hit) = self.transform_hit_at(screen)?;
        match (node, hit) {
            (None, board_handles::BoardHitTarget::Resize(h)) => {
                let gb = self.board_group_bounds()?;
                let before: Vec<Node> = self
                    .board_sel
                    .iter()
                    .filter_map(|i| self.doc().scene.node(*i).cloned())
                    .collect();
                let ids: Vec<NodeId> = before.iter().map(|n| n.id).collect();
                Some(BoardDrag::GroupResize {
                    ids,
                    before,
                    group_before: gb,
                    handle: h as u8,
                })
            }
            (None, board_handles::BoardHitTarget::Rotate(_)) => {
                let gb = self.board_group_bounds()?;
                let before: Vec<Node> = self
                    .board_sel
                    .iter()
                    .filter_map(|i| self.doc().scene.node(*i).cloned())
                    .collect();
                let ids: Vec<NodeId> = before.iter().map(|n| n.id).collect();
                let (cx, cy) = gb.center();
                let start_angle = (world.y - cy).atan2(world.x - cx);
                Some(BoardDrag::GroupRotate {
                    ids,
                    before,
                    center: (cx, cy),
                    start_angle,
                })
            }
            (Some(id), board_handles::BoardHitTarget::Resize(h)) => {
                let n = self.doc().scene.node(id).cloned()?;
                if !self.board_sel.contains(&id) {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                Some(BoardDrag::Resize {
                    id,
                    before: n,
                    handle: h as u8,
                })
            }
            (Some(id), board_handles::BoardHitTarget::Rotate(_)) => {
                let n = self.doc().scene.node(id).cloned()?;
                if !Self::node_allows_rotation(&n) {
                    return None;
                }
                if !self.board_sel.contains(&id) {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                let (cx, cy) = n.rect.center();
                let start_angle = (world.y - cy).atan2(world.x - cx);
                Some(BoardDrag::Rotate {
                    id,
                    before: n,
                    start_angle,
                })
            }
            (_, board_handles::BoardHitTarget::Body) => None,
        }
    }
}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}
