//! Selection decoration fade, shared by the apps.
use super::*;

/// Fade only selection decorations, leaving authored paint and editor chrome intact.
pub fn selection_painter(painter: &egui::Painter, id: Id, editor_open: bool) -> egui::Painter {
    let fade = painter
        .ctx()
        .animate_bool_with_time(id, editor_open, SELECTION_FADE_SECONDS);
    let mut selection = painter.clone();
    selection.multiply_opacity(1.0 - fade);
    selection
}
