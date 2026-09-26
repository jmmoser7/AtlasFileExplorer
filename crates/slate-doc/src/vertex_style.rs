//! Per-vertex stroke style on a curve (P1.curve.vertex-style): which path
//! vertices a picked grip owns, and how an edit at picked grips rewrites the
//! path's tips. Both interpreters paint the result through
//! [`crate::geom::tipped_stroke`].
//!
//! Grips follow the curve's geometry (P1.curve.grips): a circular arc has
//! three (start, through, end) and its tips are derived straight along the
//! sweep from those three; any other single-contour path has one grip per
//! anchor, and the anchor that closes a path owns the duplicate end vertex.

use vector_ink::kurbo::{BezPath, CubicBez, ParamCurve, ParamCurveArclen, Point};

use crate::geom::{arc_grip_points, path_data_to_world_bez};
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
}
