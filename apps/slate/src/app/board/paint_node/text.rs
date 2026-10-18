//! Graphical text and sticky-note interpretation within the board painter.
use super::*;

impl SlateApp {
    pub(super) fn paint_text_node(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
        node: &Node,
        outline_s: &[Pos2],
        chrome: bool,
    ) {
        let NodeKind::Text(t) = &node.kind else {
            return;
        };
        let srect = xf.rect_w2s(node.rect);
        let z = xf.z;
        let alpha = node.opacity.clamp(0.0, 1.0);
        let fade = |c: Color32| c.gamma_multiply(alpha);
        // Sticky notes mirror the artifact's background on the node.
        if let Some(fill) = t.fill {
            if !chrome || self.media_interaction_shadow_fade(painter, node) <= 0.0 {
                Self::paint_sticky_shadow(painter, xf, node, srect, z, &fade);
            }
            if let Some(clip) = &node.clip {
                paint_clip_fill(painter, xf, node, clip, fade(rgba32(fill)));
            } else {
                painter.add(egui::Shape::convex_polygon(
                    outline_s.to_vec(),
                    fade(rgba32(fill)),
                    EStroke::NONE,
                ));
            }
        }
        stroke_outline(painter, outline_s, &t.stroke, z);
        if self
            .text_edit
            .as_ref()
            .is_some_and(|(edit_id, _)| *edit_id == node.id)
        {
            return;
        }
        // An agent note shows its reply until it is edited.
        let reply = self.agent_note_reply(node.id);
        let shown = reply.as_deref().unwrap_or(&t.text);
        let sticky = t.fill.is_some();
        let draw_size = if sticky {
            self.sticky_font_size(
                painter.ctx(),
                node.id,
                shown,
                t.family,
                t.size,
                node.rect.w,
                node.rect.h,
                t.align,
                t.style,
            )
        } else {
            t.size
        };
        if canvas_text::legible(canvas_text::authored_px(draw_size, z)) {
            let galley = styled_text_galley(
                painter.ctx(),
                shown,
                styled_typeface_font(painter.ctx(), t.family, draw_size, t.style),
                node.rect.w.max(0.0),
                t.align,
                fade(rgba32(t.color)),
                z,
                t.style,
            );
            // Match the editor's vertical centering of a sticky's cached galley.
            let dy = if sticky {
                ((srect.height() - galley.rect.height()) * 0.5).max(0.0)
            } else {
                0.0
            };
            let text_pos = srect.min + egui::vec2(0.0, dy);
            if let Some(clip) = &node.clip {
                paint_clipped_galley(painter, xf, node, clip, text_pos, &galley);
            } else {
                let pos = srect.center()
                    + egui::emath::Rot2::from_angle(node.rotation_deg.to_radians())
                        * (text_pos - srect.center());
                let mut shape = egui::epaint::TextShape::new(pos, galley, Color32::WHITE);
                shape.angle = node.rotation_deg.to_radians();
                painter
                    .with_clip_rect(Rect::from_points(outline_s).expand(2.0))
                    .add(shape);
            }
        }
        if t.agent.is_some() && !self.slate_nesting() {
            self.paint_agent_note(ui, painter, xf, node, srect);
        }
    }
}
