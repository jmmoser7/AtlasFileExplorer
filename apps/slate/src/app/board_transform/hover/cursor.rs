//! Cursor feedback for the hovered transform control.
use super::*;

impl SlateApp {
    /// Hover cursors only. Selection is not required. Edge / rotate hover
    /// never paints selection chrome (P1.node.transform).
    pub(crate) fn hover_transform_chrome(
        &mut self,
        pointer: Option<Pos2>,
        xf: &BoardXf,
        ctx: &egui::Context,
        wire_grip_hovered: bool,
    ) {
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_transform_entry");
        self.board_hover_hit = None;
        self.board_hover_node = None;
        self.board_sides_hover = None;
        let Some(p) = pointer else { return };
        let maximize = self.pointer_on_portal_maximize(p, xf);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_portal_maximize");
        if maximize {
            return;
        }
        let fillet = self.fillet_grip_hit_at(p);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_fillet_grip");
        if let Some((id, vertex)) = fillet {
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
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_polygon_sides");
        let glyph = self.polygon_sides_glyph_at(p, xf).is_some();
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_polygon_glyph");
        if glyph {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            return;
        }
        let curve = self.curve_grip_under(p);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_curve_grip");
        if curve {
            return;
        }
        let transform = self.transform_hit_at(p);
        #[cfg(test)]
        crate::app::board::brush_prof::lap("hover_transform_hit");
        let Some((node, hit)) = transform else {
            return;
        };
        if hit == board_handles::BoardHitTarget::Body {
            self.board_hover_hit = Some(hit);
            self.board_hover_node = node;
            ctx.set_cursor_icon(egui::CursorIcon::Move);
            return;
        }
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
                .map(|n| self.node_interaction_geom(n, xf)),
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
