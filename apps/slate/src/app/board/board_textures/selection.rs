//! Selection adornments over the model and shared image frame.
use super::*;

impl SlateApp {
    /// Subtle selection highlight that follows the painted shape, not its
    /// axis-aligned box. Paths use the stroke; closed primitives get a faint
    /// fill plus the silhouette. `rotate_hover` uses the single-select
    /// outline; rotation feedback belongs to the pointer alone.
    pub(crate) fn paint_selected_node(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        n: &Node,
        select_tint: Color32,
        outline_w: f32,
        rotate_hover: bool,
    ) {
        if self.field_selection() == Some(n.id) {
            return;
        }
        // Rectangular media and portals announce selection with their shared
        // soft shadow. Keep curve outlines and manipulation grips independent.
        if self.board_sel.len() == 1 && matches!(n.kind, NodeKind::Image(_) | NodeKind::Portal(_)) {
            self.paint_node_fillet_grip(painter, xf, n, self.palette().sub);
            return;
        }
        let image_frame = self.image_palette_frame(painter.ctx(), n.id);
        if slate_doc::agent_chat::agent(n).is_some() && image_frame.is_none() {
            return;
        }
        if matches!(n.kind, NodeKind::Connector(_)) {
            self.paint_connector_selection(painter, xf, n);
            return;
        }
        if Self::node_uses_curve_grips(n) {
            self.paint_line_grips(painter, xf, n);
            return;
        }
        if let NodeKind::Shape(s) = &n.kind {
            if let Some(path) = s.path.as_ref() {
                if !path.is_empty() {
                    board_path::paint_path_stroke_outline(
                        painter,
                        xf,
                        n,
                        s,
                        path,
                        EStroke::new(outline_w, select_tint),
                    );
                    self.paint_node_fillet_grip(painter, xf, n, select_tint);
                    return;
                }
            }
        }
        let outline = self.node_screen_outline(painter.ctx(), xf, n);
        if outline.len() >= 3 && slate_doc::agent_chat::agent(n).is_none() {
            painter.add(egui::Shape::convex_polygon(
                outline.clone(),
                select_tint.gamma_multiply(0.16),
                EStroke::NONE,
            ));
        }
        let selection_outline = image_frame
            .as_ref()
            .and_then(|(frame, _)| frame.selection.first())
            .unwrap_or(&outline);
        let painter = if let Some((_, layer)) = &image_frame {
            let mut exterior = painter.clone().with_layer_id(*layer);
            // The editor may fade the image tint; its outside selection ring
            // remains visible without covering the authored frame stroke.
            exterior.set_opacity(1.0);
            exterior
        } else {
            painter.clone()
        };
        if let Some((frame, _)) = &image_frame {
            for outline in frame.selection.iter().skip(1) {
                painter.add(egui::Shape::closed_line(
                    outline.clone(),
                    EStroke::new(outline_w, select_tint),
                ));
            }
        }
        if rotate_hover {
            let geom = board_handles::selection_geom(xf, n.rect, n.rotation_deg);
            board_handles::paint_selection(
                &painter,
                &geom,
                selection_outline,
                select_tint,
                outline_w,
            );
            self.paint_node_fillet_grip(&painter, xf, n, select_tint);
        } else {
            painter.add(egui::Shape::closed_line(
                selection_outline.clone(),
                EStroke::new(outline_w, select_tint),
            ));
            self.paint_node_fillet_grip(&painter, xf, n, select_tint);
        }
        self.paint_node_sides_glyphs(&painter, xf, n, select_tint);
    }
}
