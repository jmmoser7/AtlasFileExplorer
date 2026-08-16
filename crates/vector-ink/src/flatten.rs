//! Flatten `BezPath` curves to polylines.

use kurbo::{flatten as kurbo_flatten, BezPath, PathEl};

use crate::geom::{from_kurbo, is_finite_pt};

/// Flatten a path to a polyline with the given tolerance (world units).
/// Multiple contours are concatenated (legacy callers). Prefer
/// [`flatten_contours`] when holes or disjoint subpaths matter.
pub fn flatten(path: &BezPath, tolerance: f64) -> Vec<[f32; 2]> {
    flatten_contours(path, tolerance)
        .into_iter()
        .flatten()
        .collect()
}

/// Flatten a path, splitting on `MoveTo` so each contour stays separate.
pub fn flatten_contours(path: &BezPath, tolerance: f64) -> Vec<Vec<[f32; 2]>> {
    if tolerance <= 0.0 || !tolerance.is_finite() {
        return Vec::new();
    }
    let mut contours = Vec::new();
    let mut cur = Vec::new();
    kurbo_flatten(path.elements().iter().copied(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            if cur.len() >= 2 {
                contours.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
            push_pt(&mut cur, from_kurbo(p));
        }
        PathEl::LineTo(p) => push_pt(&mut cur, from_kurbo(p)),
        _ => {}
    });
    if cur.len() >= 2 {
        contours.push(cur);
    }
    contours
}

fn push_pt(out: &mut Vec<[f32; 2]>, p: [f32; 2]) {
    if !is_finite_pt(p) {
        return;
    }
    if let Some(last) = out.last() {
        if dist2(*last, p) < crate::geom::EPS * crate::geom::EPS {
            return;
        }
    }
    out.push(p);
}

#[inline]
fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Circle, Shape};

    #[test]
    fn circle_flatten_within_tolerance() {
        let circle = Circle::new((0.0, 0.0), 50.0);
        let path: BezPath = circle.to_path(0.01);
        let pts = flatten(&path, 0.5);
        assert!(pts.len() > 8);
        let r = 50.0f32;
        for p in &pts {
            let d = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((d - r).abs() <= 0.5 + 0.01, "point {:?} radius {d}", p);
        }
    }
}
