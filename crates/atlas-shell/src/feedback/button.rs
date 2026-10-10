//! The suggestion-box button: a bottom-palette plate in the canvas's
//! lower-left corner, right of the readout chevron (see `canvas_corner`).

use super::FeedbackHub;
use crate::dock::paint_squircle;
use crate::dock_plate::{hover_plate_fill, primary_plate_fill};
use crate::icons::{self, Icon};
use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{self, Id, Rect, Sense, Stroke};

/// Paint the button for this frame; `true` when it was clicked (and the
/// report picker opened).
pub fn suggestion_button(
    ctx: &egui::Context,
    palette: &Palette,
    dock_id: &str,
    canvas: Rect,
    hub: &mut FeedbackHub,
) -> bool {
    let t = tokens::current();
    let dark = palette.dark_mode;
    let th = if dark { &t.dock.dark } else { &t.dock.light };
    let hit_rect = crate::canvas_corner::suggestion_button_hit_rect(canvas);
    let mut open = false;
    egui::Area::new(Id::new(("suggestion_box", dock_id)))
        .fixed_pos(hit_rect.left_bottom())
        .pivot(egui::Align2::LEFT_BOTTOM)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(hit_rect.size(), Sense::click());
            let hover_t = ui.ctx().animate_bool_with_time(
                resp.id.with("hover"),
                resp.hovered(),
                t.dock.palette.hover_fade,
            );
            if resp.hovered() {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let base = primary_plate_fill(th, &t.dock.palette, dark);
            paint_squircle(
                ui.painter(),
                rect.shrink(0.5),
                hover_plate_fill(base, th, &t.dock.palette, dark, hover_t),
                Stroke::new(1.0_f32, th.border_color()),
                t.dock.squircle_exponent,
            );
            icons::paint(
                ui.painter(),
                rect.shrink(7.0),
                Icon::Feedback,
                th.text_color(),
            );
            if resp
                .on_hover_text("Suggestion box — report a bug or request a feature")
                .clicked()
            {
                open = true;
            }
        });
    if open {
        hub.open_picker();
    }
    open
}
