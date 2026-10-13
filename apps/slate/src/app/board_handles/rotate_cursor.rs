//! Pointer-only rotation feedback in the Windows white and black cursor scheme.
use super::*;

#[cfg(test)]
#[path = "pointer_tests.rs"]
mod tests;

/// egui has no native rotation cursor. The pointer painter replaces it.
pub fn cursor_for_rotate() -> CursorIcon {
    CursorIcon::None
}

/// A two-headed quarter arc cupping a corner, at OS cursor size.
pub fn paint_rotate_cursor(painter: &egui::Painter, pos: Pos2) {
    const RADIUS: f32 = 7.0;
    const SEGMENTS: usize = 16;
    const HEAD: f32 = 4.5;
    let points: Vec<_> = (0..=SEGMENTS)
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::FRAC_PI_2 * i as f32 / SEGMENTS as f32;
            pos + Vec2::angled(angle) * RADIUS
        })
        .collect();
    painter.add(egui::Shape::line(
        points.clone(),
        EStroke::new(4.5_f32, Color32::BLACK),
    ));
    painter.add(egui::Shape::line(
        points.clone(),
        EStroke::new(2.0_f32, Color32::WHITE),
    ));
    for (end, tangent, outward) in [
        (points[0], Vec2::new(-1.0, 0.0), Vec2::new(0.0, -1.0)),
        (
            *points.last().unwrap(),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
        ),
    ] {
        painter.add(egui::Shape::convex_polygon(
            vec![
                end + tangent * HEAD,
                end + outward * HEAD,
                end - outward * HEAD,
            ],
            Color32::WHITE,
            EStroke::new(1.0_f32, Color32::BLACK),
        ));
    }
}
