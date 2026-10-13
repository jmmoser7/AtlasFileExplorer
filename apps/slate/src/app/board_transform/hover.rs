//! Transform hover ownership and cursor feedback.
use super::*;

impl SlateApp {
    /// Keep a small object's visible body available for moving when its
    /// ordinary transform hit bands cover the whole object.
    pub(crate) fn tiny_body_owns_pointer(&self, screen: Pos2) -> Option<NodeId> {
        let xf = self.board_xf();
        let world = xf.s2w(screen);
        let id = self.board_pick_node(world.x, world.y)?;
        let n = self.doc().scene.node(id)?;
        // Curves keep their explicit endpoint/anchor edit contract.
        if n.is_frame()
            || Self::node_uses_curve_grips(n)
            || self.curve_grips_of(id).is_some()
            || !n.rect.contains_rotated(world.x, world.y, n.rotation_deg)
        {
            return None;
        }
        let band = canvas_scale::hit_px(board_handles::CORNER_HIT_PX, xf.z);
        (n.rect.w.min(n.rect.h) * xf.z <= 2.0 * band).then_some(id)
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
        self.board_sides_hover = None;
        let Some(p) = pointer else { return };
        if self.pointer_on_portal_maximize(p, xf) {
            return;
        }
        if let Some(id) = self.tiny_body_owns_pointer(p) {
            self.board_hover_hit = Some(board_handles::BoardHitTarget::Body);
            self.board_hover_node = Some(id);
            ctx.set_cursor_icon(egui::CursorIcon::Move);
            return;
        }
        if let Some((id, vertex)) = self.fillet_grip_hit_at(p) {
            self.board_hover_hit = Some(board_handles::BoardHitTarget::FilletRadius);
            self.board_hover_node = Some(id);
            self.board_hover_grip_vertex = vertex;
            if let Some(edge) = self
                .doc()
                .scene
                .node(id)
                .and_then(|n| self.node_grip_edge(n, vertex))
            {
                ctx.set_cursor_icon(board_handles::cursor_along(Vec2::from(edge.dir)));
            }
            return;
        }
        self.board_sides_hover = self.polygon_sides_hover_at(p, xf);
        if self.polygon_sides_glyph_at(p, xf).is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            return;
        }
        if self.curve_grip_under(p) {
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
            board_handles::BoardHitTarget::FilletRadius => {}
        }
    }
}
