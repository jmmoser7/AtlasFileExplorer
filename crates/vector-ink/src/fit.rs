//! Freehand stroke fitting: decimation, corner detection, and one cubic
//! B-spline per corner-free run (`bspline`), stored as its exact cubic
//! Bézier segments.

use kurbo::{BezPath, Point};

use crate::bspline::{CubicBSpline, SplineFit};

/// Fit a freehand polyline (legacy entry: spacing derived from error tolerance).
pub fn fit_polyline(points: &[[f32; 2]], error_tolerance: f32) -> BezPath {
    let spacing = (error_tolerance * 0.875).max(1e-6);
    fit_polyline_spaced(points, error_tolerance, spacing)
}

/// Fit with explicit screen-space tolerances already converted to world units.
///
/// Samples closer than `min_spacing` are dropped; each run between detected
/// cusps becomes one curvature-continuous cubic B-spline within
/// `error_tolerance` of its samples; runs meet at their cusps.
pub fn fit_polyline_spaced(points: &[[f32; 2]], error_tolerance: f32, min_spacing: f32) -> BezPath {
    let mut path = BezPath::new();
    if points.len() < 2 {
        return path;
    }
    if !error_tolerance.is_finite() || error_tolerance <= 0.0 {
        return path;
    }
    let min_spacing = min_spacing.max(1e-6);

    let mut pts = decimate_by_distance(points, min_spacing);
    finalize_endpoint(&mut pts, min_spacing);
    if pts.len() < 2 {
        return path;
    }
    if pts.len() == 2 {
        path.move_to(to_k(pts[0]));
        path.line_to(to_k(pts[1]));
        return path;
    }

    let smoothed = light_smooth(&pts);
    let corners = corner_indices(
        &smoothed,
        6.0_f32.max(min_spacing * 3.0),
        65.0_f32.to_radians(),
    );
    let opts = SplineFit::new(error_tolerance);
    let mut spline: Option<CubicBSpline> = None;
    for w in corners.windows(2) {
        let Some(piece) = CubicBSpline::fit(&pts[w[0]..=w[1]], &opts) else {
            continue;
        };
        match spline.as_mut() {
            Some(s) => s.join(&piece),
            None => spline = Some(piece),
        }
    }
    spline.map(|s| s.to_bezpath()).unwrap_or(path)
}

fn to_k(p: [f32; 2]) -> Point {
    Point::new(p[0] as f64, p[1] as f64)
}

fn decimate_by_distance(points: &[[f32; 2]], min_spacing: f32) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(points.len());
    out.push(points[0]);
    for &p in &points[1..] {
        let last = *out.last().unwrap();
        if dist(last, p) >= min_spacing {
            out.push(p);
        }
    }
    if out.len() == 1 {
        out.push(*points.last().unwrap());
    }
    out
}

fn finalize_endpoint(pts: &mut Vec<[f32; 2]>, min_spacing: f32) {
    if pts.len() < 2 {
        return;
    }
    let end = pts.pop().expect("len >= 2");
    if let Some(last) = pts.last_mut() {
        if dist(*last, end) < min_spacing {
            *last = end;
        } else {
            pts.push(end);
        }
    }
}

/// Two [1 2 1] passes that skip corners. Used only to find cusps: the
/// spline fit reads the unsmoothed samples and does its own fairing.
fn light_smooth(points: &[[f32; 2]]) -> Vec<[f32; 2]> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut cur = points.to_vec();
    for _ in 0..2 {
        let mut next = cur.clone();
        for i in 1..cur.len() - 1 {
            if is_corner_at(&cur, i, 55.0_f32.to_radians()) {
                continue;
            }
            next[i] = [
                (cur[i - 1][0] + 2.0 * cur[i][0] + cur[i + 1][0]) / 4.0,
                (cur[i - 1][1] + 2.0 * cur[i][1] + cur[i + 1][1]) / 4.0,
            ];
        }
        cur = next;
    }
    cur
}

/// Run boundaries: the first and last sample plus one cusp per cluster of
/// consecutive samples that turn more than `threshold` over `window` of
/// arc length. Only the sharpest sample of a cluster splits the stroke, so
/// a corner never leaves a stub run beside it.
fn corner_indices(points: &[[f32; 2]], window: f32, threshold: f32) -> Vec<usize> {
    let n = points.len();
    let mut out = vec![0];
    let mut i = 1;
    while i + 1 < n {
        if corner_angle(points, i, window) < threshold {
            i += 1;
            continue;
        }
        let mut best = (i, corner_angle(points, i, window));
        let mut j = i + 1;
        while j + 1 < n {
            let a = corner_angle(points, j, window);
            if a < threshold {
                break;
            }
            if a > best.1 {
                best = (j, a);
            }
            j += 1;
        }
        out.push(best.0);
        i = j;
    }
    out.push(n - 1);
    out
}

fn corner_angle(points: &[[f32; 2]], idx: usize, window: f32) -> f32 {
    let p = points[idx];
    let mut back = None;
    let mut fwd = None;
    let mut acc = 0.0_f32;
    for i in (0..idx).rev() {
        acc += dist(points[i + 1], points[i]);
        if acc >= window {
            back = Some(sub(p, points[i]));
            break;
        }
    }
    acc = 0.0;
    for i in idx..points.len() - 1 {
        acc += dist(points[i], points[i + 1]);
        if acc >= window {
            fwd = Some(sub(points[i + 1], p));
            break;
        }
    }
    match (back, fwd) {
        (Some(a), Some(b))
            if a[0] * a[0] + a[1] * a[1] > 1e-8 && b[0] * b[0] + b[1] * b[1] > 1e-8 =>
        {
            let la = (a[0] * a[0] + a[1] * a[1]).sqrt();
            let lb = (b[0] * b[0] + b[1] * b[1]).sqrt();
            let dot = (a[0] * b[0] + a[1] * b[1]) / (la * lb);
            dot.clamp(-1.0, 1.0).acos()
        }
        _ => 0.0,
    }
}

fn is_corner_at(points: &[[f32; 2]], idx: usize, threshold: f32) -> bool {
    corner_angle(points, idx, 6.0) >= threshold
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;
    use kurbo::PathEl;

    #[test]
    fn collinear_collapses() {
        let pts: Vec<[f32; 2]> = (0..100).map(|i| pt(i as f32, 0.0)).collect();
        let path = fit_polyline_spaced(&pts, 2.0, 1.75);
        assert!(path.elements().len() <= 12);
    }

    #[test]
    fn l_shape_keeps_corner() {
        let pts = vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0)];
        let path = fit_polyline_spaced(&pts, 1.0, 1.0);
        assert!(path.elements().len() >= 3);
    }

    #[test]
    fn jittery_line_stays_bounded() {
        let mut pts = vec![pt(0.0, 0.0)];
        for i in 1..80 {
            let y = if i % 2 == 0 { 0.4 } else { -0.4 };
            pts.push(pt(i as f32, y));
        }
        let path = fit_polyline_spaced(&pts, 2.0, 1.75);
        assert!(path.elements().len() < 40);
    }

    #[test]
    fn handle_lengths_bounded_by_chord() {
        let pts: Vec<[f32; 2]> = (0..20)
            .map(|i| pt(i as f32 * 5.0, (i as f32).sin() * 3.0))
            .collect();
        let path = fit_polyline_spaced(&pts, 2.0, 1.75);
        let els = path.elements();
        for w in els.windows(4) {
            if let [PathEl::MoveTo(m), PathEl::CurveTo(c1, c2, end)] = w {
                let p0 = [m.x as f32, m.y as f32];
                let p3 = [end.x as f32, end.y as f32];
                let chord = dist(p0, p3);
                let h1 = dist(p0, [c1.x as f32, c1.y as f32]);
                let h2 = dist(p3, [c2.x as f32, c2.y as f32]);
                assert!(h1 <= chord + 0.01, "h1 {h1} chord {chord}");
                assert!(h2 <= chord + 0.01, "h2 {h2} chord {chord}");
            }
        }
    }

    #[test]
    fn finalize_avoids_duplicate_endpoint() {
        let mut pts = vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.2, 0.0)];
        finalize_endpoint(&mut pts, 1.75);
        assert_eq!(pts.len(), 2);
        assert!((pts[1][0] - 10.2).abs() < 0.01);
    }

    #[test]
    fn too_few_points_empty_or_line() {
        assert!(fit_polyline(&[], 1.0).elements().is_empty());
        assert!(fit_polyline(&[pt(0.0, 0.0)], 1.0).elements().is_empty());
        let two = fit_polyline(&[pt(0.0, 0.0), pt(1.0, 1.0)], 1.0);
        assert_eq!(two.elements().len(), 2);
    }

    #[test]
    fn a_corner_cluster_splits_once() {
        let mut pts: Vec<[f32; 2]> = (0..=40).map(|i| pt(i as f32 * 2.0, 0.0)).collect();
        pts.extend((1..=40).map(|i| pt(80.0, i as f32 * 2.0)));
        let corners = corner_indices(&pts, 6.0, 65.0_f32.to_radians());
        assert_eq!(corners, vec![0, 40, pts.len() - 1]);
    }
}
