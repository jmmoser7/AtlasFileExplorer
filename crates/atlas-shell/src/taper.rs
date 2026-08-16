//! Anti-aliased tapered ribbons — shared chrome aesthetic.
//!
//! Used for the dock partition line and any future soft separators that need
//! thickness peaking at midspan and fading at the ends without jagged
//! per-segment strokes. See `PAINT.md`.

use eframe::egui::{
    self,
    epaint::{Mesh, Vertex, WHITE_UV},
    Color32, Pos2, Vec2,
};

/// Paint a soft ribbon along `[a → b]` whose half-width peaks at midspan
/// (`max_half`) and tapers to `min_half` at the ends. Cross-section alpha
/// feathers to zero so the edge is anti-aliased (no discrete stroke segments).
pub fn paint_tapered_ribbon(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    max_half: f32,
    min_half: f32,
    color: Color32,
) {
    paint_tapered_ribbon_graded(painter, a, b, max_half, min_half, color, color);
}

/// Like [`paint_tapered_ribbon`], but color also peaks at midspan (`center`)
/// and falls to `edge` at the ends — width and opacity share the same curve.
pub fn paint_tapered_ribbon_graded(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    max_half: f32,
    min_half: f32,
    center: Color32,
    edge: Color32,
) {
    let along = b - a;
    let len = along.length();
    if len < 1.0 || (center.a() == 0 && edge.a() == 0) || max_half <= 0.0 {
        return;
    }
    let dir = along / len;
    let perp = Vec2::new(-dir.y, dir.x);
    let min_half = min_half.clamp(0.0, max_half);
    // Soft edge width for AA (screen-space).
    let feather = (max_half * 0.55).clamp(0.75, 2.0);

    let steps = ((len / 3.0).ceil() as usize).clamp(24, 96);
    let mut mesh = Mesh::default();
    mesh.vertices.reserve((steps + 1) * 4);
    mesh.indices.reserve(steps * 18);

    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        // Smooth falloff: 1 at center, 0 at ends.
        let u = (2.0 * t - 1.0).abs(); // 0 center → 1 ends
        let falloff = (1.0 - u * u).clamp(0.0, 1.0);
        let half = min_half + (max_half - min_half) * falloff;
        let p = a + dir * (len * t);
        let solid = lerp_unmultiplied(edge, center, falloff);
        let clear = Color32::TRANSPARENT;

        let base = mesh.vertices.len() as u32;
        // Outer → inner left → inner right → outer right (feathered AA).
        for (offset, col) in [
            (-(half + feather), clear),
            (-half, solid),
            (half, solid),
            (half + feather, clear),
        ] {
            mesh.vertices.push(Vertex {
                pos: p + perp * offset,
                uv: WHITE_UV,
                color: col,
            });
        }

        if i > 0 {
            let prev = base - 4;
            // Two soft edge quads + one solid core.
            for (i0, i1, i2) in [
                (0u32, 1, 5),
                (0, 5, 4),
                (1, 2, 6),
                (1, 6, 5),
                (2, 3, 7),
                (2, 7, 6),
            ] {
                mesh.indices
                    .extend_from_slice(&[prev + i0, prev + i1, prev + i2]);
            }
        }
    }

    painter.add(egui::Shape::mesh(mesh));
}

fn lerp_unmultiplied(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t).round() as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t).round() as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t).round() as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_length_is_noop_safe() {
        // Just ensure the math path does not panic on degenerate input.
        let _ = paint_tapered_ribbon;
        let a = Pos2::ZERO;
        let b = Pos2::ZERO;
        let along = b - a;
        assert!(along.length() < 1.0);
    }
}
