//! Hit-testing stroked paths.

use kurbo::BezPath;

use crate::flatten::flatten;
use crate::geom::{cumulative_arclength, dist_to_segment, half_width_at, EPS};
use crate::stroke::valid_style;
use crate::StrokeStyle;

/// Hit-test a point against the stroked region (world units).
pub fn hit_stroke(path: &BezPath, style: &StrokeStyle, point: [f32; 2], slop: f32) -> bool {
    if !valid_style(style) || !point[0].is_finite() || !point[1].is_finite() {
        return false;
    }
    let tol = 0.25f64;
    let flat = flatten(path, tol);
    if flat.len() < 2 {
        return false;
    }
    let arc = cumulative_arclength(&flat);
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
}
