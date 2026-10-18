//! Hit-testing stroked paths.

use kurbo::BezPath;

use crate::flatten::flatten_contours;
use crate::geom::{cumulative_arclength, dist_to_segment, half_width_at, EPS};
use crate::stroke::valid_style;
use crate::{Cap, StrokeStyle, TipEase};

/// Pick the actual painted boundary, including variable widths, caps, joins,
/// dashes and taper. Slop expands only that boundary, never the widest tip.
pub fn hit_stroke_outline(
    path: &BezPath,
    style: &StrokeStyle,
    ends: [Cap; 2],
    widths: Option<&[f32]>,
    ease: TipEase,
    point: [f32; 2],
    slop: f32,
) -> bool {
    if !valid_style(style) || !point.into_iter().all(f32::is_finite) {
        return false;
    }
    let outline = crate::stroke_outline_ends(path, style, ends, widths, ease, 0.25);
    let contours = flatten_contours(&outline, 0.25);
    crate::point_in_polygon(&contours, point)
        || contours.iter().any(|contour| {
            contour
                .windows(2)
                .any(|p| dist_to_segment(point, p[0], p[1]) <= slop.max(0.0))
        })
}

/// Hit-test a point against the stroked region (world units).
///
/// Each contour is tested on its own. Concatenating subpaths would invent a
/// segment between the last vertex of one and the first of the next — that
/// ghost often leaves the node's AABB (classically a line toward the origin).
pub fn hit_stroke(path: &BezPath, style: &StrokeStyle, point: [f32; 2], slop: f32) -> bool {
    CachedStrokeHit::new(path, style).is_some_and(|cached| cached.hit(point, slop))
}

/// Reusable hit-test geometry for a stroked path. Flattening and cumulative
/// arclength are computed once so a moving pick can test many points cheaply.
pub struct CachedStrokeHit {
    style: StrokeStyle,
    contours: Vec<CachedContour>,
}

struct CachedContour {
    points: Vec<[f32; 2]>,
    arclength: Vec<f32>,
    total: f32,
}

impl CachedStrokeHit {
    /// Prepare the exact geometry used by [`hit_stroke`]. Invalid styles have
    /// no hit region and return `None`.
    pub fn new(path: &BezPath, style: &StrokeStyle) -> Option<Self> {
        if !valid_style(style) {
            return None;
        }
        let contours = flatten_contours(path, 0.25)
            .into_iter()
            .map(|points| {
                let arclength = cumulative_arclength(&points);
                let total = *arclength.last().unwrap_or(&0.0);
                CachedContour {
                    points,
                    arclength,
                    total,
                }
            })
            .collect();
        Some(Self {
            style: style.clone(),
            contours,
        })
    }

    /// Test one point against the cached painted boundary.
    pub fn hit(&self, point: [f32; 2], slop: f32) -> bool {
        if !point[0].is_finite() || !point[1].is_finite() {
            return false;
        }
        self.contours
            .iter()
            .any(|contour| hit_cached_flat_stroke(contour, &self.style, point, slop))
    }

    /// Number of flat line segments visited by a hit when it misses.
    pub fn segment_count(&self) -> usize {
        self.contours
            .iter()
            .map(|contour| contour.points.len().saturating_sub(1))
            .sum()
    }
}

fn hit_cached_flat_stroke(
    contour: &CachedContour,
    style: &StrokeStyle,
    point: [f32; 2],
    slop: f32,
) -> bool {
    let flat = &contour.points;
    if flat.len() < 2 || contour.total <= EPS {
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
        let arc_at = contour.arclength[i] + seg_len * t_seg;
        let t_frac = arc_at / contour.total;
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

    #[test]
    fn variable_width_pick_follows_outline_and_cap() {
        let style = StrokeStyle {
            width: 100.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let hit = |p| {
            hit_stroke_outline(
                &hline(),
                &style,
                [Cap::Butt; 2],
                Some(&[2.0, 100.0]),
                TipEase::Linear,
                p,
                4.0,
            )
        };
        assert!(hit([5.0, 4.0]));
        assert!(
            !hit([5.0, 30.0]),
            "the widest tip must not expand the narrow end"
        );
        assert!(!hit([-8.0, 0.0]), "flat cap has only screen slop");
        assert!(hit([95.0, 40.0]));
    }

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
    fn cached_pick_reuses_the_same_hit_boundary_for_many_points() {
        let path = hline();
        let style = style();
        let cached = CachedStrokeHit::new(&path, &style).unwrap();
        for (point, expected) in [
            ([0.0, 0.0], true),
            ([50.0, 1.0], true),
            ([50.0, 8.0], false),
            ([100.0, 0.0], true),
        ] {
            assert_eq!(cached.hit(point, 0.0), expected);
            assert_eq!(
                cached.hit(point, 0.0),
                hit_stroke(&path, &style, point, 0.0)
            );
        }
    }

    #[test]
    fn taper_thin_end_misses_where_thick_would_hit() {
        let path = hline();
        let style = StrokeStyle {
            width: 10.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: Some(crate::Taper::Linear(1.0, 0.0)),
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
