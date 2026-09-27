//! Stroking: mesh, outline, bounds, and subpath extraction.

use kurbo::{BezPath, PathEl};

use crate::dash::dash_runs;
use crate::flatten::{flatten, flatten_contours};
use crate::geom::{cumulative_arclength, dist, from_kurbo, is_finite_pt, lerp, to_kurbo, EPS};
use crate::mesh::{run_outline, run_pieces, tessellate_run};
use crate::trim::Polygon;
use crate::{InkMesh, StrokeStyle, TintPiece, TipEase};

/// Samples per segment, at least, where a smooth blend changes the tip: the
/// piecewise-linear strip stays within 0.3% of the tip change of the curve.
pub(crate) const SMOOTH_TIP_STEPS: usize = 16;

pub(crate) fn valid_style(style: &StrokeStyle) -> bool {
    style.width.is_finite() && style.width > 0.0
}

/// Tessellate a stroked path into a feathered AA mesh.
pub fn stroke_mesh(path: &BezPath, style: &StrokeStyle, feather: f32, tolerance: f64) -> InkMesh {
    stroke_mesh_with(path, style, None, feather, tolerance)
}

/// [`stroke_mesh`] with a full width at every on-curve vertex: each `MoveTo`,
/// then the end of each segment, in path order. A segment blends between its
/// end widths by arc length, as `ease` says; `ClosePath` returns to the
/// contour's first width. A count that does not match the path's vertices
/// strokes at `style.width`. The taper, if any, still scales the widths.
pub fn stroke_mesh_tipped(
    path: &BezPath,
    style: &StrokeStyle,
    widths: &[f32],
    ease: TipEase,
    feather: f32,
    tolerance: f64,
) -> InkMesh {
    let tips = Tipping::new(widths, None, ease);
    stroke_mesh_with(path, style, Some(tips), feather, tolerance)
}

/// [`stroke_mesh_tipped`] with a straight RGBA color (`0..=1`) at every
/// vertex as well, blended between vertices exactly like the widths. The
/// mesh then carries one color per vertex ([`InkMesh::colors`]). Colors
/// whose count does not match `widths` are ignored.
pub fn stroke_mesh_tinted(
    path: &BezPath,
    style: &StrokeStyle,
    widths: &[f32],
    colors: &[[f32; 4]],
    ease: TipEase,
    feather: f32,
    tolerance: f64,
) -> InkMesh {
    let tips = Tipping::new(widths, Some(colors), ease);
    stroke_mesh_with(path, style, Some(tips), feather, tolerance)
}

/// Per-vertex tips: a full width and optionally a straight RGBA color at
/// every on-curve vertex, blended between vertices as `ease` says.
#[derive(Clone, Copy)]
struct Tipping<'a> {
    widths: &'a [f32],
    colors: Option<&'a [[f32; 4]]>,
    ease: TipEase,
}

impl<'a> Tipping<'a> {
    fn new(widths: &'a [f32], colors: Option<&'a [[f32; 4]]>, ease: TipEase) -> Self {
        Tipping {
            widths,
            colors: colors.filter(|c| c.len() == widths.len()),
            ease,
        }
    }

    fn tip(&self, i: usize) -> Tip {
        let c = self.colors.map_or([1.0; 4], |c| c[i]);
        [self.widths[i], c[0], c[1], c[2], c[3]]
    }
}

/// Width, then straight RGBA, at one point of a tipped stroke.
type Tip = [f32; 5];

fn lerp_tip(a: Tip, b: Tip, t: f32) -> Tip {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}

fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| lerp(a[i], b[i], t))
}

fn stroke_mesh_with(
    path: &BezPath,
    style: &StrokeStyle,
    tips: Option<Tipping>,
    feather: f32,
    tolerance: f64,
) -> InkMesh {
    if !valid_style(style)
        || !feather.is_finite()
        || feather < 0.0
        || (tolerance <= 0.0 || !tolerance.is_finite())
    {
        return InkMesh::default();
    }
    let mut mesh = InkMesh::default();
    mesh.vertices.reserve(256);
    mesh.indices.reserve(512);

    for sub in stroke_subpaths(path, tips, tolerance) {
        for run in stroke_runs(sub, style) {
            tessellate_run(
                &mut mesh,
                &run.points,
                run.widths.as_deref(),
                run.colors.as_deref(),
                style,
                feather,
                run.closed,
                tolerance,
            );
        }
    }

    mesh
}

struct SubPath {
    points: Vec<[f32; 2]>,
    closed: bool,
    /// Full width per point, for a tipped stroke.
    widths: Option<Vec<f32>>,
    /// Straight RGBA per point, for a tinted stroke.
    colors: Option<Vec<[f32; 4]>>,
}

struct Run {
    points: Vec<[f32; 2]>,
    widths: Option<Vec<f32>>,
    colors: Option<Vec<[f32; 4]>>,
    closed: bool,
}

fn stroke_subpaths(path: &BezPath, tips: Option<Tipping>, tolerance: f64) -> Vec<SubPath> {
    tips.and_then(|t| tipped_subpaths(path, t, tolerance))
        .unwrap_or_else(|| subpaths(path, tolerance))
}

fn stroke_runs(mut sub: SubPath, style: &StrokeStyle) -> Vec<Run> {
    if sub.points.len() < 2 {
        return Vec::new();
    }
    let Some((pattern, phase)) = &style.dash else {
        return vec![Run {
            points: sub.points,
            widths: sub.widths,
            colors: sub.colors,
            closed: sub.closed,
        }];
    };
    if sub.closed {
        sub.points.push(sub.points[0]);
        if let Some(w) = &mut sub.widths {
            w.push(w[0]);
        }
        if let Some(c) = &mut sub.colors {
            c.push(c[0]);
        }
    }
    let lengths =
        (sub.widths.is_some() || sub.colors.is_some()).then(|| cumulative_arclength(&sub.points));
    dash_runs(&sub.points, pattern, *phase)
        .into_iter()
        .map(|(start, mut points)| {
            let closed = sub.closed
                && points.len() > 2
                && dist2(points[0], *points.last().unwrap()) < EPS * EPS;
            if closed {
                points.pop();
            }
            let widths = sub
                .widths
                .as_deref()
                .zip(lengths.as_deref())
                .map(|(w, l)| values_along(&points, start, l, w, lerp));
            let colors = sub
                .colors
                .as_deref()
                .zip(lengths.as_deref())
                .map(|(c, l)| values_along(&points, start, l, c, lerp_color));
            Run {
                points,
                widths,
                colors,
                closed,
            }
        })
        .collect()
}

/// Per-point values for a dash run that starts `start` along a contour
/// whose points sit at `lengths` with `values`. A run past the closed seam
/// wraps.
fn values_along<T: Copy>(
    points: &[[f32; 2]],
    start: f32,
    lengths: &[f32],
    values: &[T],
    mix: fn(T, T, f32) -> T,
) -> Vec<T> {
    let total = lengths.last().copied().unwrap_or(0.0);
    let mut at = start;
    let mut out = Vec::with_capacity(points.len());
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            at += dist(points[i - 1], *p);
        }
        let s = if total > EPS && at > total + EPS {
            at - total
        } else {
            at
        };
        out.push(value_at(lengths, values, s, mix));
    }
    out
}

fn value_at<T: Copy>(lengths: &[f32], values: &[T], at: f32, mix: fn(T, T, f32) -> T) -> T {
    let i = lengths.partition_point(|&l| l < at);
    if i == 0 {
        return values[0];
    }
    if i >= lengths.len() {
        return values[values.len() - 1];
    }
    let span = lengths[i] - lengths[i - 1];
    let t = if span > EPS {
        (at - lengths[i - 1]) / span
    } else {
        1.0
    };
    mix(values[i - 1], values[i], t)
}

/// Flatten segment by segment so every point knows its tip. `None` when the
/// tips do not match the path's on-curve vertices.
fn tipped_subpaths(path: &BezPath, tips: Tipping, tolerance: f64) -> Option<Vec<SubPath>> {
    let vertices = path
        .elements()
        .iter()
        .filter(|el| !matches!(el, PathEl::ClosePath))
        .count();
    if vertices != tips.widths.len() || tips.widths.iter().any(|w| !w.is_finite() || *w < 0.0) {
        return None;
    }
    let tinted = tips.colors.is_some();
    let ease = tips.ease;
    let mut out = Vec::new();
    let mut points: Vec<[f32; 2]> = Vec::new();
    let mut ts: Vec<Tip> = Vec::new();
    let mut next = (0..vertices).map(|i| tips.tip(i));
    let mut last = kurbo::Point::ZERO;
    let mut last_t: Tip = [0.0; 5];
    let mut contour_start = (kurbo::Point::ZERO, last_t);
    let flush = |out: &mut Vec<SubPath>, points: &mut Vec<_>, ts: &mut Vec<Tip>, closed| {
        if !points.is_empty() {
            let ts = std::mem::take(ts);
            out.push(SubPath {
                points: std::mem::take(points),
                closed,
                widths: Some(ts.iter().map(|t| t[0]).collect()),
                colors: tinted.then(|| ts.iter().map(|t| [t[1], t[2], t[3], t[4]]).collect()),
            });
        }
    };
    for el in path.elements() {
        let (seg, end_t) = match *el {
            PathEl::MoveTo(p) => {
                flush(&mut out, &mut points, &mut ts, false);
                last = p;
                last_t = next.next()?;
                contour_start = (p, last_t);
                points.push(from_kurbo(p));
                ts.push(last_t);
                continue;
            }
            PathEl::ClosePath => {
                // The implicit closing edge blends back to the first tip.
                let (start, start_t) = contour_start;
                if !points.is_empty() && (start - last).hypot() > EPS as f64 {
                    push_tipped_segment(
                        &mut points,
                        &mut ts,
                        (last, last_t),
                        PathEl::LineTo(start),
                        start_t,
                        ease,
                        tolerance,
                    );
                }
                if points.len() >= 2 && dist2(points[0], *points.last().unwrap()) < EPS * EPS {
                    points.pop();
                    ts.pop();
                }
                flush(&mut out, &mut points, &mut ts, true);
                (last, last_t) = contour_start;
                continue;
            }
            PathEl::LineTo(_) | PathEl::QuadTo(..) | PathEl::CurveTo(..) => (*el, next.next()?),
        };
        if points.is_empty() {
            points.push(from_kurbo(last));
            ts.push(last_t);
        }
        push_tipped_segment(
            &mut points,
            &mut ts,
            (last, last_t),
            seg,
            end_t,
            ease,
            tolerance,
        );
        last = match seg {
            PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => p,
            _ => last,
        };
        last_t = end_t;
    }
    flush(&mut out, &mut points, &mut ts, false);
    Some(out)
}

/// Flatten one segment from `from` onto `points`, blending its tips from
/// the start tip to `end` by arc length. A smooth blend that changes the
/// tip is sampled at least [`SMOOTH_TIP_STEPS`] times, and densely enough
/// that the sampled width strays from the smoothstep by at most `tolerance`
/// (its second derivative peaks at `6 * dw`, so the chord error is at most
/// `6 * dw / (8 * steps^2)`).
fn push_tipped_segment(
    points: &mut Vec<[f32; 2]>,
    ts: &mut Vec<Tip>,
    from: (kurbo::Point, Tip),
    seg: PathEl,
    end: Tip,
    ease: TipEase,
    tolerance: f64,
) {
    let (start, start_t) = from;
    let mut piece = BezPath::new();
    piece.move_to(start);
    piece.push(seg);
    let mut flat = flatten(&piece, tolerance);
    let dw = (end[0] - start_t[0]).abs() as f64;
    let recolors = (1..5).any(|i| (end[i] - start_t[i]).abs() > 1e-3);
    if ease == TipEase::Smooth && (dw > tolerance || recolors) {
        let steps = (0.75 * dw / tolerance.max(1e-3)).sqrt().ceil() as usize;
        flat = densify(&flat, steps.clamp(SMOOTH_TIP_STEPS, 256));
    }
    let lengths = cumulative_arclength(&flat);
    let total = lengths.last().copied().unwrap_or(0.0);
    for (p, l) in flat.iter().zip(&lengths).skip(1) {
        points.push(*p);
        ts.push(if total > EPS {
            lerp_tip(start_t, end, ease.weight(l / total))
        } else {
            end
        });
    }
}

/// `flat` resampled so no chord is longer than `1 / steps` of its length.
pub(crate) fn densify(flat: &[[f32; 2]], steps: usize) -> Vec<[f32; 2]> {
    let total = cumulative_arclength(flat).last().copied().unwrap_or(0.0);
    if flat.len() < 2 || total <= EPS {
        return flat.to_vec();
    }
    let max = total / steps as f32;
    let mut out = vec![flat[0]];
    for pair in flat.windows(2) {
        let n = (dist(pair[0], pair[1]) / max).ceil().max(1.0) as usize;
        for i in 1..=n {
            let t = i as f32 / n as f32;
            out.push([
                lerp(pair[0][0], pair[1][0], t),
                lerp(pair[0][1], pair[1][1], t),
            ]);
        }
    }
    out
}

fn subpaths(path: &BezPath, tolerance: f64) -> Vec<SubPath> {
    if path.elements().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut chunk = BezPath::new();
    for el in path.elements() {
        match el {
            PathEl::MoveTo(p) => {
                if !chunk.elements().is_empty() {
                    out.push(finish_chunk(&chunk, tolerance));
                    chunk = BezPath::new();
                }
                chunk.move_to(*p);
            }
            PathEl::LineTo(p) => chunk.line_to(*p),
            PathEl::QuadTo(p1, p2) => chunk.quad_to(*p1, *p2),
            PathEl::CurveTo(p1, p2, p3) => chunk.curve_to(*p1, *p2, *p3),
            PathEl::ClosePath => {
                chunk.close_path();
                out.push(finish_chunk(&chunk, tolerance));
                chunk = BezPath::new();
            }
        }
    }
    if !chunk.elements().is_empty() {
        out.push(finish_chunk(&chunk, tolerance));
    }
    out
}

fn finish_chunk(chunk: &BezPath, tolerance: f64) -> SubPath {
    let closed = chunk
        .elements()
        .last()
        .map(|e| matches!(e, PathEl::ClosePath))
        .unwrap_or(false);
    let mut points = flatten(chunk, tolerance);
    if closed && points.len() >= 2 && dist2(points[0], *points.last().unwrap()) < EPS * EPS {
        points.pop();
    }
    SubPath {
        points,
        closed,
        widths: None,
        colors: None,
    }
}

#[inline]
fn dist2(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

/// The stroked region as a closed outline path (for SVG export).
pub fn stroke_outline(path: &BezPath, style: &StrokeStyle, tolerance: f64) -> BezPath {
    stroke_outline_with(path, style, None, tolerance)
}

/// [`stroke_outline`] with per-vertex widths, as in [`stroke_mesh_tipped`].
pub fn stroke_outline_tipped(
    path: &BezPath,
    style: &StrokeStyle,
    widths: &[f32],
    ease: TipEase,
    tolerance: f64,
) -> BezPath {
    let tips = Tipping::new(widths, None, ease);
    stroke_outline_with(path, style, Some(tips), tolerance)
}

/// The stroked region of [`stroke_mesh_tinted`] cut into the quads between
/// its consecutive sections, each with its two end colors, for an SVG export
/// that fills every piece with a two-stop linear gradient. Together the
/// pieces tile [`stroke_outline_tipped`]'s region; the gradient runs from
/// `from` to `to`, the centers of the piece's two sections, which is how the
/// mesh interpolates across the same quad. Empty when the colors or widths
/// do not match the path's vertices.
pub fn stroke_pieces_tinted(
    path: &BezPath,
    style: &StrokeStyle,
    widths: &[f32],
    colors: &[[f32; 4]],
    ease: TipEase,
    tolerance: f64,
) -> Vec<TintPiece> {
    if !valid_style(style) || (tolerance <= 0.0 || !tolerance.is_finite()) {
        return Vec::new();
    }
    let tips = Tipping::new(widths, Some(colors), ease);
    if tips.colors.is_none() {
        return Vec::new();
    }
    let Some(subs) = tipped_subpaths(path, tips, tolerance) else {
        return Vec::new();
    };
    let mut pieces = Vec::new();
    for sub in subs {
        for run in stroke_runs(sub, style) {
            if let Some(colors) = &run.colors {
                pieces.extend(run_pieces(
                    &run.points,
                    run.widths.as_deref(),
                    colors,
                    style,
                    run.closed,
                    tolerance,
                ));
            }
        }
    }
    pieces
}

fn stroke_outline_with(
    path: &BezPath,
    style: &StrokeStyle,
    tips: Option<Tipping>,
    tolerance: f64,
) -> BezPath {
    if !valid_style(style) || (tolerance <= 0.0 || !tolerance.is_finite()) {
        return BezPath::new();
    }
    let mut outline = BezPath::new();
    for sub in stroke_subpaths(path, tips, tolerance) {
        for run in stroke_runs(sub, style) {
            for ring in run_outline(
                &run.points,
                run.widths.as_deref(),
                style,
                run.closed,
                tolerance,
            ) {
                outline.move_to(to_kurbo(ring[0]));
                for p in &ring[1..] {
                    outline.line_to(to_kurbo(*p));
                }
                outline.close_path();
            }
        }
    }

    outline
}

/// Closed region of a stroked open polyline (round-cap ribbon). Used by
/// Join when an open curve unions with a filled shape.
pub fn stroke_ribbon(pts: &[[f32; 2]], style: &StrokeStyle) -> Option<Polygon> {
    if pts.len() < 2 || !valid_style(style) {
        return None;
    }
    let mut bez = BezPath::new();
    bez.move_to(to_kurbo(pts[0]));
    for p in &pts[1..] {
        bez.line_to(to_kurbo(*p));
    }
    let outline = stroke_outline(&bez, style, 0.35);
    let contours = flatten_contours(&outline, 0.35);
    let contours: Polygon = contours.into_iter().filter(|c| c.len() >= 3).collect();
    if contours.is_empty() {
        None
    } else {
        Some(contours)
    }
}

/// Bounding box of the stroked path including width and feather.
pub fn stroke_bounds(
    path: &BezPath,
    style: &StrokeStyle,
    feather: f32,
) -> Option<([f32; 2], [f32; 2])> {
    if path.elements().is_empty() || !valid_style(style) || !feather.is_finite() {
        return None;
    }
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    let mut any = false;
    for el in path.elements() {
        if let PathEl::MoveTo(p) | PathEl::LineTo(p) = el {
            expand_pt(&mut min, &mut max, &mut any, from_kurbo(*p), style, feather);
        } else if let PathEl::QuadTo(_, p2) = el {
            expand_pt(
                &mut min,
                &mut max,
                &mut any,
                from_kurbo(*p2),
                style,
                feather,
            );
        } else if let PathEl::CurveTo(_, _, p3) = el {
            expand_pt(
                &mut min,
                &mut max,
                &mut any,
                from_kurbo(*p3),
                style,
                feather,
            );
        }
    }
    if !any {
        return None;
    }
    Some((min, max))
}

fn expand_pt(
    min: &mut [f32; 2],
    max: &mut [f32; 2],
    any: &mut bool,
    p: [f32; 2],
    style: &StrokeStyle,
    feather: f32,
) {
    if !is_finite_pt(p) {
        return;
    }
    *any = true;
    let pad = style.width * 0.5 + feather * 0.5;
    min[0] = min[0].min(p[0] - pad);
    min[1] = min[1].min(p[1] - pad);
    max[0] = max[0].max(p[0] + pad);
    max[1] = max[1].max(p[1] + pad);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cap, Join, StrokeStyle};

    fn line_path(x0: f32, y0: f32, x1: f32, y1: f32) -> BezPath {
        let mut p = BezPath::new();
        p.move_to((x0 as f64, y0 as f64));
        p.line_to((x1 as f64, y1 as f64));
        p
    }

    #[test]
    fn horizontal_line_mesh_symmetric_alphas() {
        let path = line_path(0.0, 0.0, 100.0, 0.0);
        let style = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let mesh = stroke_mesh(&path, &style, 1.0, 0.01);
        assert!(!mesh.vertices.is_empty());
        for v in &mesh.vertices {
            assert!(v.alpha == 0.0 || v.alpha == 1.0);
            assert!((v.pos[1].abs() - 0.0).abs() <= 2.5 || v.pos[1].abs() <= 3.5);
        }
        let min_x = mesh
            .vertices
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = mesh
            .vertices
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - 0.0).abs() < 0.1);
        assert!((max_x - 100.0).abs() < 0.1);
    }

    #[test]
    fn taper_half_widths_decrease() {
        let path = line_path(0.0, 0.0, 100.0, 0.0);
        let style = StrokeStyle {
            width: 10.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: Some(crate::Taper::Linear(1.0, 0.0)),
            dash: None,
        };
        let mesh = stroke_mesh(&path, &style, 0.5, 0.01);
        let mut samples: Vec<(f32, f32)> = mesh
            .vertices
            .iter()
            .filter(|v| v.alpha == 1.0)
            .map(|v| (v.pos[0], v.pos[1].abs()))
            .collect();
        samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let mut last_x = -1.0f32;
        let mut last_half = f32::MAX;
        for (x, y) in samples {
            if (x - last_x).abs() < 2.0 {
                continue;
            }
            last_x = x;
            if y > last_half + 0.01 {
                panic!("half width grew at x={x}: {y} vs {last_half}");
            }
            last_half = y;
        }
        assert!(last_x > 50.0);
    }

    fn inside(mesh: &InkMesh, p: [f32; 2]) -> bool {
        let verts: Vec<[f32; 2]> = mesh.vertices.iter().map(|v| v.pos).collect();
        crate::point_in_mesh(&verts, &mesh.indices, p)
    }

    #[test]
    fn tipped_stroke_follows_its_vertex_widths() {
        let mut path = line_path(0.0, 0.0, 50.0, 0.0);
        path.line_to((100.0, 0.0));
        let style = StrokeStyle {
            width: 10.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let widths = [2.0, 2.0, 10.0];
        let mesh = stroke_mesh_tipped(&path, &style, &widths, TipEase::Linear, 0.0, 0.05);
        assert!(inside(&mesh, [25.0, 0.8]), "narrow first span has ink");
        assert!(!inside(&mesh, [25.0, 3.0]), "first span stays 2 wide");
        assert!(inside(&mesh, [98.0, 4.5]), "second span widens to 10");
        let outline = stroke_outline_tipped(&path, &style, &widths, TipEase::Linear, 0.05);
        let bounds = kurbo::Shape::bounding_box(&outline);
        assert!((bounds.y1 - 5.0).abs() < 0.01, "outline peaks at 10 wide");
        assert!(bounds.y0 > -5.01);

        let mismatched = stroke_mesh_tipped(&path, &style, &[2.0], TipEase::Linear, 0.0, 0.05);
        assert!(inside(&mismatched, [25.0, 4.5]), "wrong count is uniform");
    }

    /// Half-width of an outline over a horizontal stroke at `x`.
    fn outline_half(outline: &BezPath, x: f32) -> f32 {
        let contours = flatten_contours(outline, 0.01);
        let (mut lo, mut hi) = (0.0_f32, 20.0_f32);
        for _ in 0..40 {
            let mid = (lo + hi) * 0.5;
            if crate::point_in_polygon(&contours, [x, mid]) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    #[test]
    fn smooth_tips_blend_by_smoothstep_with_zero_slope_at_vertices() {
        let mut path = line_path(0.0, 0.0, 100.0, 0.0);
        path.line_to((200.0, 0.0));
        let style = StrokeStyle {
            width: 20.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let widths = [2.0, 20.0, 2.0];
        let smooth = stroke_outline_tipped(&path, &style, &widths, TipEase::Smooth, 0.01);
        let linear = stroke_outline_tipped(&path, &style, &widths, TipEase::Linear, 0.01);
        for x in [25.0_f32, 50.0, 90.0, 150.0] {
            let s = 1.0 - (x - 100.0).abs() / 100.0;
            let want = (2.0 + 18.0 * TipEase::Smooth.weight(s)) * 0.5;
            assert!((outline_half(&smooth, x) - want).abs() < 0.05, "x={x}");
            let want = (2.0 + 18.0 * s) * 0.5;
            assert!((outline_half(&linear, x) - want).abs() < 0.05, "x={x}");
        }
        let at = outline_half(&smooth, 100.0);
        for x in [99.0_f32, 101.0] {
            assert!(
                (at - outline_half(&smooth, x)).abs() < 0.01,
                "flat at the vertex"
            );
        }

        let mut closed = line_path(0.0, 0.0, 100.0, 0.0);
        closed.line_to((100.0, 100.0));
        closed.close_path();
        let ring = stroke_outline_tipped(&closed, &style, &[2.0, 20.0, 2.0], TipEase::Smooth, 0.01);
        assert!(
            !ring.elements().is_empty(),
            "closed tipped strokes still stroke"
        );
    }

    #[test]
    fn tinted_pieces_blend_colors_like_widths() {
        let mut path = line_path(0.0, 0.0, 100.0, 0.0);
        path.line_to((200.0, 0.0));
        let style = StrokeStyle {
            width: 10.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let widths = [10.0; 3];
        let colors = [
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mesh = stroke_mesh_tinted(&path, &style, &widths, &colors, TipEase::Linear, 1.0, 0.05);
        assert_eq!(mesh.colors.len(), mesh.vertices.len());
        let untinted = stroke_mesh_tipped(&path, &style, &widths, TipEase::Linear, 1.0, 0.05);
        assert!(untinted.colors.is_empty());
        let red_at = |ease: TipEase, x: f32| {
            let pieces = stroke_pieces_tinted(&path, &style, &widths, &colors, ease, 0.05);
            let piece = pieces
                .iter()
                .find(|p| crate::point_in_polygon(&vec![p.quad.to_vec()], [x, 1.0]))
                .expect("a piece covers the point");
            let d = [piece.to[0] - piece.from[0], piece.to[1] - piece.from[1]];
            let q = [x - piece.from[0], 1.0 - piece.from[1]];
            let t = ((q[0] * d[0] + q[1] * d[1]) / (d[0] * d[0] + d[1] * d[1])).clamp(0.0, 1.0);
            lerp(piece.colors[0][0], piece.colors[1][0], t)
        };
        assert!((red_at(TipEase::Linear, 50.0) - 0.5).abs() < 1e-3);
        assert!((red_at(TipEase::Linear, 150.0) - 0.5).abs() < 1e-3);
        assert!((red_at(TipEase::Smooth, 25.0) - TipEase::Smooth.weight(0.25)).abs() < 3e-3);
        let mismatched =
            stroke_pieces_tinted(&path, &style, &widths, &colors[..2], TipEase::Linear, 0.05);
        assert!(mismatched.is_empty());
    }

    #[test]
    fn round_cap_extends_half_width_not_full_width() {
        let path = line_path(0.0, 0.0, 80.0, 0.0);
        let style = StrokeStyle {
            width: 8.0,
            cap: Cap::Round,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let mesh = stroke_mesh(&path, &style, 0.0, 0.01);
        let min_x = mesh
            .vertices
            .iter()
            .map(|v| v.pos[0])
            .fold(f32::MAX, f32::min);
        let max_y = mesh
            .vertices
            .iter()
            .map(|v| v.pos[1].abs())
            .fold(0.0f32, f32::max);
        // Diameter of a round cap equals the stroke width, so the start
        // extends ~4 world units past x=0 — not ~8 (the old double offset).
        assert!(
            min_x > -5.25 && min_x < -3.25,
            "round cap should extend half-width past the end, got min_x={min_x}"
        );
        assert!(
            max_y < 4.75,
            "round cap should not be fatter than the stroke, got half={max_y}"
        );
    }

    #[test]
    fn round_cap_more_vertices_than_butt() {
        let path = line_path(0.0, 0.0, 50.0, 0.0);
        let butt = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let round = StrokeStyle {
            cap: Cap::Round,
            ..butt.clone()
        };
        let m_butt = stroke_mesh(&path, &butt, 1.0, 0.01);
        let m_round = stroke_mesh(&path, &round, 1.0, 0.01);
        assert!(m_round.vertices.len() > m_butt.vertices.len());
    }

    #[test]
    fn exported_outline_uses_round_caps_and_curved_dash_runs() {
        let path = BezPath::from_svg("M0 0Q50 100 100 0").unwrap();
        let style = StrokeStyle {
            width: 8.0,
            cap: Cap::Round,
            join: Join::Round,
            taper: Some(crate::Taper::Linear(1.0, 0.5)),
            dash: Some((vec![200.0, 10.0], 0.0)),
        };
        let outline = stroke_outline(&path, &style, 0.02);
        let contours = flatten_contours(&outline, 0.02);
        assert!(
            crate::point_in_polygon(&contours, [50.0, 50.0]),
            "dash follows the curve"
        );
        assert!(
            !crate::point_in_polygon(&contours, [50.0, 0.0]),
            "no endpoint chord"
        );
        assert!(
            contours[0].iter().any(|p| p[0] < 0.0),
            "round start cap extends beyond the start"
        );
    }

    #[test]
    fn closed_triangle_no_extra_caps_vs_open() {
        let mut tri = BezPath::new();
        tri.move_to((0.0, 0.0));
        tri.line_to((40.0, 0.0));
        tri.line_to((20.0, 35.0));
        tri.close_path();
        let style = StrokeStyle {
            width: 3.0,
            cap: Cap::Round,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let closed_mesh = stroke_mesh(&tri, &style, 0.5, 0.01);
        assert!(!closed_mesh.vertices.is_empty());

        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.line_to((40.0, 0.0));
        open.line_to((20.0, 35.0));
        let open_mesh = stroke_mesh(&open, &style, 0.5, 0.01);
        assert_ne!(closed_mesh.vertices.len(), open_mesh.vertices.len());
    }

    #[test]
    fn degenerate_empty_no_panic() {
        let empty = BezPath::new();
        let style = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        assert!(stroke_mesh(&empty, &style, 1.0, 0.01).vertices.is_empty());
        assert!(stroke_bounds(&empty, &style, 1.0).is_none());
        let mut one = BezPath::new();
        one.move_to((0.0, 0.0));
        assert!(stroke_mesh(&one, &style, 1.0, 0.01).vertices.is_empty());
        let zero_w = StrokeStyle {
            width: 0.0,
            ..style.clone()
        };
        let path = line_path(0.0, 0.0, 10.0, 0.0);
        assert!(stroke_mesh(&path, &zero_w, 1.0, 0.01).vertices.is_empty());
        let nan_w = StrokeStyle {
            width: f32::NAN,
            ..style.clone()
        };
        assert!(stroke_mesh(&path, &nan_w, 1.0, 0.01).vertices.is_empty());
        assert!(!crate::hit_stroke(&path, &nan_w, [5.0, 0.0], 0.0));
    }

    #[test]
    fn bounds_match_width_and_feather() {
        let path = line_path(0.0, 0.0, 100.0, 0.0);
        let style = StrokeStyle {
            width: 4.0,
            cap: Cap::Butt,
            join: Join::Miter,
            taper: None,
            dash: None,
        };
        let feather = 1.0;
        let (min, max) = stroke_bounds(&path, &style, feather).unwrap();
        let pad = style.width * 0.5 + feather * 0.5;
        assert!((min[1] - (-pad)).abs() < 0.01);
        assert!((max[1] - pad).abs() < 0.01);
    }

    #[test]
    fn ribbon_covers_the_stroke_centerline() {
        let style = StrokeStyle {
            width: 8.0,
            cap: Cap::Round,
            join: Join::Round,
            taper: None,
            dash: None,
        };
        let poly = stroke_ribbon(&[[0.0, 0.0], [40.0, 0.0]], &style).expect("ribbon");
        assert!(crate::point_in_polygon(&poly, [20.0, 0.0]));
        assert!(crate::point_in_polygon(&poly, [20.0, 3.0]));
        assert!(!crate::point_in_polygon(&poly, [20.0, 20.0]));
    }
}
