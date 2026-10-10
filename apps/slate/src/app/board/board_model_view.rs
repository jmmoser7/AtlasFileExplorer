//! Model viewport paint.

use super::*;

impl SlateApp {
    /// A placed 3D model (`MediaKind::Model`, or a confirmed Enscape
    /// standalone): live offscreen render while the viewport is unlocked,
    /// cached frozen-camera poster while locked, item thumbnail while the
    /// poster is still being generated. Files with no mesh reader stay on
    /// this card and say so. Photo filters apply over the render (once per
    /// camera/size/adjust stamp — not per idle frame).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint_model_viewport(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        outline: &[Pos2],
        tex_vertices: &[(Pos2, Pos2)],
        srect: Rect,
        node_id: NodeId,
        name: &str,
        alpha: f32,
    ) {
        let tint = Color32::WHITE.gamma_multiply(alpha);
        let live = self.model3d.live.contains_key(&node_id);
        let adjust = self.model_adjust_for_paint(node_id);

        let live_tex = live
            .then(|| {
                self.model_live_texture(ui.ctx(), node_id, srect.width(), srect.height(), &adjust)
            })
            .flatten();
        // A live frame drawn before its slot reached egui shows next frame;
        // hold the poster meanwhile.
        let awaiting_slot = live_tex.is_none()
            && self
                .model3d
                .live
                .get(&node_id)
                .is_some_and(|vp| vp.rendered.is_some());
        if awaiting_slot {
            ui.ctx().request_repaint();
        }
        let rendered = if live && !awaiting_slot {
            live_tex
        } else {
            let poster = self
                .model_node_info(node_id)
                .and_then(|info| self.model_poster_texture(ui.ctx(), &info, &adjust));
            if poster.is_none() && !live {
                self.request_model_poster(node_id);
            }
            poster.map(|t| t.id())
        };
        let render_ready = rendered.is_some();

        // Mesh parse still running for this node's file? Drives the load bar
        // whenever the render (live frame or poster) is waiting on it.
        let parse_progress = if render_ready {
            None
        } else {
            self.model_node_info(node_id)
                .and_then(|info| self.model_parse_progress(&info.cache_key))
        };

        // While the render isn't ready, fall back to the item thumbnail
        // (atlas-core extracts the preview image embedded in .3dm files).
        let tex = rendered
            .or_else(|| {
                self.model_node_info(node_id).and_then(|info| {
                    self.model3d
                        .external
                        .contains(&info.cache_key)
                        .then(|| self.enscape_poster_texture(ui.ctx(), &info.cache_key))
                        .flatten()
                        .map(|t| t.id())
                })
            })
            .or_else(|| {
                let desired_px = srect.width().max(srect.height()) * ui.ctx().pixels_per_point();
                self.board_texture(
                    ui.ctx(),
                    node_id,
                    self.image_item(node_id)?,
                    &adjust,
                    desired_px,
                )
                .map(|t| t.id())
            });

        match tex {
            Some(tex) => {
                paint_node_texture_id(painter, tex, tex_vertices, tint);
            }
            None => {
                let palette = self.palette();
                painter.add(egui::Shape::convex_polygon(
                    outline.to_vec(),
                    palette.thumb_bg,
                    EStroke::NONE,
                ));
                // Distinguish "still working" from "this file has no meshes"
                // (the load bar overlay below carries the working state).
                if parse_progress.is_none() {
                    let msg = self
                        .model_node_info(node_id)
                        .and_then(|info| {
                            self.model_failure(&info.cache_key)
                                .map(|msg| atlas_shell::widgets::trunc(msg, 120))
                        })
                        .unwrap_or_else(|| {
                            format!(
                                "{} — preparing 3D view…",
                                atlas_shell::widgets::trunc(name, 18)
                            )
                        });
                    let z = self.board_xf().z;
                    let size = canvas_scale::px(11.0, z);
                    if canvas_text::legible(size) {
                        canvas_text::text(
                            painter,
                            srect.center(),
                            Align2::CENTER_CENTER,
                            msg,
                            FontId::proportional(size),
                            palette.sub,
                        );
                    }
                }
            }
        }

        // Load bar while the mesh parse blocks this node's render (live
        // unlock, or first poster generation for a locked node). The worker
        // reports byte-accurate checkpoints; the bar eases between them.
        if let Some(target) = parse_progress {
            let palette = self.palette();
            let shown = ui.ctx().animate_value_with_time(
                egui::Id::new(("slate_model_progress", node_id.0)),
                target,
                0.4,
            );
            let z = self.board_xf().z;
            let bar_w = srect.width() * 0.55;
            let bar_h = canvas_scale::px(5.0, z);
            let bar = Rect::from_center_size(srect.center(), Vec2::new(bar_w, bar_h));
            painter.rect_filled(
                bar.expand2(Vec2::new(
                    canvas_scale::px(8.0, z),
                    canvas_scale::px(7.0, z),
                )),
                canvas_scale::px(6.0, z),
                palette.card.gamma_multiply(0.85 * alpha),
            );
            painter.rect_filled(
                bar,
                bar_h * 0.5,
                palette.border_strong.gamma_multiply(alpha),
            );
            let mut fill = bar;
            fill.set_width(bar_w * shown.clamp(0.0, 1.0));
            painter.rect_filled(fill, bar_h * 0.5, palette.accent.gamma_multiply(alpha));
            let label = canvas_scale::px(10.5, z);
            if canvas_text::legible(label) && srect.height() > canvas_scale::px(52.0, z) {
                canvas_text::text(
                    painter,
                    bar.center_top() + Vec2::new(0.0, canvas_scale::px(-6.0, z)),
                    Align2::CENTER_BOTTOM,
                    "Preparing 3D view…",
                    FontId::proportional(label),
                    palette.sub.gamma_multiply(alpha),
                );
            }
            ui.ctx().request_repaint();
        }

        if live {
            // Accent ring: this viewport is live (consuming GPU + memory).
            let palette = self.palette();
            painter.add(egui::Shape::closed_line(
                outline.to_vec(),
                EStroke::new(canvas_scale::px(1.5, self.board_xf().z), palette.accent),
            ));
        }

        self.paint_model_wired_view_strip(ui, node_id, srect, self.board_xf().z);
    }
}
