//! Hover outlines share the active image-frame silhouette.
use super::*;

impl SlateApp {
    pub(crate) fn paint_hover_preview(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        color: egui::Color32,
    ) {
        let tokens = atlas_shell::tokens::current().board_preview.clone();
        for (id, progress) in &self.board_hover_glow {
            if *progress <= 0.001 {
                continue;
            }
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            if slate_doc::agent_chat::agent(n).is_some()
                || self.image_palette_frame(painter.ctx(), n.id).is_some()
            {
                continue;
            }
            if !self.settings.hover_highlight(n.kind.kind_name()) {
                continue;
            }
            let eased = ease_in_out_cubic(*progress);
            let stroke = egui::Stroke::new(
                atlas_shell::canvas_scale::px(tokens.hover_line_weight, xf.z),
                color.gamma_multiply(tokens.hover_opacity * eased),
            );
            if let NodeKind::Shape(s) = &n.kind {
                if let Some(path) = s.path.as_ref() {
                    if s.shape == ShapeKind::Path {
                        crate::app::board_path::paint_path_stroke_outline(
                            painter, xf, n, s, path, stroke,
                        );
                        continue;
                    }
                }
            }
            let outline = self.node_screen_outline(painter.ctx(), xf, n);
            painter.add(egui::Shape::closed_line(outline, stroke));
        }
    }
}
