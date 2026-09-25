//! Laplacian smoothing of polyline control points under a radial brush.

/// Smoothstep falloff: 1 at center, 0 at `radius`.
pub fn radial_weight(distance: f32, radius: f32) -> f32 {
    if radius <= 0.0 || distance >= radius {
        return 0.0;
    }
    let t = (1.0 - distance / radius).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

/// One incremental Laplacian pass. Interior points move toward the average of
/// neighbors, scaled by `strength` and brush falloff. Pinned indices (endpoints
/// and optional corners) are unchanged.
pub fn laplacian_smooth_pass(
    points: &mut [[f32; 2]],
    pinned: &[bool],
    center: [f32; 2],
    radius: f32,
    strength: f32,
) {
    if points.len() < 3 || strength <= 0.0 || radius <= 0.0 {
        return;
    }
    let mut delta = vec![[0.0_f32; 2]; points.len()];
    for i in 1..points.len() - 1 {
        if pinned.get(i).copied().unwrap_or(false) {
            continue;
        }
        let w = radial_weight(dist(points[i], center), radius);
        if w <= 0.0 {
            continue;
        }
        let lap = [
            (points[i - 1][0] + points[i + 1][0]) * 0.5 - points[i][0],
            (points[i - 1][1] + points[i + 1][1]) * 0.5 - points[i][1],
        ];
        delta[i][0] = lap[0] * strength * w;
        delta[i][1] = lap[1] * strength * w;
    }
    for i in 1..points.len() - 1 {
        points[i][0] += delta[i][0];
        points[i][1] += delta[i][1];
    }
}

/// Total absolute curvature proxy (sum of turning angles) — lower after smoothing.
pub fn curvature_variance(points: &[[f32; 2]]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0_f32;
    for i in 1..points.len() - 1 {
        let a = [
            points[i][0] - points[i - 1][0],
            points[i][1] - points[i - 1][1],
        ];
        let b = [
            points[i + 1][0] - points[i][0],
            points[i + 1][1] - points[i][1],
        ];
        let la = (a[0] * a[0] + a[1] * a[1]).sqrt();
        let lb = (b[0] * b[0] + b[1] * b[1]).sqrt();
        if la < 1e-6 || lb < 1e-6 {
            continue;
        }
        let dot = (a[0] * b[0] + a[1] * b[1]) / (la * lb);
        sum += dot.clamp(-1.0, 1.0).acos();
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_strength_is_noop() {
        let mut pts = vec![[0.0, 0.0], [5.0, 5.0], [10.0, 0.0]];
        let before = pts.clone();
        laplacian_smooth_pass(&mut pts, &[true, false, true], [5.0, 5.0], 10.0, 0.0);
        assert_eq!(pts, before);
    }

    #[test]
    fn endpoints_stay_pinned() {
        let mut pts = vec![[0.0, 0.0], [5.0, 8.0], [10.0, 0.0]];
        laplacian_smooth_pass(&mut pts, &[true, false, true], [5.0, 4.0], 20.0, 1.0);
        assert!((pts[0][0]).abs() < 1e-5);
        assert!((pts.last().unwrap()[0] - 10.0).abs() < 1e-5);
        assert!(pts[1][1] < 8.0);
    }

    #[test]
    fn points_outside_brush_untouched() {
        let mut pts = vec![[0.0, 0.0], [100.0, 0.0], [200.0, 0.0]];
        laplacian_smooth_pass(&mut pts, &[true, false, true], [100.0, 50.0], 10.0, 1.0);
        assert!((pts[0][1]).abs() < 1e-5);
        assert!((pts[2][1]).abs() < 1e-5);
        assert!((pts[1][1]).abs() < 1e-5);
    }

    #[test]
    fn smoothing_reduces_curvature_variance() {
        let mut pts: Vec<[f32; 2]> = (0..40)
            .map(|i| {
                let x = i as f32;
                let y = if i % 2 == 0 { 2.0 } else { -2.0 };
                [x, y]
            })
            .collect();
        let before = curvature_variance(&pts);
        let mut pin = vec![false; pts.len()];
        pin[0] = true;
        if let Some(last) = pin.last_mut() {
            *last = true;
        }
        for _ in 0..8 {
            laplacian_smooth_pass(&mut pts, &pin, [20.0, 0.0], 30.0, 0.45);
        }
        let after = curvature_variance(&pts);
        assert!(after < before * 0.85, "before {before} after {after}");
    }
}
