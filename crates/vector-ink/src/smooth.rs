//! Laplacian smoothing of polyline points or B-spline control points under
//! a radial brush.

use crate::bspline::CubicBSpline;

/// Step size at full strength and full falloff. A point-to-point zigzag
/// scales by `1 - 2 * step` per pass: at 0.5 it is gone in one pass, while
/// a larger step flips it to the other side (at 1.0, no damping at all).
const MAX_STEP: f64 = 0.5;

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
    let weights: Vec<f64> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if pinned.get(i).copied().unwrap_or(false) {
                0.0
            } else {
                f64::from(radial_weight(dist(*p, center), radius))
            }
        })
        .collect();
    let mut pts: Vec<[f64; 2]> = points
        .iter()
        .map(|p| [f64::from(p[0]), f64::from(p[1])])
        .collect();
    laplacian_step(&mut pts, &weights, f64::from(strength));
    for (p, q) in points.iter_mut().zip(pts) {
        *p = [q[0] as f32, q[1] as f32];
    }
}

/// One Laplacian pass on a B-spline's control polygon under a radial brush.
/// Each control point is weighted by the brush falloff at its Greville point
/// on the curve, so the brush acts where it covers the curve, not where the
/// off-curve control points happen to sit. The end control points (the
/// curve's endpoints) stay pinned.
///
/// The knot intervals under the brush get the same step. Uniform intervals
/// do not move; the zero intervals of a sharp (triple-knot) joint open up,
/// so a cusp under the brush relaxes into a curvature-continuous joint
/// instead of staying pinned to its control point.
pub fn laplacian_smooth_spline(
    spline: &mut CubicBSpline,
    center: [f32; 2],
    radius: f32,
    strength: f32,
) {
    let n = spline.ctrl.len();
    if n < 3 || strength <= 0.0 || radius <= 0.0 {
        return;
    }
    let weight_at = |u: f64| {
        let p = spline.eval(u);
        f64::from(radial_weight(
            dist([p[0] as f32, p[1] as f32], center),
            radius,
        ))
    };
    let weights: Vec<f64> = (0..n).map(|i| weight_at(spline.greville(i))).collect();
    // Interior intervals only: the clamped end knots stay repeated.
    let spans = 3..spline.knots.len() - 4;
    let mut intervals: Vec<[f64; 2]> = spans
        .clone()
        .map(|j| [spline.knots[j + 1] - spline.knots[j], 0.0])
        .collect();
    let interval_weights: Vec<f64> = spans
        .clone()
        .map(|j| weight_at(0.5 * (spline.knots[j] + spline.knots[j + 1])))
        .collect();
    laplacian_step(&mut spline.ctrl, &weights, f64::from(strength));
    if intervals.len() >= 3 {
        laplacian_step(&mut intervals, &interval_weights, f64::from(strength));
        for (j, d) in spans.zip(&intervals) {
            spline.knots[j + 1] = spline.knots[j] + d[0];
        }
        let last = spline.knots.len() - 4;
        let end = spline.knots[last];
        spline.knots[last..].fill(end);
    }
}

/// Interior points move toward the midpoint of their neighbors by
/// `MAX_STEP * strength * weight`; the first and last point never move.
fn laplacian_step(points: &mut [[f64; 2]], weights: &[f64], strength: f64) {
    let n = points.len();
    let mut delta = vec![[0.0_f64; 2]; n];
    for i in 1..n - 1 {
        let w = MAX_STEP * weights[i] * strength;
        if w <= 0.0 {
            continue;
        }
        delta[i] = [
            ((points[i - 1][0] + points[i + 1][0]) * 0.5 - points[i][0]) * w,
            ((points[i - 1][1] + points[i + 1][1]) * 0.5 - points[i][1]) * w,
        ];
    }
    for (p, d) in points.iter_mut().zip(delta).take(n - 1).skip(1) {
        p[0] += d[0];
        p[1] += d[1];
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

    /// Full strength removes the finest wiggle; it must not flip it to the
    /// other side and leave it as rough as before.
    #[test]
    fn full_strength_damps_a_zigzag_instead_of_flipping_it() {
        let mut pts: Vec<[f32; 2]> = (0..21)
            .map(|i| [i as f32 * 4.0, if i % 2 == 0 { 2.0 } else { -2.0 }])
            .collect();
        let mut pin = vec![false; pts.len()];
        pin[0] = true;
        pin[20] = true;
        let before = curvature_variance(&pts);
        laplacian_smooth_pass(&mut pts, &pin, [40.0, 0.0], 1000.0, 1.0);
        let after = curvature_variance(&pts);
        assert!(after < before * 0.5, "before {before} after {after}");
    }

    fn cusp_spline() -> CubicBSpline {
        let mut bez = kurbo::BezPath::new();
        bez.move_to((0.0, 0.0));
        bez.curve_to((50.0, -150.0), (150.0, -150.0), (200.0, 0.0));
        bez.curve_to((250.0, -150.0), (350.0, -150.0), (400.0, 0.0));
        let mut opts = crate::SplineFit::new(0.1);
        opts.max_span = 20.0;
        CubicBSpline::fit_path(&bez, &opts, 1.0).unwrap()
    }

    #[test]
    fn spline_smoothing_pins_ends_and_leaves_far_control_points() {
        let mut s = cusp_spline();
        let before = s.clone();
        for _ in 0..10 {
            laplacian_smooth_spline(&mut s, [200.0, 0.0], 40.0, 1.0);
        }
        assert_eq!(s.ctrl.first(), before.ctrl.first());
        assert_eq!(s.ctrl.last(), before.ctrl.last());
        assert_eq!(s.knots.len(), before.knots.len());
        assert_eq!(s.knots[..4], before.knots[..4], "clamped start stays");
        for j in 3..before.knots.len() - 4 {
            let mid = before.eval(0.5 * (before.knots[j] + before.knots[j + 1]));
            if dist([mid[0] as f32, mid[1] as f32], [200.0, 0.0]) >= 40.0 {
                let d0 = before.knots[j + 1] - before.knots[j];
                let d1 = s.knots[j + 1] - s.knots[j];
                assert!(
                    (d0 - d1).abs() < 1e-12,
                    "knot interval {j} outside the brush changed"
                );
            }
        }
        let mut far = 0;
        for (i, (a, b)) in s.ctrl.iter().zip(&before.ctrl).enumerate() {
            let g = before.eval(before.greville(i));
            if dist([g[0] as f32, g[1] as f32], [200.0, 0.0]) >= 40.0 {
                assert_eq!(a, b, "control point {i} outside the brush moved");
                far += 1;
            }
        }
        assert!(far > 10);
        assert_ne!(s.ctrl, before.ctrl);
    }

    #[test]
    fn spline_smoothing_rounds_a_cusp_under_the_brush() {
        let mut s = cusp_spline();
        let turn = |s: &CubicBSpline| {
            let pts: Vec<[f32; 2]> = (0..=400)
                .map(|i| {
                    let p = s.eval(i as f64 / 400.0 * s.domain().1);
                    [p[0] as f32, p[1] as f32]
                })
                .collect();
            pts.windows(3).map(curvature_variance).fold(0.0, f32::max)
        };
        let before = turn(&s);
        for _ in 0..10 {
            laplacian_smooth_spline(&mut s, [200.0, 0.0], 40.0, 1.0);
        }
        let after = turn(&s);
        assert!(after < before / 3.0, "cusp turn {before} -> {after}");
    }
}
