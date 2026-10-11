//! One owner for hovering interactive text on a chat card.

use super::*;

/// Hovering interactive text in Slate bolds the text; do not invent a new
/// box, underline, or color wash.
pub(super) fn hover_interactive_text(
    ui: &mut egui::Ui,
    id: Id,
    text: &str,
    zoom: f32,
    designed_px: f32,
    color: Color32,
) -> egui::Response {
    let font = canvas_scale::font(designed_px, zoom);
    let size = label_size(ui.painter(), text, font.clone(), color);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let response = ui.interact(rect, id, Sense::click());
    let hot = response.hovered()
        || ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|p| rect.contains(p));
    paint_label(
        ui.painter(),
        rect.left_top(),
        Align2::LEFT_TOP,
        text,
        font,
        color,
        hot,
    );
    response
}

/// "Rename conversation" under the title: the title's type family and resting
/// weight, slightly larger, in world units so it scales with the board.
pub(super) fn paint_rename_linger(painter: &egui::Painter, under: Pos2, zoom: f32, color: Color32) {
    canvas_text::text(
        painter,
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
        // A second copy, a fraction of the type size over, is the bold.
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
