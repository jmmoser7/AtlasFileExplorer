//! Model status, the Enscape window, and measurements.

use super::*;

impl SlateApp {
    /// Hover caption along the bottom of a model card.
    pub(super) fn paint_model_status_hint(
        &self,
        ui: &egui::Ui,
        xf: &BoardXf,
        srect: Rect,
        text: &str,
    ) {
        let painter = ui.painter_at(self.canvas_rect);
        let z = xf.z;
        let size = canvas_scale::px(10.5, z);
        if !canvas_text::legible(size) {
            return;
        }
        let pos = srect.center_bottom() + Vec2::new(0.0, canvas_scale::px(-8.0, z));
        let laid = canvas_text::layout_no_wrap(
            &painter,
            text.into(),
            FontId::proportional(size),
            Color32::from_white_alpha(235),
        );
        let sz = laid.size();
        let bg = Rect::from_center_size(
            pos - Vec2::new(0.0, sz.y * 0.5),
            sz + Vec2::new(canvas_scale::px(12.0, z), canvas_scale::px(6.0, z)),
        );
        if bg.width() < srect.width() {
            painter.rect_filled(bg, bg.height() * 0.5, Color32::from_black_alpha(150));
            laid.paint(
                &painter,
                bg.center() - sz * 0.5,
                Color32::from_white_alpha(235),
            );
        }
    }

    /// Keep a running Enscape window matched to its card. A click outside
    /// the card, or Escape, parks it: the process stays up and the card
    /// shows the last frame until the next double-click.
    #[cfg_attr(not(windows), allow(clippy::needless_return))] // The placement block follows.
    pub(super) fn place_enscape_window(&mut self, ui: &egui::Ui, xf: &BoardXf) {
        let Some(node) = self.enscape_shown_node() else {
            return;
        };
        let Some(info) = self.model_node_info(node) else {
            return;
        };
        let srect = xf.rect_w2s(info.rect).intersect(self.canvas_rect);
        let settled = self.enscape_shown_for() > std::time::Duration::from_millis(400);
        let leave = ui.ctx().input(|i| {
            let outside = i.pointer.interact_pos().is_some_and(|p| !srect.contains(p));
            settled
                && (i.key_pressed(egui::Key::Escape) || (i.pointer.primary_pressed() && outside))
        });
        if leave {
            self.park_enscape();
            return;
        }
        #[cfg(windows)]
        {
            let ppp = ui.ctx().pixels_per_point();
            let rect = (srect.width() >= 8.0 && srect.height() >= 8.0).then_some((
                (srect.min.x * ppp) as i32,
                (srect.min.y * ppp) as i32,
                (srect.width() * ppp) as i32,
                (srect.height() * ppp) as i32,
            ));
            self.place_active_enscape(rect);
        }
    }

    /// One caption along the bottom of a 3D model node. Frozen and hovered:
    /// how to enter. Live: the measure prompt, then the auto-lock countdown
    /// (`model3d::AUTO_LOCK`). The node carries no other in-frame chrome;
    /// display and measure live on its selection strip.
    pub(super) fn model_status_hints(&mut self, ui: &mut egui::Ui, xf: &BoardXf) {
        let pointer = ui.ctx().pointer_latest_pos();
        for info in self.model_nodes() {
            // Hidden nodes show no chrome either.
            if self.doc().scene.node(info.node).is_none_or(|n| n.hidden) {
                continue;
            }
            let srect = xf.rect_w2s(info.rect);
            if !srect.intersects(self.canvas_rect) {
                continue;
            }
            let live = self.model3d.live.contains_key(&info.node);
            let external = self.model3d.external.contains(&info.cache_key);
            let hovered = pointer.is_some_and(|p| srect.contains(p));
            if !live && !hovered {
                continue;
            }
            if external {
                let hint = if cfg!(windows) {
                    "Double-click to walk. Click away to keep it ready."
                } else {
                    "Not on this computer"
                };
                self.paint_model_status_hint(ui, xf, srect, hint);
                continue;
            }
            if !live {
                let failed = self.model_failure(&info.cache_key).is_some();
                if !failed && self.board_drag.is_none() {
                    let text = if srect.width() >= 150.0 {
                        "Double-click to enter 3D"
                    } else {
                        "2×click: 3D"
                    };
                    self.paint_model_status_hint(ui, xf, srect, text);
                }
                continue;
            }
            let Some(vp) = self.model3d.live.get(&info.node) else {
                continue;
            };
            let left = super::super::model3d::AUTO_LOCK.saturating_sub(vp.last_interact.elapsed());
            let text = if vp.tool == model3d::ModelViewportTool::MeasureDistance {
                if vp.measure_first.is_some() {
                    "Pick the second point".to_string()
                } else {
                    "Pick two points to measure".to_string()
                }
            } else if left <= std::time::Duration::from_secs(10) && vp.rendered.is_some() {
                format!("Freezes in {} s", left.as_secs().max(1))
            } else {
                continue;
            };
            self.paint_model_status_hint(ui, xf, srect, &text);
        }
    }

    /// Dimension lines for point-to-point measurements. Completed ones stay
    /// on the live viewport after Measure returns to Navigate; lock clears them.
    pub(super) fn paint_model_measurements(&self, painter: &egui::Painter, xf: &BoardXf) {
        let palette = self.palette();
        let accent = palette.accent;
        let ink = palette.ink;

        for (id, vp) in &self.model3d.live {
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            let srect = xf.rect_w2s(n.rect);
            let bounds = match self.model3d.bounds.get(&vp.cache_key) {
                Some(b) => *b,
                None => continue,
            };
            let aspect = n.rect.w / n.rect.h.max(1.0);
            let cam = vp.cam;

            let to_screen = |p: [f32; 3]| -> Option<Pos2> {
                let (u, v) = model3d::project_model_point(p, aspect, &cam, bounds)?;
                Some(Pos2::new(
                    srect.min.x + u * srect.width(),
                    srect.min.y + v * srect.height(),
                ))
            };

            let draw_segment = |a: [f32; 3], b: [f32; 3], label: &str| {
                let Some(sa) = to_screen(a) else {
                    return;
                };
                let Some(sb) = to_screen(b) else {
                    return;
                };
                let z = xf.z;
                painter.line_segment([sa, sb], EStroke::new(canvas_scale::px(2.0, z), accent));
                painter.circle_filled(sa, canvas_scale::px(4.0, z), accent);
                painter.circle_filled(sb, canvas_scale::px(4.0, z), accent);
                let mid = sa.lerp(sb, 0.5);
                let size = canvas_scale::px(11.0, z);
                if canvas_text::legible(size) {
                    canvas_text::text(
                        painter,
                        mid + Vec2::new(0.0, canvas_scale::px(-10.0, z)),
                        Align2::CENTER_BOTTOM,
                        label,
                        FontId::monospace(size),
                        ink,
                    );
                }
            };

            for m in &vp.measures {
                draw_segment(m.a, m.b, &format!("{:.3}", m.length()));
            }

            if let Some(a) = vp.measure_first {
                let end = vp.measure_preview.unwrap_or(a);
                let len = model3d::DistanceMeasurement { a, b: end }.length();
                let label = if vp.measure_preview.is_some() {
                    format!("{:.3}", len)
                } else {
                    String::new()
                };
                draw_segment(a, end, &label);
            }
        }
    }
}
