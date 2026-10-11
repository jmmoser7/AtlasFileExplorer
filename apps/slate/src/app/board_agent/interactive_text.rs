//! One owner for hovering interactive text on a chat card.

use super::*;

/// Hovering interactive text bolds it. The widget is placed at `at` (left top
/// of the galley) so every card uses the same hit, whatever else is in the train.
pub(super) fn hover_interactive_text(
    ui: &egui::Ui,
    id: Id,
    at: Pos2,
    text: &str,
    zoom: f32,
    designed_px: f32,
    color: Color32,
) -> egui::Response {
    let font = canvas_scale::font(designed_px, zoom);
    let size = label_size(ui.painter(), text, font.clone(), color);
    let rect = Rect::from_min_size(at, size);
    let response = ui.interact(rect, id, Sense::click());
    let hot = response.hovered()
        || ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|p| rect.contains(p));
    paint_label(ui.painter(), at, Align2::LEFT_TOP, text, font, color, hot);
    response
}

/// Distance from the galley's top to the top of a lowercase letter.
pub(super) fn x_height_rise(painter: &egui::Painter, font: &FontId) -> f32 {
    let laid = canvas_text::layout_no_wrap(painter, "x".into(), font.clone(), Color32::WHITE);
    let bounds = laid.galley().mesh_bounds;
    if bounds.height() > 0.0 && bounds.min.y.is_finite() {
        bounds.min.y * laid.scale()
    } else {
        font.size * 0.24
    }
}

/// "Rename conversation" on the tooltip layer, above the card's own text.
pub(super) fn paint_rename_linger(
    ui: &egui::Ui,
    id: impl std::hash::Hash,
    under: Pos2,
    zoom: f32,
    color: Color32,
) {
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        Id::new(("rename-linger", id)),
    ));
    canvas_text::text(
        &painter,
        under + egui::vec2(0.0, canvas_scale::px(1.0, zoom)),
        Align2::LEFT_TOP,
        "Rename conversation",
        canvas_scale::font(15.0, zoom),
        color,
    );
}

fn label_size(painter: &egui::Painter, text: &str, font: FontId, color: Color32) -> egui::Vec2 {
    canvas_text::layout_no_wrap(painter, text.to_owned(), font, color).size()
}

fn paint_label(
    painter: &egui::Painter,
    pos: Pos2,
    anchor: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    hot: bool,
) -> Rect {
    let rect = canvas_text::text(painter, pos, anchor, text, font.clone(), color);
    if hot {
        let shift = font.size * (0.45 / 13.0);
        canvas_text::text(
            painter,
            pos + egui::vec2(shift, 0.0),
            anchor,
            text,
            font,
            color,
        );
    }
    rect
}
