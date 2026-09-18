//! Hit-testing stroked paths.

use kurbo::BezPath;

use crate::flatten::flatten_contours;
use crate::geom::{cumulative_arclength, dist_to_segment, half_width_at, EPS};
use crate::stroke::valid_style;
use crate::StrokeStyle;

/// Hit-test a point against the stroked region (world units).
///
/// Each contour is tested on its own. Concatenating subpaths would invent a
/// segment between the last vertex of one and the first of the next — that
/// ghost often leaves the node's AABB (classically a line toward the origin).
pub fn hit_stroke(path: &BezPath, style: &StrokeStyle, point: [f32; 2], slop: f32) -> bool {
    if !valid_style(style) || !point[0].is_finite() || !point[1].is_finite() {
        return false;
    }
    let tol = 0.25f64;
    for flat in flatten_contours(path, tol) {
        if hit_flat_stroke(&flat, style, point, slop) {
            return true;
        }
    }
    false
}

fn hit_flat_stroke(flat: &[[f32; 2]], style: &StrokeStyle, point: [f32; 2], slop: f32) -> bool {
    if flat.len() < 2 {
        return false;
    }
    let arc = cumulative_arclength(flat);
    let total = *arc.last().unwrap_or(&0.0);
    if total <= EPS {
        return false;
    }

    for i in 0..flat.len() - 1 {
        let a = flat[i];
        let b = flat[i + 1];
        let seg_len = crate::geom::dist(a, b);
        if seg_len <= EPS {
            continue;
        }
        let ab = [b[0] - a[0], b[1] - a[1]];
        let ab_len_sq = ab[0] * ab[0] + ab[1] * ab[1];
        let t_seg = if ab_len_sq > EPS * EPS {
            ((point[0] - a[0]) * ab[0] + (point[1] - a[1]) * ab[1]) / ab_len_sq
        } else {
            0.0
        }
        .clamp(0.0, 1.0);
        let arc_at = arc[i] + seg_len * t_seg;
        let t_frac = arc_at / total;
        let half = half_width_at(style, t_frac);
        if half <= EPS {
            continue;
        }
        let d = dist_to_segment(point, a, b);
        if d <= half + slop {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cap, Join, StrokeStyle};
    use kurbo::BezPath;

    fn hline() -> BezPath {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((100.0, 0.0));
        p
    }

    #[test]
    fn on_line_hits_off_line_misses() {
        let path = hline();
        let style = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        assert!(hit_stroke(&path, &style, [50.0, 0.0], 0.0));
        assert!(!hit_stroke(&path, &style, [50.0, 8.0], 0.0));
    }

    #[test]
    fn taper_thin_end_misses_where_thick_would_hit() {
        let path = hline();
        let style = StrokeStyle {
            width: 10.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: Some((1.0, 0.0)),
            dash: None,
        };
        assert!(hit_stroke(&path, &style, [5.0, 0.0], 0.0));
        // On the centerline the tapered half-width is still > 0; use lateral offset.
        assert!(hit_stroke(&path, &style, [5.0, 2.0], 0.0));
        assert!(!hit_stroke(&path, &style, [99.0, 2.0], 0.0));
    }

    fn style() -> StrokeStyle {
        StrokeStyle {
            width: 2.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        }
    }

    #[test]
    fn closed_triangle_hits_the_seam_not_the_origin() {
        let mut path = BezPath::new();
        path.move_to((400.0, 300.0));
        path.line_to((480.0, 300.0));
        path.line_to((400.0, 380.0));
        path.close_path();
        let s = style();
        assert!(hit_stroke(&path, &s, [400.0, 340.0], 0.0), "closing seam");
        assert!(!hit_stroke(&path, &s, [0.0, 0.0], 0.0), "world origin");
        assert!(
            !hit_stroke(&path, &s, [200.0, 150.0], 0.0),
            "along the line from the first vertex toward the origin"
        );
        assert!(
            !hit_stroke(&path, &s, [200.0, 300.0], 0.0),
            "infinite extension of the base, outside the AABB"
        );
    }

    #[test]
    fn disjoint_contours_do_not_invent_a_joining_segment() {
        let mut path = BezPath::new();
        path.move_to((400.0, 300.0));
        path.line_to((480.0, 300.0));
        path.close_path();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        let s = style();
        // Midpoint of the ghost line from (400,300) to (0,0).
        assert!(
            !hit_stroke(&path, &s, [200.0, 150.0], 0.0),
            "concatenated contours must not hit between subpaths"
        );
        assert!(hit_stroke(&path, &s, [440.0, 300.0], 0.0));
        assert!(hit_stroke(&path, &s, [5.0, 0.0], 0.0));
    }
}
