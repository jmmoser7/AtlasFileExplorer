//! Flat board backgrounds on the software GL renderer.

use eframe::egui::{self, Color32, PaintCallback, Rect};
use eframe::glow::{self, HasContext};
use std::sync::Arc;

pub(in crate::app) fn supported(gl: &glow::Context) -> bool {
    gl.version().major >= 3 && {
        // Once, while the native creation callback has the GL context current.
        let renderer = unsafe { gl.get_parameter_string(glow::RENDERER) };
        renderer.to_ascii_lowercase().starts_with("llvmpipe")
    }
}

pub(in crate::app) fn clear(rect: Rect, color: Color32) -> PaintCallback {
    let rgba = color.to_normalized_gamma_f32();
    PaintCallback {
        rect,
        callback: Arc::new(eframe::egui_glow::CallbackFn::new(move |_, painter| {
            // egui_glow sets the current primitive's physical-pixel scissor
            // immediately before calling this. The board painter clips it to
            // the canvas. ClearBufferfv leaves the GL clear color, framebuffer
            // bindings, viewport, blending and scissor state untouched.
            unsafe { painter.gl().clear_buffer_f32_slice(glow::COLOR, 0, &rgba) };
        })),
    }
}

pub(in crate::app) fn paint(ui: &egui::Ui, bg: Color32) {
    let rect = ui.available_rect_before_wrap();
    ui.painter_at(rect).add(clear(rect, bg));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::Harness;

    #[test]
    fn native_software_clear_only_replaces_the_ordinary_board_background() {
        let mut h = Harness::new("board_background");
        h.app.leave_home();
        h.app.ensure_work_tab();
        let out = h.frame_output(|_| {});
        let board = h.app.canvas_rect;
        let bg = h.app.palette().bg;
        let callbacks = |out: &egui::FullOutput| {
            out.shapes.iter().filter(|s| {
                matches!(&s.shape, egui::Shape::Callback(c) if c.rect == board && s.clip_rect == board)
            }).count()
        };
        assert_eq!(callbacks(&out), 0);
        assert_eq!(
            out.shapes
                .iter()
                .filter(|s| {
                    matches!(&s.shape, egui::Shape::Rect(r) if r.rect == board && r.fill == bg)
                })
                .count(),
            1
        );

        h.app.software_board_clear = true;
        assert_eq!(callbacks(&h.frame_output(|_| {})), 1);
        h.app.go_home();
        assert_eq!(callbacks(&h.frame_output(|_| {})), 0);
    }
}
