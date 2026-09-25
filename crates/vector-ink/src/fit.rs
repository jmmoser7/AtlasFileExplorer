//! Freehand stroke fitting: decimation, light smoothing, corner splits,
//! Schneider cubics with loop guards.

use kurbo::{BezPath, PathEl, Point};

/// Fit a freehand polyline (legacy entry: spacing derived from error tolerance).
pub fn fit_polyline(points: &[[f32; 2]], error_tolerance: f32) -> BezPath {
    let spacing = (error_tolerance * 0.875).max(1e-6);
    fit_polyline_spaced(points, error_tolerance, spacing)
}

/// Fit with explicit screen-space tolerances already converted to world units.
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

    pts = light_smooth(&pts);
    let runs = split_at_corners(&pts, 6.0_f32.max(min_spacing * 3.0), 65.0_f32.to_radians());

    for run in runs {
        if run.len() < 2 {
            continue;
        }
        if run.len() == 2 {
            if path.elements().is_empty() {
                path.move_to(to_k(run[0]));
            }
            path.line_to(to_k(run[1]));
            continue;
        }
        fit_run_schneider(&mut path, &run, error_tolerance);
    }
    path
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

fn split_at_corners(points: &[[f32; 2]], window: f32, threshold: f32) -> Vec<Vec<[f32; 2]>> {
    if points.len() < 3 {
        return vec![points.to_vec()];
    }
    let mut corners = vec![0usize];
    for i in 1..points.len() - 1 {
        if corner_angle(points, i, window) >= threshold {
            corners.push(i);
        }
    }
    corners.push(points.len() - 1);
    let mut runs = Vec::new();
    for w in corners.windows(2) {
        let (a, b) = (w[0], w[1]);
        runs.push(points[a..=b].to_vec());
    }
    runs
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

fn fit_run_schneider(path: &mut BezPath, run: &[[f32; 2]], tol: f32) {
    if run.len() == 2 {
        if path.elements().is_empty() {
            path.move_to(to_k(run[0]));
        }
        path.line_to(to_k(run[1]));
        return;
    }
    if run_is_collinear(run, tol) {
        if path.elements().is_empty() {
            path.move_to(to_k(run[0]));
        }
        path.line_to(to_k(*run.last().unwrap()));
        return;
    }
    schneider_rec(path, run, 0, run.len() - 1, tol);
}

fn run_is_collinear(run: &[[f32; 2]], tol: f32) -> bool {
    let a = run[0];
    let b = *run.last().unwrap();
    if dist(a, b) < 1e-6 {
        return true;
    }
    run.iter()
        .all(|p| crate::geom::dist_to_segment(*p, a, b) <= tol)
}

fn schneider_rec(path: &mut BezPath, pts: &[[f32; 2]], left: usize, right: usize, tol: f32) {
    if right <= left {
        return;
    }
    if right - left == 1 {
        if path.elements().is_empty() {
            path.move_to(to_k(pts[left]));
        }
        path.line_to(to_k(pts[right]));
        return;
    }

    let p0 = pts[left];
    let p3 = pts[right];
    let chord = dist(p0, p3);
    if chord < 1e-6 {
        return;
    }

    let slice = &pts[left..=right];
    let mut t = chord_length_params(slice);
    let (t0, t1) = end_tangents(slice);
    let (mut c1, mut c2) = initial_controls(p0, p3, t0, t1, chord);

    for _ in 0..4 {
        let prev = t.clone();
        (c1, c2) = least_squares_controls(slice, &t, p0, p3);
        apply_loop_guards(p0, p3, chord, &mut c1, &mut c2);
        t = reparameterize(slice, p0, p3, c1, c2, &t);
        if !params_increasing(&prev, &t) {
            break;
        }
    }

    let err = max_fit_error(slice, p0, p3, c1, c2, &t);
    if err <= tol || right - left <= 2 {
        emit_cubic(
            path,
            p0,
            c1,
            c2,
            p3,
            left == 0 && path.elements().is_empty(),
        );
        return;
    }

    let split = left + max_error_index(slice, p0, p3, c1, c2, &t);
    if split <= left || split >= right {
        emit_cubic(
            path,
            p0,
            c1,
            c2,
            p3,
            left == 0 && path.elements().is_empty(),
        );
        return;
    }
    schneider_rec(path, pts, left, split, tol);
    schneider_rec(path, pts, split, right, tol);
}

fn emit_cubic(
    path: &mut BezPath,
    p0: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    p3: [f32; 2],
    move_first: bool,
) {
    if move_first {
        path.move_to(to_k(p0));
    }
    path.curve_to(to_k(c1), to_k(c2), to_k(p3));
}

fn chord_length_params(pts: &[[f32; 2]]) -> Vec<f64> {
    let mut t = vec![0.0_f64];
    for i in 0..pts.len() - 1 {
        t.push(t[i] + dist(pts[i], pts[i + 1]) as f64);
    }
    let total = t.last().copied().unwrap_or(1.0).max(1e-6);
    t.iter().map(|v| v / total).collect()
}

fn end_tangents(pts: &[[f32; 2]]) -> ([f32; 2], [f32; 2]) {
    let n = pts.len();
    let t0 = normalize(sub(pts[1.min(n - 1)], pts[0]));
    let t1 = normalize(sub(pts[n - 1], pts[(n - 2).max(0)]));
    (t0, t1)
}

fn initial_controls(
    p0: [f32; 2],
    p3: [f32; 2],
    t0: [f32; 2],
    t1: [f32; 2],
    chord: f32,
) -> ([f32; 2], [f32; 2]) {
    let a = chord / 3.0;
    let c1 = add(p0, scale(t0, a));
    let c2 = add(p3, scale(t1, -a));
    (c1, c2)
}

fn least_squares_controls(
    pts: &[[f32; 2]],
    t: &[f64],
    p0: [f32; 2],
    p3: [f32; 2],
) -> ([f32; 2], [f32; 2]) {
    let mut c00 = 0.0_f64;
    let mut c01 = 0.0_f64;
    let mut c11 = 0.0_f64;
    let mut x0 = 0.0_f64;
    let mut x1 = 0.0_f64;
    let mut y0 = 0.0_f64;
    let mut y1 = 0.0_f64;

    for (i, &pt) in pts
        .iter()
        .enumerate()
        .skip(1)
        .take(pts.len().saturating_sub(2))
    {
        let ti = t[i];
        let b0 = (1.0 - ti).powi(3);
        let b1 = 3.0 * (1.0 - ti).powi(2) * ti;
        let b2 = 3.0 * (1.0 - ti) * ti.powi(2);
        let b3 = ti.powi(3);
        let a1 = b1;
        let a2 = b2;
        let tmp = [
            pt[0] as f64 - (p0[0] as f64 * b0 + p3[0] as f64 * b3),
            pt[1] as f64 - (p0[1] as f64 * b0 + p3[1] as f64 * b3),
        ];
        c00 += a1 * a1;
        c01 += a1 * a2;
        c11 += a2 * a2;
        x0 += a1 * tmp[0];
        x1 += a2 * tmp[0];
        y0 += a1 * tmp[1];
        y1 += a2 * tmp[1];
    }

    let det = c00 * c11 - c01 * c01;
    if det.abs() < 1e-12 {
        let chord = dist(p0, p3);
        return initial_controls(p0, p3, end_tangents(pts).0, end_tangents(pts).1, chord);
    }
    let alpha1 = (c11 * x0 - c01 * x1) / det;
    let alpha2 = (c00 * x1 - c01 * x0) / det;
    let beta1 = (c11 * y0 - c01 * y1) / det;
    let beta2 = (c00 * y1 - c01 * y0) / det;

    let chord = dist(p0, p3);
    let (t0, t1) = end_tangents(pts);
    let mut c1 = add(p0, scale(t0, alpha1.max(1e-6) as f32 * chord / 3.0));
    let mut c2 = add(p3, scale(t1, -alpha2.max(1e-6) as f32 * chord / 3.0));
    let _ = beta1;
    let _ = beta2;
    apply_loop_guards(p0, p3, chord, &mut c1, &mut c2);
    (c1, c2)
}

fn apply_loop_guards(p0: [f32; 2], p3: [f32; 2], chord: f32, c1: &mut [f32; 2], c2: &mut [f32; 2]) {
    let third = chord / 3.0;
    clamp_handle(p0, c1, chord);
    clamp_handle(p3, c2, chord);
    if dist(p0, *c1) < 1e-6 || dist(p3, *c2) < 1e-6 {
        *c1 = add(p0, scale(sub(*c1, p0), third / dist(p0, *c1).max(1e-6)));
        *c2 = add(p3, scale(sub(*c2, p3), third / dist(p3, *c2).max(1e-6)));
    }
    if handles_cross(p0, p3, *c1, *c2) {
        *c1 = add(p0, scale(sub(p3, p0), 1.0 / 3.0));
        *c2 = add(p3, scale(sub(p0, p3), 1.0 / 3.0));
    }
}

fn clamp_handle(anchor: [f32; 2], control: &mut [f32; 2], max_len: f32) {
    let v = sub(*control, anchor);
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len > max_len {
        *control = add(anchor, scale(v, max_len / len));
    }
}

fn handles_cross(p0: [f32; 2], p3: [f32; 2], c1: [f32; 2], c2: [f32; 2]) -> bool {
    let d = sub(p3, p0);
    let n = [-d[1], d[0]];
    (sub(c1, p0)[0] * n[0] + sub(c1, p0)[1] * n[1]).signum()
        != (sub(c2, p0)[0] * n[0] + sub(c2, p0)[1] * n[1]).signum()
}

fn reparameterize(
    pts: &[[f32; 2]],
    p0: [f32; 2],
    p3: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    t: &[f64],
) -> Vec<f64> {
    pts.iter()
        .enumerate()
        .map(|(i, &p)| {
            if i == 0 {
                return 0.0;
            }
            if i == pts.len() - 1 {
                return 1.0;
            }
            let ti = t[i];
            let q = cubic_at(p0, c1, c2, p3, ti as f32);
            let q1 = cubic_at(p0, c1, c2, p3, (ti - 0.001).max(0.0) as f32);
            let deriv = sub(q, q1);
            let denom = dot(deriv, deriv).max(1e-8);
            let dotp = (p[0] - q[0]) * deriv[0] + (p[1] - q[1]) * deriv[1];
            (ti + dotp as f64 / denom as f64).clamp(0.0, 1.0)
        })
        .collect()
}

fn params_increasing(prev: &[f64], next: &[f64]) -> bool {
    prev.iter().zip(next.iter()).all(|(a, b)| *b >= *a - 1e-9)
}

fn cubic_at(p0: [f32; 2], c1: [f32; 2], c2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let u = 1.0 - t;
    [
        u.powi(3) * p0[0]
            + 3.0 * u.powi(2) * t * c1[0]
            + 3.0 * u * t.powi(2) * c2[0]
            + t.powi(3) * p3[0],
        u.powi(3) * p0[1]
            + 3.0 * u.powi(2) * t * c1[1]
            + 3.0 * u * t.powi(2) * c2[1]
            + t.powi(3) * p3[1],
    ]
}

fn max_fit_error(
    pts: &[[f32; 2]],
    p0: [f32; 2],
    p3: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    t: &[f64],
) -> f32 {
    let mut best = 0.0_f32;
    for (i, &p) in pts.iter().enumerate() {
        let q = cubic_at(p0, c1, c2, p3, t[i] as f32);
        best = best.max(dist(p, q));
    }
    best
}

fn max_error_index(
    pts: &[[f32; 2]],
    p0: [f32; 2],
    p3: [f32; 2],
    c1: [f32; 2],
    c2: [f32; 2],
    t: &[f64],
) -> usize {
    let mut best_i = 0;
    let mut best = 0.0_f32;
    for (i, &p) in pts.iter().enumerate() {
        let q = cubic_at(p0, c1, c2, p3, t[i] as f32);
        let d = dist(p, q);
        if d > best {
            best = d;
            best_i = i;
        }
    }
    best_i
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale(v: [f32; 2], s: f32) -> [f32; 2] {
    [v[0] * s, v[1] * s]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn normalize(v: [f32; 2]) -> [f32; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len < 1e-8 {
        return v;
    }
    scale(v, 1.0 / len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;

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
}
