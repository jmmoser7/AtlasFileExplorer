//! Bottom toast stack — shared by File Atlas and Slate.

use crate::canvas_corner::bottom_tool_palette_rect;
use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, RichText, Stroke, Vec2,
};

fn measure_body(ctx: &egui::Context, text: &str, wrap_width: f32, t: &tokens::ToastTokens) -> Vec2 {
    ctx.fonts(|fonts| {
        let job = egui::text::LayoutJob::simple(
            text.to_owned(),
            FontId::proportional(t.text_size),
            Color32::WHITE,
            wrap_width,
        );
        let galley = fonts.layout_job(job);
        galley.size() + Vec2::new(t.pad_x * 2.0, t.pad_y * 2.0)
    })
}

fn wrap_width(screen_width: f32, t: &tokens::ToastTokens) -> f32 {
    (t.max_width_px.min(screen_width * t.max_width_fraction)).max(t.min_width_px)
}

/// Bottom edge (y) of the toast stack — above the tool palette.
pub fn toast_stack_bottom_y(canvas: Rect) -> f32 {
    let t = tokens::current().toast.clone();
    bottom_tool_palette_rect(canvas).top() - t.above_palette_gap
}

/// Union rect of all toast bodies if painted on `canvas` with `screen_width`.
pub fn toast_stack_rect(
    ctx: &egui::Context,
    canvas: Rect,
    screen_width: f32,
    messages: &[&str],
) -> Option<Rect> {
    if messages.is_empty() {
        return None;
    }
    let t = tokens::current().toast.clone();
    let wrap = wrap_width(screen_width, &t);
    let mut bottom = toast_stack_bottom_y(canvas);
    let mut union: Option<Rect> = None;
    for msg in messages {
        let size = measure_body(ctx, msg, wrap, &t);
        let rect = Rect::from_min_max(
            Pos2::new(canvas.center().x - size.x * 0.5, bottom - size.y),
            Pos2::new(canvas.center().x + size.x * 0.5, bottom),
        );
        union = Some(union.map_or(rect, |u| u.union(rect)));
        bottom = rect.top() - t.stack_gap;
    }
    union
}

/// Paint toasts centered above the bottom tool palette. Returns the painted union rect.
pub fn paint_stack(
    ctx: &egui::Context,
    palette: &Palette,
    canvas: Rect,
    messages: &[&str],
) -> Option<Rect> {
    if messages.is_empty() {
        return None;
    }
    let t = tokens::current().toast.clone();
    let screen_w = ctx.screen_rect().width();
    let wrap = wrap_width(screen_w, &t);
    let mut bottom = toast_stack_bottom_y(canvas);
    let mut union: Option<Rect> = None;
    for (i, msg) in messages.iter().enumerate() {
        let pos = Pos2::new(canvas.center().x, bottom);
        let rect = egui::Area::new(Id::new(("atlas_toast", i)))
            .fixed_pos(pos)
            .pivot(Align2::CENTER_BOTTOM)
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_max_width(wrap);
                egui::Frame::popup(ui.style())
                    .fill(palette.card)
                    .stroke(Stroke::new(
                        1.0_f32,
                        palette.border_strong.gamma_multiply(0.85),
                    ))
                    .inner_margin(egui::Margin::symmetric(
                        t.pad_x.round() as i8,
                        t.pad_y.round() as i8,
                    ))
                    .corner_radius(CornerRadius::same(t.corner_radius.round() as u8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(*msg).color(palette.ink).size(t.text_size));
                    })
                    .response
                    .rect
            })
            .inner;
        union = Some(union.map_or(rect, |u| u.union(rect)));
        bottom = rect.top() - t.stack_gap;
    }
    union
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_corner::bottom_tool_palette_rect;

    fn canvas() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1440.0, 820.0))
    }

    #[test]
    fn toast_stack_sits_above_the_tool_palette() {
        let ctx = egui::Context::default();
        let canvas = canvas();
        let msg = "Save the workbook first — assets travel beside the .slate file.";
        let toast = toast_stack_rect(&ctx, canvas, 1440.0, &[msg]).expect("toast");
        let palette = bottom_tool_palette_rect(canvas);
        assert!(!toast.intersects(palette));
        assert!(toast.bottom() <= palette.top() - tokens::current().toast.above_palette_gap + 0.5);
    }

    #[test]
    fn long_toast_wraps_below_max_width() {
        let ctx = egui::Context::default();
        let t = tokens::current().toast.clone();
        let wrap = (t.max_width_px.min(1440.0 * t.max_width_fraction)).max(t.min_width_px);
        let msg = "word ".repeat(80);
        let size = measure_body(&ctx, &msg, wrap, &t);
        assert!(size.x <= wrap + t.pad_x * 2.0 + 1.0);
        assert!(size.y > t.text_size * 2.0, "multi-line wrap");
    }
}
