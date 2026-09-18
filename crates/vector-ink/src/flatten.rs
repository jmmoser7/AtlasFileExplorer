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
///
/// `ClosePath` closes the current contour (appends the start point when the
/// seam is missing). Kurbo's flattener does not emit that seam as a `LineTo`,
/// and ignoring it left a hole in stroke hits — or, if a later contour was
/// concatenated, a ghost segment from the last vertex toward the next
/// `MoveTo` (often the world origin).
pub fn flatten_contours(path: &BezPath, tolerance: f64) -> Vec<Vec<[f32; 2]>> {
    if tolerance <= 0.0 || !tolerance.is_finite() {
        return Vec::new();
    }
    let mut contours = Vec::new();
    let mut cur = Vec::new();
    kurbo_flatten(path.elements().iter().copied(), tolerance, |el| match el {
        PathEl::MoveTo(p) => {
            finish_contour(&mut contours, &mut cur, false);
            push_pt(&mut cur, from_kurbo(p));
        }
        PathEl::LineTo(p) => push_pt(&mut cur, from_kurbo(p)),
        PathEl::ClosePath => finish_contour(&mut contours, &mut cur, true),
        _ => {}
    });
    finish_contour(&mut contours, &mut cur, false);
    contours
}

fn finish_contour(contours: &mut Vec<Vec<[f32; 2]>>, cur: &mut Vec<[f32; 2]>, closed: bool) {
    if closed && cur.len() >= 2 {
        let first = cur[0];
        if dist2(*cur.last().unwrap(), first) >= crate::geom::EPS * crate::geom::EPS {
            cur.push(first);
        }
    }
    if cur.len() >= 2 {
        contours.push(std::mem::take(cur));
    } else {
        cur.clear();
    }
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

    #[test]
    fn closed_triangle_far_from_origin_has_no_phantom_vertex() {
        let mut path = BezPath::new();
        path.move_to((400.0, 300.0));
        path.line_to((480.0, 300.0));
        path.line_to((400.0, 380.0));
        path.close_path();
        let contours = flatten_contours(&path, 0.25);
        assert_eq!(contours.len(), 1);
        let pts = &contours[0];
        assert!(
            pts.len() >= 4,
            "ClosePath must emit the closing seam, got {pts:?}"
        );
        let first = pts[0];
        let last = *pts.last().unwrap();
        assert!((first[0] - last[0]).abs() < 0.01 && (first[1] - last[1]).abs() < 0.01);
        for p in pts {
            assert!(
                p[0] >= 399.0 && p[0] <= 481.0 && p[1] >= 299.0 && p[1] <= 381.0,
                "flatten invented a point outside the triangle AABB: {p:?}"
            );
        }
    }

    #[test]
    fn two_contours_stay_separate() {
        let mut path = BezPath::new();
        path.move_to((400.0, 300.0));
        path.line_to((480.0, 300.0));
        path.close_path();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        let contours = flatten_contours(&path, 0.25);
        assert_eq!(contours.len(), 2, "ClosePath + MoveTo must not concatenate");
        assert!(contours[0].iter().all(|p| p[0] >= 399.0));
        assert!(contours[1].iter().all(|p| p[0] <= 11.0));
    }
}
