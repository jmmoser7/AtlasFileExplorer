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
use crate::scene::{PathData, PathSeg, Rgba, Stroke, StrokeSpan, WorldRect};

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

/// Give each grip of a just-drawn curve the width its point was placed
/// with, in grip order, keeping the stroke's other style. Equal widths set
/// the stroke width alone. `false` when `widths` does not fit the curve.
pub fn set_grip_widths(
    path: &mut PathData,
    stroke: &mut Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    widths: &[f32],
) -> bool {
    let Some(&first) = widths.first() else {
        return false;
    };
    if widths.iter().all(|w| *w == first) {
        path.tips.clear();
        stroke.width = first;
        return true;
    }
    let grips: Vec<StrokeSpan> = widths
        .iter()
        .map(|&width| StrokeSpan {
            width,
            ..StrokeSpan::of(stroke)
        })
        .collect();
    set_grip_tips(path, stroke, rect, rotation_deg, &grips)
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
            })
            .collect(),
    )
}

/// The tips `path` paints at vertex parameters `at`, for pieces cut from it
/// or geometry refit over it. A vertex parameter is the vertex a segment
/// leaves plus the arc-length fraction along that segment; a closed
/// contour's implicit closing edge leaves its last vertex and ends on its
/// first. Between vertices the tip blends as the painters do: a hard stroke
/// by [`tip_ease`] (straight on polylines, lines and arcs, smoothstep on
/// other curves), a stamped stroke straight. `None` when `path` paints one
/// tip everywhere.
pub fn split_tips_at(
    path: &PathData,
    stroke: &Stroke,
    rect: WorldRect,
    rotation_deg: f32,
    at: &[f32],
) -> Option<Vec<StrokeSpan>> {
    let tips = vertex_tips(path, stroke)?;
    let ease = if stroke.paints_as_stamp() {
        vector_ink::TipEase::Linear
    } else {
        tip_ease(&path_data_to_world_bez(path, rect, rotation_deg))
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
                lerp_span(tips[i], tips[next], ease.weight(f))
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
pub fn carry_vertex_style(
    old: (&PathData, WorldRect, f32),
    new: &mut PathData,
    stroke: &mut Stroke,
    params: &[f32],
) {
    let (old_path, rect, rotation_deg) = old;
    if params.len() != vertex_count(new) {
        return;
    }
    if let Some(tips) = split_tips_at(old_path, stroke, rect, rotation_deg, params) {
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
        let at =
            |path: &PathData, p: f32| split_tips_at(path, &hard(10.0), UNIT, 0.0, &[p]).unwrap()[0];
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
        assert!(split_tips_at(&PathData::default(), &hard(1.0), UNIT, 0.0, &[0.0]).is_none());
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
        let got = split_tips_at(&closed, &hard(10.0), UNIT, 0.0, &[2.5, 3.0, 3.5]).unwrap();
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
        carry_vertex_style((&old, UNIT, 0.0), &mut piece, &mut stroke, &[0.5, 1.0, 1.5]);
        let widths: Vec<f32> = piece.tips.iter().map(|t| t.width).collect();
        assert_eq!(widths, vec![6.0, 10.0, 7.0]);
        assert_eq!(stroke.width, 10.0, "the widest painted tip");
        assert_eq!(piece.corner_amounts, vec![None, Some(6.0), None]);

        let mut narrow = PathData {
            segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
            ..PathData::default()
        };
        let mut stroke = hard(10.0);
        carry_vertex_style((&old, UNIT, 0.0), &mut narrow, &mut stroke, &[0.0, 0.5]);
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
}
