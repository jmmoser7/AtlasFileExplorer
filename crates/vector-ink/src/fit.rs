//! Freehand polyline fitting (RDP + Catmull-Rom cubics).

use kurbo::{BezPath, Point};

/// Fit a freehand polyline to a smooth `BezPath`.
pub fn fit_polyline(points: &[[f32; 2]], tolerance: f32) -> BezPath {
    let mut path = BezPath::new();
    if points.len() < 2 {
        return path;
    }
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return path;
    }
    if points.len() == 2 {
        path.move_to(to_k(points[0]));
        path.line_to(to_k(points[1]));
        return path;
    }

    let simplified = rdp(points, tolerance);
    if simplified.len() < 2 {
        return path;
    }
    if simplified.len() == 2 {
        path.move_to(to_k(simplified[0]));
        path.line_to(to_k(simplified[1]));
        return path;
    }

    path.move_to(to_k(simplified[0]));
    for i in 0..simplified.len() - 1 {
        let p0 = simplified[i.saturating_sub(1)];
        let p1 = simplified[i];
        let p2 = simplified[i + 1];
        let p3 = simplified[(i + 2).min(simplified.len() - 1)];

        let c1 = catmull_to_bezier(p0, p1, p2, p3, true);
        let c2 = catmull_to_bezier(p0, p1, p2, p3, false);
        path.curve_to(to_k(c1), to_k(c2), to_k(p2));
    }
    path
}

fn to_k(p: [f32; 2]) -> Point {
    Point::new(p[0] as f64, p[1] as f64)
}

fn rdp(points: &[[f32; 2]], tolerance: f32) -> Vec<[f32; 2]> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    *keep.last_mut().unwrap() = true;

    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let mut max_d = 0.0f32;
        let mut idx = start;
        for i in start + 1..end {
            let d = dist_to_seg(points[i], points[start], points[end]);
            if d > max_d {
                max_d = d;
                idx = i;
            }
        }
        if max_d > tolerance {
            keep[idx] = true;
            stack.push((start, idx));
            stack.push((idx, end));
        }
    }

    points
        .iter()
        .zip(keep.iter())
        .filter_map(|(p, &k)| k.then_some(*p))
        .collect()
}

fn dist_to_seg(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    crate::geom::dist_to_segment(p, a, b)
}

fn catmull_to_bezier(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    first: bool,
) -> [f32; 2] {
    if first {
        [p1[0] + (p2[0] - p0[0]) / 6.0, p1[1] + (p2[1] - p0[1]) / 6.0]
    } else {
        [p2[0] - (p3[0] - p1[0]) / 6.0, p2[1] - (p3[1] - p1[1]) / 6.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::pt;

    #[test]
    fn collinear_collapses() {
        let pts: Vec<[f32; 2]> = (0..100).map(|i| pt(i as f32, 0.0)).collect();
        let path = fit_polyline(&pts, 0.5);
        assert!(path.elements().len() <= 4);
    }

    #[test]
    fn l_shape_keeps_corner() {
        let pts = vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0)];
        let path = fit_polyline(&pts, 0.5);
        assert!(path.elements().len() >= 3);
    }

    #[test]
    fn too_few_points_empty_or_line() {
        assert!(fit_polyline(&[], 1.0).elements().is_empty());
        assert!(fit_polyline(&[pt(0.0, 0.0)], 1.0).elements().is_empty());
        let two = fit_polyline(&[pt(0.0, 0.0), pt(1.0, 1.0)], 1.0);
        assert_eq!(two.elements().len(), 2);
    }
}
