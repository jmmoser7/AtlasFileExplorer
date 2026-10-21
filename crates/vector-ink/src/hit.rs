//! Hit-testing stroked paths.

use kurbo::BezPath;

use crate::flatten::flatten_contours;
use crate::geom::{
    cumulative_arclength, dist_to_segment, half_width_at, EPS, MAX_CAP_SLOPE, MITER_LIMIT,
};
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
    if !outline_might_hit(path, style, ends, widths, point, slop) {
        return false;
    }
    hit_stroke_outline_exact(path, style, ends, widths, ease, point, slop)
}

/// Reject distant picks before flattening and constructing the stroke outline.
/// A Bezier stays inside its control hull. Widths, taper, limited miters and
/// tangential round caps can expand that hull; none may be clipped by this test.
fn outline_might_hit(
    path: &BezPath,
    style: &StrokeStyle,
    ends: [Cap; 2],
    widths: Option<&[f32]>,
    point: [f32; 2],
    slop: f32,
) -> bool {
    let width = widths
        .unwrap_or_default()
        .iter()
        .copied()
        .fold(style.width, f32::max) as f64;
    let taper = match style.taper {
        None => 1.0,
        Some(crate::Taper::Linear(a, b)) => a.abs().max(b.abs()) as f64,
        Some(crate::Taper::Ends(tip)) => tip.abs().max(1.0) as f64,
    };
    let cap_reach = if ends.contains(&Cap::Round) || style.cap == Cap::Round {
        let slope = MAX_CAP_SLOPE as f64;
        slope + (1.0 + slope * slope).sqrt()
    } else {
        std::f64::consts::SQRT_2
    };
    let bounds = path.control_box();
    // The outline uses f32 arithmetic for both placement and cap expansion.
    let scale = bounds
        .x0
        .abs()
        .max(bounds.x1.abs())
        .max(bounds.y0.abs())
        .max(bounds.y1.abs());
    let reach = width * 0.5 * taper * cap_reach.max(MITER_LIMIT as f64) + slop.max(0.0) as f64;
    let reach = reach + (scale + reach.abs()).max(1.0) * f32::EPSILON as f64 * 8.0;
    let [x, y] = point.map(f64::from);
    !(x < bounds.x0 - reach
        || x > bounds.x1 + reach
        || y < bounds.y0 - reach
        || y > bounds.y1 + reach)
}

fn hit_stroke_outline_exact(
    path: &BezPath,
    style: &StrokeStyle,
    ends: [Cap; 2],
    widths: Option<&[f32]>,
    ease: TipEase,
    point: [f32; 2],
    slop: f32,
) -> bool {
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
mod tests;
