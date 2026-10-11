//! Polygon side-count glyphs: visual size and pointer tolerance.

use super::*;

/// Designed radius of a polygon vertex's + / − (P1.shape.polygon-sides).
pub const SIDES_GLYPH_RADIUS: f32 = 3.0;
/// Keep small glyphs easy to target; legibility also uses this control size.
pub const SIDES_GLYPH_HIT: f32 = 6.0;
/// Designed distance from the vertex to each glyph's center, along the
/// vertex's bisector: + outside, − inside.
pub const SIDES_GLYPH_OFFSET: f32 = 14.0;
/// Designed radius around a vertex that reveals its glyphs.
pub const SIDES_VERTEX_HIT: f32 = 10.0;

pub fn paint_sides_glyph(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    zoom: f32,
    add: bool,
    ink: Color32,
    fill: Color32,
) {
    painter.circle(
        center,
        radius,
        fill,
        EStroke::new(canvas_scale::px(0.5, zoom), ink),
    );
    let arm = radius * 0.55;
    let bar = EStroke::new(canvas_scale::px(0.75, zoom), ink);
    painter.line_segment([center - Vec2::X * arm, center + Vec2::X * arm], bar);
    if add {
        painter.line_segment([center - Vec2::Y * arm, center + Vec2::Y * arm], bar);
    }
}
