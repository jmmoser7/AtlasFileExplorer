//! Regular polygon geometry and side-count edits.

use super::WorldRect;

pub fn default_regular_sides() -> u8 {
    6
}

pub const REGULAR_POLYGON_SIDES_MIN: u8 = 3;
pub const REGULAR_POLYGON_SIDES_MAX: u8 = 12;

pub fn clamp_regular_sides(sides: u8) -> u8 {
    sides.clamp(REGULAR_POLYGON_SIDES_MIN, REGULAR_POLYGON_SIDES_MAX)
}

/// Vertices of a regular n-gon inscribed in `rect`. Vertex `i` sits at
/// parameter angle `-90° + phase_deg + i·360°/n` on the inscribed ellipse,
/// so phase 0 puts the first vertex at top center.
pub fn regular_polygon_vertices(rect: WorldRect, sides: u8, phase_deg: f32) -> Vec<[f32; 2]> {
    (0..clamp_regular_sides(sides) as usize)
        .map(|i| regular_polygon_vertex(rect, sides, phase_deg, i).unwrap())
        .collect()
}

/// One vertex in the same order as [`regular_polygon_vertices`], without
/// allocating a complete outline for a hovered vertex's rotation hint.
pub fn regular_polygon_vertex(
    rect: WorldRect,
    sides: u8,
    phase_deg: f32,
    i: usize,
) -> Option<[f32; 2]> {
    if i >= clamp_regular_sides(sides) as usize {
        return None;
    }
    let n = clamp_regular_sides(sides) as f32;
    let (cx, cy) = rect.center();
    let rx = rect.w * 0.5;
    let ry = rect.h * 0.5;
    let start = -std::f32::consts::FRAC_PI_2 + phase_deg.to_radians();
    let a = start + i as f32 * (std::f32::consts::TAU / n);
    Some([cx + rx * a.cos(), cy + ry * a.sin()])
}

/// Side count and phase after adding (`add`) or removing one side at
/// vertex `k` (P1.shape.polygon-sides). Removing keeps a vertex exactly at
/// vertex `k`: the new vertex 0 takes its angle. Adding keeps vertex `k`
/// at that same angle and re-spaces the others around it. `None` at the
/// 3 / 12 limits.
pub fn regular_polygon_resided(
    sides: u8,
    phase_deg: f32,
    k: usize,
    add: bool,
) -> Option<(u8, f32)> {
    let n = clamp_regular_sides(sides);
    let next = if add { n + 1 } else { n - 1 };
    if next != clamp_regular_sides(next) {
        return None;
    }
    let k = k % n as usize;
    let at = phase_deg + k as f32 * 360.0 / n as f32;
    let phase = if add {
        at - k as f32 * 360.0 / next as f32
    } else {
        at
    };
    let phase = phase.rem_euclid(360.0);
    Some((next, if phase >= 360.0 { 0.0 } else { phase }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_vertex_queries_keep_batch_order_clamping_and_phase() {
        let rect = WorldRect::new(-20.0, 10.0, 240.0, 180.0);
        for sides in [0, 3, 6, 12, 255] {
            for phase in [0.0, 37.0, 270.0] {
                let vertices = regular_polygon_vertices(rect, sides, phase);
                for (i, &vertex) in vertices.iter().enumerate() {
                    assert_eq!(regular_polygon_vertex(rect, sides, phase, i), Some(vertex));
                }
                assert_eq!(
                    regular_polygon_vertex(rect, sides, phase, vertices.len()),
                    None
                );
            }
        }
        let top = regular_polygon_vertex(rect, 4, 0.0, 0).unwrap();
        assert!((top[0] - 100.0).abs() < 1e-4 && (top[1] - 10.0).abs() < 1e-4);
    }
}
