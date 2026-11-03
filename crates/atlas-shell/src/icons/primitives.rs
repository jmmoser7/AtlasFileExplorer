//! Small shared glyph primitives; hosts own hit testing, layout and hover state.
use eframe::egui;

/// Bare plus shared by add controls; callers own their surrounding surface.
pub fn paint_plus(painter: &egui::Painter, center: egui::Pos2, radius: f32, stroke: egui::Stroke) {
    for axis in [egui::Vec2::X, egui::Vec2::Y] {
        painter.line_segment([center - axis * radius, center + axis * radius], stroke);
    }
}

/// The existing native More mark, with caller-owned spacing and hover sizing.
pub fn paint_more(
    painter: &egui::Painter,
    center: egui::Pos2,
    pitch: f32,
    radius: f32,
    color: egui::Color32,
) {
    for offset in [-pitch, 0.0, pitch] {
        painter.circle_filled(center + egui::vec2(offset, 0.0), radius, color);
    }
}
