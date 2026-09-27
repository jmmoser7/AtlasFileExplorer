//! Per-vertex stroke style on a curve (P1.curve.vertex-style): which path
//! vertices a picked grip owns, and how an edit at picked grips rewrites the
//! path's tips. Both interpreters paint the result through
//! [`crate::geom::tipped_stroke`].
//!
//! Grips follow the curve's geometry (P1.curve.grips): a circular arc has
//! three (start, through, end) and its tips are derived straight along the
//! sweep from those three; any other single-contour path has one grip per
//! anchor, and the anchor that closes a path owns the duplicate end vertex.

use vector_ink::kurbo::{
    BezPath, CubicBez, Line, ParamCurve, ParamCurveArclen, ParamCurveNearest, PathEl,
    PathSeg as KSeg, Point,
};

use crate::geom::{arc_grip_points, path_data_to_world_bez, tip_ease};
use crate::scene::{Corner, PathData, PathSeg, Rgba, Stroke, StrokeSpan, WorldRect};

/// `a` blended toward `b` by `t` (0 = `a`, 1 = `b`).
pub fn lerp_span(a: StrokeSpan, b: StrokeSpan, t: f32) -> StrokeSpan {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let mut color = [0u8; 4];
    for (i, c) in color.iter_mut().enumerate() {
        *c = mix(a.color.0[i] as f32, b.color.0[i] as f32).round() as u8;
    }
    StrokeSpan {
        width: mix(a.width, b.width),
        softness: mix(a.softness, b.softness),
        color: Rgba(color),
        texture: if t < 0.5 { a.texture } else { b.texture },
    }
}

enum Grips {
    /// One grip per anchor; `dup` when the closing segment ends on a copy of
    /// the start vertex, which anchor 0 then owns too.
    Anchors {
        dup: bool,
    },
    Arc,
}

fn grips_of(path: &PathData, world: &BezPath) -> Option<Grips> {
    if !path.extra.is_empty() || path.is_empty() {
        return None;
    }
    if !path.closed && arc_grip_points(world).is_some() {
        return Some(Grips::Arc);
    }
    let ends: Vec<Point> = world
        .elements()
        .iter()
        .filter_map(|el| el.end_point())
        .collect();
    let dup = path.closed && ends.len() >= 2 && (ends[ends.len() - 1] - ends[0]).hypot() <= 1e-6;
    Some(Grips::Anchors { dup })
}

/// One tip per vertex, as the path paints now: stored tips at their painted
/// width (`PathData::vector_widths`), else the stroke itself everywhere.
fn painted_tips(path: &PathData, stroke: &Stroke) -> Vec<StrokeSpan> {
    let n = 1 + path.segs.len();
    match path.vector_widths(stroke) {
        Some(widths) => path
            .tips
            .iter()
            .zip(widths)
            .map(|(t, width)| StrokeSpan { width, ..*t })
            .collect(),
        None if path.tips.len() == n => path
            .tips
            .iter()
            .map(|t| StrokeSpan {
                width: stroke.width,
                ..*t
            })
            .collect(),
        None => vec![StrokeSpan::of(stroke); n],
    }
}

/// The painted tip at each grip of the curve, in grip order.
pub fn grip_tips(
    path: &PathData,
    stroke: &Stroke,
    rect: WorldRect,
    rotation_deg: f32,
) -> Option<Vec<StrokeSpan>> {
    let world = path_data_to_world_bez(path, rect, rotation_deg);
    let tips = painted_tips(path, stroke);
    Some(match grips_of(path, &world)? {
        Grips::Anchors { dup } => tips[..tips.len() - usize::from(dup)].to_vec(),
        Grips::Arc => {
            let n = path.segs.len();
            let mid = if n.is_multiple_of(2) {
                tips[n / 2]
            } else {
                lerp_span(tips[n / 2], tips[n / 2 + 1], 0.5)
            };
            vec![tips[0], mid, tips[n]]
        }
    })
}

/// Rewrite the path's tips from one tip per grip. Equal tips clear the list
/// and set the stroke itself; otherwise the stroke width becomes the widest
/// tip, so the path paints each tip at its own width. `false` (nothing
/// changed) when `grips` does not fit the curve.
pub fn set_grip_tips(
    path: &mut PathData,
    stroke: &mut Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    grips: &[StrokeSpan],
) -> bool {
    let world = path_data_to_world_bez(path, rect, rotation_deg);
    let Some(model) = grips_of(path, &world) else {
        return false;
    };
    let uniform = grips.iter().all(|g| *g == grips[0]);
    let tips = match model {
        Grips::Anchors { dup } => {
            if grips.len() + usize::from(dup) != 1 + path.segs.len() {
                return false;
            }
            let mut tips = grips.to_vec();
            if dup {
                tips.push(grips[0]);
            }
            tips
        }
        Grips::Arc => {
            if grips.len() != 3 {
                return false;
            }
            if !uniform && !path.segs.len().is_multiple_of(2) {
                split_middle_span(path);
            }
            let world = path_data_to_world_bez(path, rect, rotation_deg);
            arc_tips(&world, path.segs.len(), [grips[0], grips[1], grips[2]])
        }
    };
    write_tips(path, stroke, tips);
    true
}

/// The tip a point of a drawn curve was placed with (P1.curve.tip-chord):
/// the tool's width, color, and opacity at that moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedTip {
    pub width: f32,
    pub color: Rgba,
    pub opacity: f32,
}

impl PlacedTip {
    /// `a` blended toward `b` by `t` (0 = `a`, 1 = `b`).
    pub fn lerp(a: PlacedTip, b: PlacedTip, t: f32) -> PlacedTip {
        let t = t.clamp(0.0, 1.0);
        let mix = |x: f32, y: f32| x + (y - x) * t;
        PlacedTip {
            width: mix(a.width, b.width),
            color: Rgba(std::array::from_fn(|i| {
                mix(a.color.0[i] as f32, b.color.0[i] as f32).round() as u8
            })),
            opacity: mix(a.opacity, b.opacity),
        }
    }

    /// Straight color with the opacity folded into alpha, as it paints.
    pub fn rgba(self) -> Rgba {
        let [r, g, b, a] = self.color.0;
        Rgba([
            r,
            g,
            b,
            (a as f32 * self.opacity.clamp(0.0, 1.0)).round() as u8,
        ])
    }
}

/// Placed tips as a hard curve stores them: the node opacity is the most
/// opaque tip's, and each tip's alpha carries its share of that, so the
/// curve paints every tip at the opacity it was placed with. When every tip
/// was placed at 0 % the node is 0 and each tip keeps its full alpha, so
/// raising the node opacity later shows the tips as drawn. The softness
/// and texture are the stroke's.
pub fn placed_spans(stroke: &Stroke, tips: &[PlacedTip]) -> (f32, Vec<StrokeSpan>) {
    let top = tips.iter().map(|t| t.opacity).fold(0.0_f32, f32::max);
    let spans = tips
        .iter()
        .map(|t| {
            let share = if top > 0.0 { t.opacity / top } else { 1.0 };
            let mut color = t.color;
            color.0[3] = (color.0[3] as f32 * share.clamp(0.0, 1.0)).round() as u8;
            StrokeSpan {
                width: t.width,
                color,
                ..StrokeSpan::of(stroke)
            }
        })
        .collect();
    (top, spans)
}

/// Give each grip of a just-drawn curve the tip its point was placed with,
/// in grip order ([`placed_spans`]). Equal tips leave a plain stroke. The
/// node opacity the curve takes, or `None` when `tips` does not fit it.
pub fn set_grip_placed_tips(
    path: &mut PathData,
    stroke: &mut Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    tips: &[PlacedTip],
) -> Option<f32> {
    if tips.is_empty() {
        return None;
    }
    let (opacity, spans) = placed_spans(stroke, tips);
    set_grip_tips(path, stroke, rect, rotation_deg, &spans).then_some(opacity)
}

/// Write one tip per vertex of the one-contour `path`. Equal tips clear
/// the list and set the stroke; otherwise the stroke width becomes the
/// widest tip. `false` when `tips` does not fit the path.
pub fn set_vertex_tips(path: &mut PathData, stroke: &mut Stroke, tips: Vec<StrokeSpan>) -> bool {
    if tips.is_empty() || !path.extra.is_empty() || tips.len() != 1 + path.segs.len() {
        return false;
    }
    write_tips(path, stroke, tips);
    true
}

/// Apply `edit` to the tips at the `picked` grips. `false` when the curve
/// has no grips or none of `picked` is one of them.
pub fn edit_grip_tips(
    path: &mut PathData,
    stroke: &mut Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    picked: &[usize],
    edit: impl Fn(&mut StrokeSpan),
) -> bool {
    let Some(mut grips) = grip_tips(path, stroke, rect, rotation_deg) else {
        return false;
    };
    let mut any = false;
    for &i in picked {
        if let Some(tip) = grips.get_mut(i) {
            edit(tip);
            any = true;
        }
    }
    any && set_grip_tips(path, stroke, rect, rotation_deg, &grips)
}

/// Carry `old`'s tips onto its rebuilt geometry `new` (a grip drag, a
/// direct edit): grip for grip when the grips still fit, else vertex for
/// vertex when the vertex count is unchanged. Otherwise the tips are
/// dropped with the old geometry.
pub fn keep_tips(
    old: (&PathData, WorldRect, f32),
    new: (&mut PathData, WorldRect, f32),
    stroke: &mut Stroke,
) {
    let (old_path, old_rect, old_rot) = old;
    let (new_path, new_rect, new_rot) = new;
    if old_path.tips.is_empty() {
        return;
    }
    let before = *stroke;
    if let Some(grips) = grip_tips(old_path, &before, old_rect, old_rot) {
        if set_grip_tips(new_path, stroke, new_rect, new_rot, &grips) {
            return;
        }
    }
    if old_path.tips.len() == 1 + new_path.segs.len() && new_path.extra.is_empty() {
        new_path.tips = old_path.tips.clone();
    }
}

/// Carry `old`'s per-vertex corner overrides onto its rebuilt geometry
/// `new`, vertex for vertex, so a moved vertex keeps its own corner and the
/// fillet re-clamps it to its new edges. A closing copy of the start
/// vertex follows the shape's corner on either side. Dropped when the
/// vertex count changed.
pub fn keep_corner_amounts(old: &PathData, new: &mut PathData) {
    if old.corner_amounts.len() != 1 + old.segs.len() || !new.extra.is_empty() {
        return;
    }
    let vertices = |p: &PathData| 1 + p.segs.len() - usize::from(closes_on_start(p));
    let n = vertices(old);
    if n != vertices(new) {
        return;
    }
    let mut amounts = old.corner_amounts[..n].to_vec();
    amounts.resize(1 + new.segs.len(), None);
    new.corner_amounts = amounts;
}

/// Whether `path` stores per-vertex style an edit that changes its vertices
/// has to carry: tips, or a corner override.
pub fn has_vertex_style(path: &PathData) -> bool {
    !path.tips.is_empty() || path.corner_amounts.iter().any(Option::is_some)
}

/// One vertex's style as it paints: its tip at painted width and color
/// (a stamped stroke's tip as stored), and its corner override.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexStyle {
    pub tip: StrokeSpan,
    pub corner: Option<f32>,
}

/// The style of each of `count` vertices of a curve stroked with `stroke`,
/// for carrying vertices onto another path (an object-level Join). A curve
/// with no path (a Line) or whose tips do not fit paints the stroke itself
/// at every vertex.
pub fn vertex_styles(path: Option<&PathData>, stroke: &Stroke, count: usize) -> Vec<VertexStyle> {
    let path = path.filter(|p| vertex_count(p) == count);
    let tips = path.and_then(|p| vertex_tips(p, stroke));
    let corners = path.map(|p| p.corner_amounts.as_slice()).unwrap_or(&[]);
    (0..count)
        .map(|i| VertexStyle {
            tip: tips.as_ref().map_or(StrokeSpan::of(stroke), |t| t[i]),
            corner: corners.get(i).copied().flatten(),
        })
        .collect()
}

/// Write `styles`, one per vertex, onto `path` stroked with `stroke`: a hard
/// stroke's width becomes its widest tip and equal tips set the stroke
/// itself ([`set_grip_tips`]). Corner overrides are kept only where they
/// apply, on a straight-segment polyline. Nothing changes when `styles`
/// does not fit the path.
pub fn apply_vertex_styles(path: &mut PathData, stroke: &mut Stroke, styles: &[VertexStyle]) {
    if styles.is_empty() || styles.len() != vertex_count(path) {
        return;
    }
    let tips: Vec<StrokeSpan> = styles.iter().map(|s| s.tip).collect();
    if stroke.paints_as_stamp() {
        path.tips = tips;
    } else {
        write_tips(path, stroke, tips);
    }
    path.corner_amounts.clear();
    if crate::geom::path_is_line_polyline(path) && styles.iter().any(|s| s.corner.is_some()) {
        path.corner_amounts = styles.iter().map(|s| s.corner).collect();
    }
}

/// First vertex, vertex count and closed flag of each contour of `path`, in
/// vertex order (the primary contour, then `extra`).
fn contours(path: &PathData) -> Vec<(usize, usize, bool)> {
    let mut out = vec![(0, 1 + path.segs.len(), path.closed)];
    let mut first = 1 + path.segs.len();
    for c in &path.extra {
        out.push((first, 1 + c.segs.len(), c.closed));
        first += 1 + c.segs.len();
    }
    out
}

fn vertex_count(path: &PathData) -> usize {
    contours(path).iter().map(|c| c.1).sum()
}

/// One tip per vertex as `path` paints it, when it paints per vertex: a hard
/// stroke at its painted widths and colors (`PathData::vector_widths`,
/// `PathData::vector_colors`, else the stroke's own), a stamped stroke's
/// tips as stored.
fn vertex_tips(path: &PathData, stroke: &Stroke) -> Option<Vec<StrokeSpan>> {
    if path.tips.is_empty() || path.tips.len() != vertex_count(path) {
        return None;
    }
    if stroke.paints_as_stamp() {
        return Some(path.tips.clone());
    }
    let widths = path.vector_widths(stroke);
    let colors = path.vector_colors();
    Some(
        path.tips
            .iter()
            .enumerate()
            .map(|(i, t)| StrokeSpan {
                width: widths.as_ref().map_or(stroke.width, |w| w[i]),
                softness: t.softness,
                color: colors.as_ref().map_or(stroke.color, |c| c[i]),
                texture: t.texture,
            })
            .collect(),
    )
}

/// The tips `path` paints at vertex parameters `at`, for pieces cut from it
/// or geometry refit over it. A vertex parameter is the vertex a segment
/// leaves plus the arc-length fraction along that segment; a closed
/// contour's implicit closing edge leaves its last vertex and ends on its
/// first. Between vertices the tip blends as the painters do: a hard stroke
/// by [`tip_ease`] on the path as `corner` paints it (straight on sharp or
/// chamfered polylines, lines and arcs, smoothstep on filleted polylines and
/// other curves), a stamped stroke per segment (straight on a line,
/// smoothstep on a curve, as `vector_ink::tipped_contours`). `None` when
/// `path` paints one tip everywhere.
pub fn split_tips_at(
    path: &PathData,
    stroke: &Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    corner: Corner,
    at: &[f32],
) -> Option<Vec<StrokeSpan>> {
    let tips = vertex_tips(path, stroke)?;
    let stamp = stroke.paints_as_stamp();
    let painted = crate::geom::tipped_stroke_world_path(path, rect, rotation_deg, corner).0;
    let hard_ease = tip_ease(&painted);
    let segs: Vec<&PathSeg> = std::iter::once(&path.segs)
        .chain(path.extra.iter().map(|c| &c.segs))
        .flat_map(|s| s.iter().map(Some).chain(std::iter::once(None)))
        .map(|s| s.unwrap_or(&PathSeg::Line { to: [0.0, 0.0] }))
        .collect();
    let ease_at = |i: usize| {
        if !stamp {
            hard_ease
        } else if matches!(
            segs.get(i),
            Some(PathSeg::Quad { .. } | PathSeg::Cubic { .. })
        ) {
            vector_ink::TipEase::Smooth
        } else {
            vector_ink::TipEase::Linear
        }
    };
    let spans = contours(path);
    Some(
        at.iter()
            .map(|&p| {
                let p = if p.is_finite() { p.max(0.0) } else { 0.0 };
                let i = (p.floor() as usize).min(tips.len() - 1);
                let f = (p - i as f32).clamp(0.0, 1.0);
                let (first, count, closed) = spans
                    .iter()
                    .copied()
                    .find(|(first, count, _)| i < first + count)
                    .unwrap_or((0, tips.len(), false));
                let next = if i + 1 < first + count {
                    i + 1
                } else if closed {
                    first
                } else {
                    i
                };
                lerp_span(tips[i], tips[next], ease_at(i).weight(f))
            })
            .collect(),
    )
}

/// Write `old`'s per-vertex style onto `new`, whose vertex `i` sits at
/// vertex parameter `params[i]` of `old` ([`split_tips_at`]). Tips are
/// sampled there, so a kept vertex keeps its own and a new one takes the
/// stroke's value at its spot. A corner override stays on a vertex that is
/// an old vertex and nowhere else; a closing copy of the start follows the
/// shape's corner, as in [`keep_corner_amounts`]. `stroke` is `old`'s stroke
/// and becomes `new`'s: a hard stroke's width is its widest painted tip.
/// `old` is the source path, its rect, rotation, and shape corner.
pub fn carry_vertex_style(
    old: (&PathData, WorldRect, f32, Corner),
    new: &mut PathData,
    stroke: &mut Stroke,
    params: &[f32],
) {
    let (old_path, rect, rotation_deg, corner) = old;
    if params.len() != vertex_count(new) {
        return;
    }
    if let Some(tips) = split_tips_at(old_path, stroke, rect, rotation_deg, corner, params) {
        if stroke.paints_as_stamp() {
            new.tips = tips;
        } else {
            write_tips(new, stroke, tips);
        }
    }
    new.corner_amounts.clear();
    let old_vertices = 1 + old_path.segs.len();
    if old_path.corner_amounts.len() != old_vertices || !new.extra.is_empty() {
        return;
    }
    let mut amounts: Vec<Option<f32>> = params
        .iter()
        .map(|&p| {
            let r = p.round();
            if (p - r).abs() > VERTEX_SNAP || r < 0.0 {
                return None;
            }
            let i = r as usize;
            let i = if old_path.closed && i == old_vertices {
                0
            } else {
                i
            };
            old_path.corner_amounts.get(i).copied().flatten()
        })
        .collect();
    if closes_on_start(new) {
        if let Some(last) = amounts.last_mut() {
            *last = None;
        }
    }
    if amounts.iter().any(Option::is_some) {
        new.corner_amounts = amounts;
    }
}

/// A point within this fraction of a segment of one of its ends is that
/// vertex.
const VERTEX_SNAP: f32 = 1e-4;

/// World segments of `path` in vertex order, each with the vertex it leaves
/// and the vertex it ends on (a closing edge ends on its contour's first).
fn world_segments(
    path: &PathData,
    rect: WorldRect,
    rotation_deg: f32,
) -> Vec<(KSeg, usize, usize)> {
    let bez = path_data_to_world_bez(path, rect, rotation_deg);
    let mut out = Vec::new();
    let (mut next, mut from, mut first) = (0usize, 0usize, 0usize);
    let (mut last, mut start) = (Point::ZERO, Point::ZERO);
    for el in bez.elements() {
        let seg = match *el {
            PathEl::MoveTo(p) => {
                (first, from, last, start) = (next, next, p, p);
                next += 1;
                continue;
            }
            PathEl::ClosePath => {
                if (last - start).hypot() > 1e-9 {
                    out.push((KSeg::Line(Line::new(last, start)), from, first));
                }
                continue;
            }
            PathEl::LineTo(p) => KSeg::Line(Line::new(last, p)),
            PathEl::QuadTo(c, p) => KSeg::Quad(vector_ink::kurbo::QuadBez::new(last, c, p)),
            PathEl::CurveTo(c1, c2, p) => KSeg::Cubic(CubicBez::new(last, c1, c2, p)),
        };
        out.push((seg, from, next));
        (from, last) = (next, seg.end());
        next += 1;
    }
    out
}

/// Vertex parameter at arc-length fraction `frac` of a segment from vertex
/// `from` to vertex `to`; an end within [`VERTEX_SNAP`] is that vertex.
fn param_at(from: usize, to: usize, frac: f64) -> f32 {
    let frac = frac as f32;
    if frac <= VERTEX_SNAP {
        from as f32
    } else if frac >= 1.0 - VERTEX_SNAP {
        to as f32
    } else {
        from as f32 + frac
    }
}

/// The vertex parameter ([`split_tips_at`]) of each world point of a piece
/// cut from `path`: points on it within `tolerance`, in order along the
/// piece. Each is looked for first on the previous point's segment and its
/// neighbors, so a piece that runs along the path never jumps to a span
/// that crosses it; a point off the path takes its nearest spot.
pub fn locate_vertex_params(
    path: &PathData,
    rect: WorldRect,
    rotation_deg: f32,
    points: &[[f32; 2]],
    tolerance: f64,
) -> Vec<f32> {
    let segs = world_segments(path, rect, rotation_deg);
    if segs.is_empty() {
        return vec![0.0; points.len()];
    }
    let reach = tolerance * tolerance;
    let mut last = 0usize;
    points
        .iter()
        .map(|p| {
            let q = Point::new(p[0] as f64, p[1] as f64);
            let near = |k: usize| {
                let hit = segs[k].0.nearest(q, 1e-9);
                (hit.distance_sq, hit.t)
            };
            let window = [last, last + 1, last + 2, last.wrapping_sub(1)];
            let (k, t) = window
                .into_iter()
                .filter(|k| *k < segs.len())
                .find_map(|k| {
                    let (d, t) = near(k);
                    (d <= reach).then_some((k, t))
                })
                .unwrap_or_else(|| {
                    (0..segs.len())
                        .map(|k| (k, near(k)))
                        .min_by(|a, b| a.1 .0.total_cmp(&b.1 .0))
                        .map(|(k, (_, t))| (k, t))
                        .unwrap_or((0, 0.0))
                });
            last = k;
            let (seg, from, to) = segs[k];
            let len = seg.arclen(1e-6);
            let frac = if len > 1e-9 {
                seg.subsegment(0.0..t).arclen(1e-6) / len
            } else {
                0.0
            };
            param_at(from, to, frac)
        })
        .collect()
}

/// Deepest bisection of one span in [`cut_curve`].
const CUT_MAX_DEPTH: u32 = 6;

/// How far a piece's tips may paint from its source's between vertices:
/// width in world units, color in channel steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TipTolerance {
    pub width: f32,
    pub color: f32,
}

/// The piece of the open one-contour `path` between vertex parameters
/// `span` ([`split_tips_at`]), cut in curve parameter space: world geometry
/// with an on-curve vertex at each returned parameter, and those
/// parameters, for [`carry_vertex_style`]. The piece keeps the source's
/// curves and the vertices between the cuts. Blends have zero slope at
/// every vertex and a cut lands where the source's blend is still moving,
/// so a span whose blend between its end tips strays from what the source
/// paints there by more than `tolerance` is bisected. `None` for a closed
/// or compound path or an empty span.
pub fn cut_curve(
    path: &PathData,
    stroke: &Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    span: std::ops::Range<f32>,
    tolerance: TipTolerance,
) -> Option<(BezPath, Vec<f32>)> {
    if path.closed || !path.extra.is_empty() {
        return None;
    }
    let segs = world_segments(path, rect, rotation_deg);
    let end = segs.len() as f32;
    let (from, to) = (span.start.clamp(0.0, end), span.end.clamp(0.0, end));
    if segs.is_empty() || from.partial_cmp(&to) != Some(std::cmp::Ordering::Less) {
        return None;
    }
    let mut params = vec![from];
    params.extend(
        (from.floor() as usize + 1..segs.len())
            .map(|v| v as f32)
            .filter(|v| *v < to),
    );
    params.push(to);
    let hard_ease = tip_ease(&subpath_through(&segs, &params));
    let ease_between = |a: f32, b: f32| {
        if !stroke.paints_as_stamp() {
            hard_ease
        } else if matches!(
            segs.get(((a + b) / 2.0).floor() as usize),
            Some((KSeg::Line(_), ..))
        ) {
            vector_ink::TipEase::Linear
        } else {
            vector_ink::TipEase::Smooth
        }
    };
    let strays = |a: f32, b: f32| {
        const SAMPLES: [f32; 7] = [0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875];
        let mut at = vec![a, b];
        at.extend(SAMPLES.iter().map(|g| a + (b - a) * g));
        let Some(tips) = split_tips_at(path, stroke, rect, rotation_deg, Corner::Square, &at)
        else {
            return false;
        };
        let ease = ease_between(a, b);
        SAMPLES.iter().zip(&tips[2..]).any(|(g, source)| {
            let piece = lerp_span(tips[0], tips[1], ease.weight(*g));
            (piece.width - source.width).abs() > tolerance.width
                || (0..4).any(|c| {
                    (f32::from(piece.color.0[c]) - f32::from(source.color.0[c])).abs()
                        > tolerance.color
                })
        })
    };
    let mut refined = vec![from];
    for w in params.windows(2) {
        bisect(w[0], w[1], 0, &strays, &mut refined);
    }
    Some((subpath_through(&segs, &refined), refined))
}

fn bisect(a: f32, b: f32, depth: u32, strays: &dyn Fn(f32, f32) -> bool, out: &mut Vec<f32>) {
    if depth < CUT_MAX_DEPTH && strays(a, b) {
        let mid = (a + b) / 2.0;
        bisect(a, mid, depth + 1, strays, out);
        bisect(mid, b, depth + 1, strays, out);
    } else {
        out.push(b);
    }
}

/// World path through increasing vertex parameters `params` of the open
/// path whose segments are `segs`, with a vertex at each: consecutive
/// parameters lie on one segment, and the part between them is that
/// segment cut at their arc-length fractions.
fn subpath_through(segs: &[(KSeg, usize, usize)], params: &[f32]) -> BezPath {
    let on = |p: f32, k: usize| {
        let seg = segs[k].0;
        let frac = f64::from((p - k as f32).clamp(0.0, 1.0));
        let len = seg.arclen(1e-6);
        let t = if frac <= 0.0 || frac >= 1.0 || len <= 1e-9 {
            frac
        } else {
            seg.inv_arclen(frac * len, 1e-6)
        };
        (seg, t)
    };
    let mut bez = BezPath::new();
    let Some(&first) = params.first() else {
        return bez;
    };
    let last = segs.len() - 1;
    let (seg, t) = on(first, (first.floor() as usize).min(last));
    bez.move_to(seg.eval(t));
    for w in params.windows(2) {
        let k = (w[0].floor() as usize).min(last);
        let (seg, t0) = on(w[0], k);
        let (_, t1) = on(w[1], k);
        bez.push(seg.subsegment(t0..t1).as_path_el());
    }
    bez
}

/// Arc length at each on-curve vertex of the one-contour `bez`, then its
/// total, a closed contour's implicit closing edge included.
fn vertex_lengths(bez: &BezPath, closed: bool) -> (Vec<f64>, f64) {
    let mut along = Vec::new();
    let (mut last, mut start) = (Point::ZERO, Point::ZERO);
    for el in bez.elements() {
        let len = match *el {
            PathEl::MoveTo(p) => {
                if !along.is_empty() {
                    break;
                }
                (last, start) = (p, p);
                along.push(0.0);
                continue;
            }
            PathEl::ClosePath => break,
            PathEl::LineTo(p) => Line::new(last, p).arclen(1e-6),
            PathEl::QuadTo(c, p) => vector_ink::kurbo::QuadBez::new(last, c, p).arclen(1e-6),
            PathEl::CurveTo(c1, c2, p) => CubicBez::new(last, c1, c2, p).arclen(1e-6),
        };
        last = el.end_point().unwrap_or(last);
        along.push(along.last().copied().unwrap_or(0.0) + len);
    }
    let end = along.last().copied().unwrap_or(0.0);
    let total = if closed {
        end + (start - last).hypot()
    } else {
        end
    };
    (along, total)
}

/// For each vertex of the refit contour `new`, the vertex parameter
/// ([`split_tips_at`], local to the contour) of the spot on `old` at the
/// same fraction of its length: how a smoothing pass carries tips onto the
/// vertices it refits.
pub fn arc_length_params(
    old: &BezPath,
    old_closed: bool,
    new: &BezPath,
    new_closed: bool,
) -> Vec<f32> {
    let (old_along, old_total) = vertex_lengths(old, old_closed);
    let (new_along, new_total) = vertex_lengths(new, new_closed);
    let n = old_along.len();
    if n == 0 {
        return vec![0.0; new_along.len()];
    }
    new_along
        .iter()
        .map(|l| {
            let f = if new_total > 1e-9 { l / new_total } else { 0.0 };
            let at = f * old_total;
            let k = old_along.partition_point(|x| *x <= at).saturating_sub(1);
            let (to, end) = if k + 1 < n {
                (k + 1, old_along[k + 1])
            } else if old_closed {
                (0, old_total)
            } else {
                return (n - 1) as f32;
            };
            let span = end - old_along[k];
            let frac = if span > 1e-9 {
                (at - old_along[k]) / span
            } else {
                1.0
            };
            param_at(k, to, frac.clamp(0.0, 1.0))
        })
        .collect()
}

fn closes_on_start(path: &PathData) -> bool {
    path.closed
        && path.segs.last().is_some_and(|s| {
            let end = seg_end(s);
            (end[0] - path.start[0]).abs() <= 1e-6 && (end[1] - path.start[1]).abs() <= 1e-6
        })
}

fn write_tips(path: &mut PathData, stroke: &mut Stroke, tips: Vec<StrokeSpan>) {
    if tips.iter().all(|t| *t == tips[0]) {
        path.tips.clear();
        stroke.width = tips[0].width;
        stroke.color = tips[0].color;
        return;
    }
    stroke.width = tips.iter().map(|t| t.width).fold(0.0_f32, f32::max);
    if tips.iter().all(|t| t.color == tips[0].color) {
        stroke.color = tips[0].color;
    }
    path.tips = tips;
}

/// Tips at each joint of an arc of `n` spans: straight along the sweep from
/// the start grip to the through grip (joint `n / 2`), then to the end grip.
fn arc_tips(world: &BezPath, n: usize, grips: [StrokeSpan; 3]) -> Vec<StrokeSpan> {
    let mut along = vec![0.0_f64];
    for seg in world.segments() {
        let len = seg.arclen(1e-3);
        along.push(along.last().copied().unwrap_or(0.0) + len);
    }
    along.resize(n + 1, *along.last().unwrap_or(&0.0));
    let mid = n / 2;
    let (first, second) = (along[mid] - along[0], along[n] - along[mid]);
    (0..=n)
        .map(|j| {
            if j <= mid {
                let t = if first > 0.0 { along[j] / first } else { 0.0 };
                lerp_span(grips[0], grips[1], t as f32)
            } else {
                let t = if second > 0.0 {
                    (along[j] - along[mid]) / second
                } else {
                    1.0
                };
                lerp_span(grips[1], grips[2], t as f32)
            }
        })
        .collect()
}

/// Split an odd arc's middle span in two, so the through point is a joint.
fn split_middle_span(path: &mut PathData) {
    let k = path.segs.len() / 2;
    let from = if k == 0 {
        path.start
    } else {
        seg_end(&path.segs[k - 1])
    };
    let PathSeg::Cubic { c1, c2, to } = path.segs[k] else {
        return;
    };
    let p = |q: [f32; 2]| Point::new(q[0] as f64, q[1] as f64);
    let q = |pt: Point| [pt.x as f32, pt.y as f32];
    let (a, b) = CubicBez::new(p(from), p(c1), p(c2), p(to)).subdivide();
    path.segs[k] = PathSeg::Cubic {
        c1: q(a.p1),
        c2: q(a.p2),
        to: q(a.p3),
    };
    path.segs.insert(
        k + 1,
        PathSeg::Cubic {
            c1: q(b.p1),
            c2: q(b.p2),
            to: q(b.p3),
        },
    );
}

fn seg_end(seg: &PathSeg) -> [f32; 2] {
    match *seg {
        PathSeg::Line { to } | PathSeg::Quad { to, .. } | PathSeg::Cubic { to, .. } => to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(closing_copy: bool) -> PathData {
        let line = |to| PathSeg::Line { to };
        let mut segs = vec![line([1.0, 0.0]), line([0.0, 1.0])];
        if closing_copy {
            segs.push(line([0.0, 0.0]));
        }
        PathData {
            start: [0.0, 0.0],
            segs,
            closed: true,
            ..PathData::default()
        }
    }

    #[test]
    fn corner_overrides_follow_their_vertices_across_a_rebuild() {
        let mut old = triangle(true);
        old.corner_amounts = vec![Some(4.0), None, Some(9.0), None];
        let mut rebuilt = triangle(false);
        keep_corner_amounts(&old, &mut rebuilt);
        assert_eq!(rebuilt.corner_amounts, vec![Some(4.0), None, Some(9.0)]);

        let mut again = triangle(true);
        keep_corner_amounts(&rebuilt, &mut again);
        assert_eq!(again.corner_amounts, vec![Some(4.0), None, Some(9.0), None]);

        let mut fewer = PathData {
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            closed: false,
            ..triangle(false)
        };
        keep_corner_amounts(&old, &mut fewer);
        assert!(
            fewer.corner_amounts.is_empty(),
            "a changed vertex count drops them"
        );
    }

    fn tip(width: f32, red: u8) -> StrokeSpan {
        StrokeSpan {
            width,
            softness: 0.0,
            color: Rgba([red, 0, 0, 255]),
            texture: Default::default(),
        }
    }

    fn hard(width: f32) -> Stroke {
        Stroke {
            width,
            ..Stroke::default()
        }
    }

    const UNIT: WorldRect = WorldRect {
        x: 0.0,
        y: 0.0,
        w: 100.0,
        h: 100.0,
    };
    const SQ: Corner = Corner::Square;

    /// A straight polyline (0,0) → (100,0) → (100,100) in `UNIT`.
    fn ell() -> PathData {
        PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            tips: vec![tip(2.0, 0), tip(10.0, 200), tip(4.0, 100)],
            ..PathData::default()
        }
    }

    /// A straight run drawn as one cubic, so it blends by smoothstep.
    fn straight_cubic() -> PathData {
        PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Cubic {
                c1: [0.3, 0.0],
                c2: [0.7, 0.0],
                to: [1.0, 0.0],
            }],
            tips: vec![tip(2.0, 0), tip(10.0, 200)],
            ..PathData::default()
        }
    }

    #[test]
    fn split_tips_follow_the_painters_blend() {
        let at = |path: &PathData, p: f32| {
            split_tips_at(path, &hard(10.0), UNIT, 0.0, SQ, &[p]).unwrap()[0]
        };
        let line = ell();
        assert_eq!(at(&line, 1.0), tip(10.0, 200), "a vertex keeps its own");
        let quarter = at(&line, 0.25);
        assert!((quarter.width - 4.0).abs() < 1e-5, "straight on a polyline");
        assert_eq!(quarter.color.0[0], 50);
        let ease = vector_ink::TipEase::Smooth.weight(0.25);
        let curve = at(&straight_cubic(), 0.25);
        assert!(
            (curve.width - (2.0 + 8.0 * ease)).abs() < 1e-5,
            "smoothstep on a curve"
        );
        assert_eq!(curve.color.0[0], (200.0 * ease).round() as u8);
        assert!(split_tips_at(&PathData::default(), &hard(1.0), UNIT, 0.0, SQ, &[0.0]).is_none());
    }

    #[test]
    fn a_stamped_stroke_splits_straight_on_lines_and_smoothly_on_curves() {
        let brush = Stroke {
            stamp: true,
            ..hard(10.0)
        };
        let at =
            |path: &PathData, p: f32| split_tips_at(path, &brush, UNIT, 0.0, SQ, &[p]).unwrap()[0];
        let mut mixed = straight_cubic();
        mixed.segs.push(PathSeg::Line { to: [1.0, 1.0] });
        mixed.tips.push(tip(4.0, 0));
        let ease = vector_ink::TipEase::Smooth.weight(0.25);
        let curve = at(&mixed, 0.25);
        assert!((curve.width - (2.0 + 8.0 * ease)).abs() < 1e-5, "{curve:?}");
        let line = at(&mixed, 1.25);
        assert!((line.width - 8.5).abs() < 1e-5, "{line:?}");
    }

    /// A filleted polyline paints its tips by smoothstep between its
    /// vertices (P1.curve.tip-chord), so a cut there takes that value.
    #[test]
    fn a_filleted_polyline_splits_by_smoothstep_as_it_paints() {
        let round = Corner::Rounded { radius: 20.0 };
        let at = |corner, p: f32| {
            split_tips_at(&ell(), &hard(10.0), UNIT, 0.0, corner, &[p]).unwrap()[0]
        };
        let ease = vector_ink::TipEase::Smooth.weight(0.25);
        let got = at(round, 0.25);
        assert!((got.width - (2.0 + 8.0 * ease)).abs() < 1e-5, "{got:?}");
        assert_eq!(got.color.0[0], (200.0 * ease).round() as u8);
        assert!((at(Corner::Chamfer { cut: 20.0 }, 0.25).width - 4.0).abs() < 1e-5);
        let painted = crate::geom::tipped_stroke(&ell(), &hard(10.0), UNIT, 0.0, round).unwrap();
        let k = painted
            .bez
            .elements()
            .iter()
            .filter_map(|el| el.end_point())
            .position(|p| (p - Point::new(25.0, 0.0)).hypot() < 1e-6)
            .expect("a painted vertex at the cut");
        assert!(
            (painted.widths[k] - got.width).abs() < 1e-4,
            "cut matches the paint"
        );
    }

    #[test]
    fn placed_tips_fold_opacity_into_alpha_and_equal_tips_stay_plain() {
        let red = Rgba([200, 0, 0, 255]);
        let placed = |width, opacity| PlacedTip {
            width,
            color: red,
            opacity,
        };
        let (opacity, spans) = placed_spans(&hard(4.0), &[placed(2.0, 0.8), placed(6.0, 0.4)]);
        assert_eq!(opacity, 0.8, "the node takes the most opaque tip");
        assert_eq!(spans[0].color.0[3], 255);
        assert_eq!(spans[1].color.0[3], 128, "half the node's opacity");
        let mid = PlacedTip::lerp(placed(2.0, 0.8), placed(6.0, 0.4), 0.5);
        assert!((mid.width - 4.0).abs() < 1e-5 && (mid.opacity - 0.6).abs() < 1e-5);

        let mut path = PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            ..PathData::default()
        };
        let mut stroke = hard(4.0);
        let same = [placed(3.0, 0.5), placed(3.0, 0.5)];
        let opacity = set_grip_placed_tips(&mut path, &mut stroke, UNIT, 0.0, &same);
        assert_eq!(opacity, Some(0.5));
        assert!(path.tips.is_empty(), "equal tips commit a plain stroke");
        assert_eq!((stroke.width, stroke.color), (3.0, red));
    }

    /// Opacity reaches 0 % on every curve tool: tips all placed at 0 % make
    /// a clear node whose tips keep their full relative alpha.
    #[test]
    fn tips_all_placed_at_zero_opacity_make_a_clear_node() {
        let red = Rgba([200, 0, 0, 255]);
        let placed = |width| PlacedTip {
            width,
            color: red,
            opacity: 0.0,
        };
        let (opacity, spans) = placed_spans(&hard(4.0), &[placed(2.0), placed(6.0)]);
        assert_eq!(opacity, 0.0, "the node is as clear as its tips");
        assert!(spans.iter().all(|s| s.color.0[3] == 255), "{spans:?}");

        let mut path = PathData {
            start: [0.0, 0.0],
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            ..PathData::default()
        };
        let mut stroke = hard(4.0);
        let same = [placed(3.0), placed(3.0)];
        let opacity = set_grip_placed_tips(&mut path, &mut stroke, UNIT, 0.0, &same);
        assert_eq!(opacity, Some(0.0));
        assert_eq!(stroke.color, red, "the stroke keeps its own alpha");
    }

    #[test]
    fn split_tips_wrap_a_closed_contour_and_keep_contours_apart() {
        let mut closed = ell();
        closed.closed = true;
        closed.extra.push(crate::scene::PathContour {
            start: [0.0, 0.5],
            segs: vec![PathSeg::Line { to: [0.5, 0.5] }],
            closed: false,
        });
        closed.tips.extend([tip(6.0, 0), tip(8.0, 0)]);
        let got = split_tips_at(&closed, &hard(10.0), UNIT, 0.0, SQ, &[2.5, 3.0, 3.5]).unwrap();
        assert!(
            (got[0].width - 3.0).abs() < 1e-5,
            "closing edge blends to the start"
        );
        assert_eq!(got[1].width, 6.0, "vertex 3 starts the next contour");
        assert!((got[2].width - 7.0).abs() < 1e-5);
    }

    #[test]
    fn located_points_read_back_as_arc_length_parameters() {
        let line = ell();
        let params = locate_vertex_params(
            &line,
            UNIT,
            0.0,
            &[
                [0.0, 0.0],
                [25.0, 0.0],
                [100.0, 0.0],
                [100.0, 50.0],
                [100.0, 100.0],
            ],
            0.1,
        );
        assert_eq!(params, vec![0.0, 0.25, 1.0, 1.5, 2.0]);
        let curve = straight_cubic();
        let params = locate_vertex_params(&curve, UNIT, 0.0, &[[25.0, 0.0]], 0.1);
        assert!(
            (params[0] - 0.25).abs() < 1e-4,
            "arc length, not t: {params:?}"
        );
    }

    #[test]
    fn carried_style_samples_tips_and_keeps_corners_on_old_vertices() {
        let mut old = ell();
        old.corner_amounts = vec![None, Some(6.0), None];
        let mut piece = PathData {
            start: [0.0, 0.0],
            segs: vec![
                PathSeg::Line { to: [1.0, 0.0] },
                PathSeg::Line { to: [1.0, 1.0] },
            ],
            ..PathData::default()
        };
        let mut stroke = hard(10.0);
        carry_vertex_style(
            (&old, UNIT, 0.0, SQ),
            &mut piece,
            &mut stroke,
            &[0.5, 1.0, 1.5],
        );
        let widths: Vec<f32> = piece.tips.iter().map(|t| t.width).collect();
        assert_eq!(widths, vec![6.0, 10.0, 7.0]);
        assert_eq!(stroke.width, 10.0, "the widest painted tip");
        assert_eq!(piece.corner_amounts, vec![None, Some(6.0), None]);

        let mut narrow = PathData {
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            ..PathData::default()
        };
        let mut stroke = hard(10.0);
        carry_vertex_style((&old, UNIT, 0.0, SQ), &mut narrow, &mut stroke, &[0.0, 0.5]);
        assert_eq!(stroke.width, 6.0, "a piece paints at its own widest");
        let painted = narrow.vector_widths(&stroke).unwrap();
        assert_eq!(painted, vec![2.0, 6.0], "and keeps the widths it painted");
        assert!(narrow.corner_amounts.is_empty(), "no override survives");
    }

    #[test]
    fn arc_length_params_map_refit_vertices_by_fraction_of_length() {
        let old = path_data_to_world_bez(&ell(), UNIT, 0.0);
        let mut new = BezPath::new();
        new.move_to((0.0, 0.0));
        new.line_to((50.0, 0.0));
        new.line_to((100.0, 100.0));
        let got = arc_length_params(&old, false, &new, false);
        assert_eq!(got[0], 0.0);
        assert_eq!(got[2], 2.0);
        let f = 50.0 / (50.0 + 50.0_f64.hypot(100.0));
        assert!((got[1] as f64 - f * 2.0).abs() < 1e-4, "{got:?}");
        assert_eq!(
            arc_length_params(&old, false, &old, false),
            vec![0.0, 1.0, 2.0]
        );
    }

    #[test]
    fn cut_curve_keeps_polyline_vertices_and_splits_only_where_blends_stray() {
        let tight = TipTolerance {
            width: 0.05,
            color: 1.0,
        };
        let (bez, params) = cut_curve(&ell(), &hard(10.0), UNIT, 0.0, 0.5..2.0, tight).unwrap();
        assert_eq!(
            params,
            vec![0.5, 1.0, 2.0],
            "straight blends restrict exactly"
        );
        let ends: Vec<Point> = bez
            .elements()
            .iter()
            .filter_map(|e| e.end_point())
            .collect();
        assert_eq!(
            ends,
            vec![
                Point::new(50.0, 0.0),
                Point::new(100.0, 0.0),
                Point::new(100.0, 100.0)
            ]
        );

        let curve = straight_cubic();
        let stroke = hard(10.0);
        let (bez, params) = cut_curve(&curve, &stroke, UNIT, 0.0, 0.25..1.0, tight).unwrap();
        assert_eq!((params[0], *params.last().unwrap()), (0.25, 1.0));
        assert!(params.len() > 2, "a smoothstep cut is bisected: {params:?}");
        assert!(params.windows(2).all(|w| w[0] < w[1]));
        assert!(bez
            .elements()
            .iter()
            .skip(1)
            .all(|e| matches!(e, PathEl::CurveTo(..))));
        let start = bez.elements()[0].end_point().unwrap();
        assert!((start - Point::new(25.0, 0.0)).hypot() < 1e-3, "{start:?}");
        let tips = split_tips_at(&curve, &stroke, UNIT, 0.0, SQ, &params).unwrap();
        for (w, t) in params.windows(2).zip(tips.windows(2)) {
            for g in [0.25_f32, 0.5, 0.75] {
                let want =
                    split_tips_at(&curve, &stroke, UNIT, 0.0, SQ, &[w[0] + (w[1] - w[0]) * g])
                        .unwrap()[0];
                let got = lerp_span(t[0], t[1], vector_ink::TipEase::Smooth.weight(g));
                assert!((got.width - want.width).abs() <= 0.05, "{w:?} at {g}");
            }
        }
        assert!(cut_curve(&curve, &stroke, UNIT, 0.0, 1.0..0.5, tight).is_none());
    }

    #[test]
    fn vertex_styles_round_trip_and_fall_back_to_the_stroke() {
        let mut old = ell();
        old.corner_amounts = vec![None, Some(6.0), None];
        let styles = vertex_styles(Some(&old), &hard(10.0), 3);
        let widths: Vec<f32> = styles.iter().map(|s| s.tip.width).collect();
        assert_eq!(widths, vec![2.0, 10.0, 4.0]);
        assert_eq!(styles[1].corner, Some(6.0));
        let plain = vertex_styles(None, &hard(3.0), 2);
        assert_eq!(plain[0].tip, StrokeSpan::of(&hard(3.0)));
        assert_eq!(plain[1].corner, None);

        let mut joined = ell();
        joined.tips.clear();
        let mut stroke = hard(1.0);
        let reversed: Vec<VertexStyle> = styles.iter().rev().copied().collect();
        apply_vertex_styles(&mut joined, &mut stroke, &reversed);
        assert_eq!(joined.vector_widths(&stroke).unwrap(), vec![4.0, 10.0, 2.0]);
        assert_eq!(stroke.width, 10.0);
        assert_eq!(joined.corner_amounts, vec![None, Some(6.0), None]);

        let mut curve = straight_cubic();
        curve.tips.clear();
        apply_vertex_styles(&mut curve, &mut stroke, &styles[..2]);
        assert!(curve.corner_amounts.is_empty(), "no corners on a curve");
        apply_vertex_styles(&mut curve, &mut stroke, &styles);
        assert_eq!(curve.tips.len(), 2, "a list that does not fit is refused");
    }
}
