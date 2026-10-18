use eframe::egui::{Color32, Painter, Rect, Stroke, StrokeKind};

pub(super) fn paint(painter: &Painter, rect: Rect, active: bool, color: Color32) {
    if active {
        painter.rect_stroke(
            rect.expand(2.0),
            2.0,
            Stroke::new(2.0_f32, color),
            StrokeKind::Outside,
        );
    }
}
