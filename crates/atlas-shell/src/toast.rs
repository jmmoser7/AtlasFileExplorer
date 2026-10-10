//! Bottom toast stack — shared by File Atlas and Slate.
//!
//! Window chrome (P0.9 exception): screen-sized, centered on the canvas, and
//! stacked upward from just above the bottom tool palette.

use crate::canvas_corner::bottom_tool_palette_rect;
use crate::theme::Palette;
use crate::tokens::{self, ToastTokens};
use eframe::egui::{self, Color32, CornerRadius, FontId, Galley, Id, Pos2, Rect, Stroke, Vec2};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Set by the UI tuner's "Lock sample toasts open".
pub(crate) static TUNER_PREVIEW: AtomicBool = AtomicBool::new(false);

const TUNER_SAMPLES: [&str; 2] = [
    "Copied 12 files to Destination — the originals stay where they were, and the \
     journal can undo this.",
    "Saved.",
];

/// Width the toast body text wraps at on a window `screen_width` wide.
pub fn wrap_width(screen_width: f32, t: &ToastTokens) -> f32 {
    t.max_width_px
        .min(screen_width * t.max_width_fraction)
        .max(t.min_width_px)
}

/// Bottom edge (y) of the toast stack: `above_palette_gap` above the bottom
/// palette's partition line, which sits `partition_gap` above its icons.
pub fn toast_stack_bottom_y(canvas: Rect) -> f32 {
    let t = tokens::current();
    bottom_tool_palette_rect(canvas).top() - t.dock.partition_gap - t.toast.above_palette_gap
}

/// Body rect and laid-out text for each toast, newest at the bottom.
fn layout(
    ctx: &egui::Context,
    canvas: Rect,
    screen_width: f32,
    messages: &[&str],
    ink: Color32,
) -> Vec<(Rect, Arc<Galley>)> {
    let t = tokens::current().toast.clone();
    let wrap = wrap_width(screen_width, &t);
    let pad = Vec2::new(t.pad_x, t.pad_y);
    let mut bottom = toast_stack_bottom_y(canvas);
    let mut out = Vec::with_capacity(messages.len());
    for msg in messages.iter().rev() {
        let galley = ctx.fonts(|fonts| {
            fonts.layout(
                (*msg).to_owned(),
                FontId::proportional(t.text_size),
                ink,
                wrap,
            )
        });
        let size = galley.size() + pad * 2.0;
        let rect = Rect::from_min_size(
            Pos2::new(canvas.center().x - size.x * 0.5, bottom - size.y),
            size,
        );
        bottom = rect.top() - t.stack_gap;
        out.push((rect, galley));
    }
    out
}

/// Union rect of all toast bodies if painted on `canvas` with `screen_width`.
pub fn toast_stack_rect(
    ctx: &egui::Context,
    canvas: Rect,
    screen_width: f32,
    messages: &[&str],
) -> Option<Rect> {
    layout(ctx, canvas, screen_width, messages, Color32::WHITE)
        .into_iter()
        .map(|(rect, _)| rect)
        .reduce(|a, b| a.union(b))
}

/// Paint toasts centered above the bottom tool palette, newest lowest.
/// Call every frame, even with no messages, so the tuner preview can show.
/// Returns the painted union rect.
pub fn paint_stack(
    ctx: &egui::Context,
    palette: &Palette,
    canvas: Rect,
    messages: &[&str],
) -> Option<Rect> {
    let messages = if messages.is_empty() && TUNER_PREVIEW.load(Ordering::Relaxed) {
        &TUNER_SAMPLES[..]
    } else {
        messages
    };
    let t = tokens::current().toast.clone();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("atlas_toasts"),
    ));
    let pad = Vec2::new(t.pad_x, t.pad_y);
    let radius = CornerRadius::same(t.corner_radius.round() as u8);
    let mut union: Option<Rect> = None;
    let screen_w = ctx.screen_rect().width();
    for (rect, galley) in layout(ctx, canvas, screen_w, messages, palette.ink) {
        painter.rect(
            rect,
            radius,
            palette.window,
            Stroke::new(1.0_f32, palette.border_strong),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + pad, galley, palette.ink);
        union = Some(union.map_or(rect, |u| u.union(rect)));
    }
    union
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1440.0, 820.0))
    }

    fn ctx() -> egui::Context {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |_| {});
        ctx
    }

    #[test]
    fn toast_stack_sits_above_the_tool_palette() {
        let ctx = ctx();
        let canvas = canvas();
        let msg = "Save the workbook first — assets travel beside the .slate file.";
        let toast = toast_stack_rect(&ctx, canvas, 1440.0, &[msg, msg]).expect("toast");
        let palette = bottom_tool_palette_rect(canvas);
        let t = tokens::current();
        let partition_y = palette.top() - t.dock.partition_gap;
        assert!(toast.bottom() <= partition_y - t.toast.above_palette_gap + 0.01);
    }

    #[test]
    fn long_toast_wraps_at_the_max_width_never_one_word_per_line() {
        let ctx = ctx();
        let t = tokens::current().toast.clone();
        let wrap = wrap_width(1440.0, &t);
        let msg = "word ".repeat(80);
        let toast = toast_stack_rect(&ctx, canvas(), 1440.0, &[&msg]).expect("toast");
        let text_w = toast.width() - t.pad_x * 2.0;
        assert!(text_w <= wrap + 1.0, "{text_w} wraps within {wrap}");
        assert!(
            text_w >= wrap * 0.8,
            "{text_w} fills the line before wrapping"
        );
        assert!(toast.height() > t.text_size * 2.0, "multi-line wrap");
    }

    #[test]
    fn short_toast_hugs_its_text() {
        let ctx = ctx();
        let t = tokens::current().toast.clone();
        let toast = toast_stack_rect(&ctx, canvas(), 1440.0, &["Saved."]).expect("toast");
        assert!(toast.width() < wrap_width(1440.0, &t) * 0.5);
        assert!(toast.height() < t.text_size * 2.0 + t.pad_y * 2.0);
    }

    #[test]
    fn narrow_window_still_wraps_at_the_minimum_width() {
        let t = tokens::current().toast.clone();
        assert!(wrap_width(320.0, &t) >= t.min_width_px);
    }
}
