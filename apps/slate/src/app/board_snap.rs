//! Smart guides for the Board canvas — object-to-object alignment and spacing.
//!
//! Defaults follow InDesign / tldraw / Keynote: only objects in the current
//! view, in the same row or column (a "lane"), and not behind a closer
//! neighbor. Hold Alt to suspend. Reach (Tight / Nearby / Wide) is a
//! session preference under Document Settings.

use eframe::egui::{Pos2, Vec2};
use slate_doc::scene::WorldRect;
use slate_doc::NodeId;

/// Snap activates within this many screen pixels (InDesign "snap-to zone" ≈ 6 pt).
pub const SNAP_SCREEN_PX: f32 = 6.0;
/// Board grid spacing in world units (visible dots + optional snap).
pub const GRID_WORLD: f32 = 20.0;
/// Mild rotation snap threshold in degrees (45° and 90° multiples).
pub const ROTATION_SNAP_DEG: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideAxis {
    Vertical,
    Horizontal,
}

/// Who may participate in a smart-guide snap (world units).
#[derive(Clone, Copy, Debug)]
pub struct SnapScope {
    /// InDesign-style snap-to zone (edge must be this close on the snap axis).
    pub threshold: f32,
    /// Max gap on the snap axis. `INFINITY` = no extra neighborhood limit.
    pub reach: f32,
    /// Max gap on the cross axis to share a row (X-align) or column (Y-align).
    pub lane: f32,
    /// Current canvas view in world space (InDesign / tldraw viewport cull).
    pub view: WorldRect,
}

impl SnapScope {
    /// No culling — used by unit tests that only care about the math.
    pub fn open(threshold: f32) -> Self {
        Self {
            threshold,
            reach: f32::INFINITY,
            lane: f32::INFINITY,
            view: WorldRect::new(-1.0e7, -1.0e7, 2.0e7, 2.0e7),
        }
    }
}

/// A temporary alignment line shown while snapping (not persisted).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGuide {
    pub axis: GuideAxis,
    /// x for vertical guides, y for horizontal guides.
    pub pos: f32,
    pub span_start: f32,
    pub span_end: f32,
    /// World coordinate of the impact along the guide (y for vertical, x for
    /// horizontal) — where the moving object meets the snap field.
    pub origin: f32,
}

#[derive(Clone, Copy)]
struct SnapLines {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    cx: f32,
    cy: f32,
}

impl SnapLines {
    fn from_rect(r: WorldRect) -> Self {
        Self {
            left: r.x,
            right: r.x + r.w,
            top: r.y,
            bottom: r.y + r.h,
            cx: r.x + r.w * 0.5,
            cy: r.y + r.h * 0.5,
        }
    }

    fn x_candidates(&self) -> [f32; 3] {
        [self.left, self.cx, self.right]
    }

    fn y_candidates(&self) -> [f32; 3] {
        [self.top, self.cy, self.bottom]
    }
}

fn collect_targets(exclude: &[NodeId], all: &[(NodeId, WorldRect)]) -> Vec<SnapLines> {
    all.iter()
        .filter(|(id, _)| !exclude.contains(id))
        .map(|(_, r)| SnapLines::from_rect(*r))
        .collect()
}

fn gap_1d(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    if a1 < b0 {
        b0 - a1
    } else if b1 < a0 {
        a0 - b1
    } else {
        0.0
    }
}

fn in_view(t: &SnapLines, view: WorldRect, pad: f32) -> bool {
    t.right >= view.x - pad
        && t.left <= view.x + view.w + pad
        && t.bottom >= view.y - pad
        && t.top <= view.y + view.h + pad
}

fn same_lane(moving: &SnapLines, t: &SnapLines, axis: GuideAxis, lane: f32) -> bool {
    match axis {
        GuideAxis::Vertical => gap_1d(moving.top, moving.bottom, t.top, t.bottom) <= lane,
        GuideAxis::Horizontal => gap_1d(moving.left, moving.right, t.left, t.right) <= lane,
    }
}

fn along_gap(moving: &SnapLines, t: &SnapLines, axis: GuideAxis) -> f32 {
    match axis {
        GuideAxis::Vertical => gap_1d(moving.left, moving.right, t.left, t.right),
        GuideAxis::Horizontal => gap_1d(moving.top, moving.bottom, t.top, t.bottom),
    }
}

/// True when another in-lane object sits strictly between `moving` and
/// `target` along the guide (the cross axis). A neighbor in the same
/// column blocks X-alignments beyond it; a neighbor in the same row
/// blocks Y-alignments beyond it.
fn occluded(
    moving: &SnapLines,
    target: &SnapLines,
    others: &[SnapLines],
    axis: GuideAxis,
    lane: f32,
) -> bool {
    let (m0, m1, t0, t1) = match axis {
        GuideAxis::Vertical => (moving.top, moving.bottom, target.top, target.bottom),
        GuideAxis::Horizontal => (moving.left, moving.right, target.left, target.right),
    };
    if (t0 - m0).abs() < 0.01 && (t1 - m1).abs() < 0.01 {
        return false;
    }
    for o in others {
        if std::ptr::eq(o, target) || !same_lane(moving, o, axis, lane) {
            continue;
        }
        // Blocker must share the column (X-align) or row (Y-align) — not
        // a neighbor off to the side.
        let on_guide = match axis {
            GuideAxis::Vertical => gap_1d(moving.left, moving.right, o.left, o.right) <= 0.0,
            GuideAxis::Horizontal => gap_1d(moving.top, moving.bottom, o.top, o.bottom) <= 0.0,
        };
        if !on_guide {
            continue;
        }
        let (o0, o1) = match axis {
            GuideAxis::Vertical => (o.top, o.bottom),
            GuideAxis::Horizontal => (o.left, o.right),
        };
        if t0 >= m1 {
            if o0 >= m1 && o1 <= t0 && o1 < t0 {
                return true;
            }
        } else if t1 <= m0 && o1 <= m0 && o0 >= t1 && o0 > t1 {
            return true;
        }
    }
    false
}

fn eligible(
    moving: &SnapLines,
    t: &SnapLines,
    others: &[SnapLines],
    scope: &SnapScope,
    axis: GuideAxis,
) -> bool {
    if !in_view(t, scope.view, scope.lane) {
        return false;
    }
    if !same_lane(moving, t, axis, scope.lane) {
        return false;
    }
    if along_gap(moving, t, axis) > scope.reach {
        return false;
    }
    !occluded(moving, t, others, axis, scope.lane)
}

fn guide_between(axis: GuideAxis, pos: f32, moving: &SnapLines, target: &SnapLines) -> SnapGuide {
    match axis {
        GuideAxis::Vertical => SnapGuide {
            axis,
            pos,
            span_start: moving.top.min(target.top),
            span_end: moving.bottom.max(target.bottom),
            origin: (moving.top + moving.bottom) * 0.5,
        },
        GuideAxis::Horizontal => SnapGuide {
            axis,
            pos,
            span_start: moving.left.min(target.left),
            span_end: moving.right.max(target.right),
            origin: (moving.left + moving.right) * 0.5,
        },
    }
}

fn best_axis_snap(
    moving: &SnapLines,
    targets: &[SnapLines],
    scope: &SnapScope,
    axis: GuideAxis,
) -> (f32, Option<SnapGuide>) {
    let moving_lines = match axis {
        GuideAxis::Vertical => moving.x_candidates(),
        GuideAxis::Horizontal => moving.y_candidates(),
    };
    let mut best_delta = 0.0f32;
    let mut best_dist = scope.threshold;
    let mut best_guide = None;

    for t in targets {
        if !eligible(moving, t, targets, scope, axis) {
            continue;
        }
        let t_lines = match axis {
            GuideAxis::Vertical => t.x_candidates(),
            GuideAxis::Horizontal => t.y_candidates(),
        };
        for &mp in &moving_lines {
            for &tp in &t_lines {
                let delta = tp - mp;
                let dist = delta.abs();
                if dist < best_dist {
                    best_dist = dist;
                    best_delta = delta;
                    best_guide = Some(guide_between(axis, tp, moving, t));
                }
            }
        }
    }
    (best_delta, best_guide)
}

/// Snap a proposed bounding box to nearby object edges/centers.
pub fn snap_bbox(
    proposed: WorldRect,
    exclude: &[NodeId],
    all: &[(NodeId, WorldRect)],
    threshold: f32,
) -> (WorldRect, Vec<SnapGuide>) {
    snap_bbox_scoped(proposed, exclude, all, SnapScope::open(threshold))
}

/// [`snap_bbox`] with viewport / lane / reach culling.
pub fn snap_bbox_scoped(
    proposed: WorldRect,
    exclude: &[NodeId],
    all: &[(NodeId, WorldRect)],
    scope: SnapScope,
) -> (WorldRect, Vec<SnapGuide>) {
    let trects = collect_targets(exclude, all);
    if trects.is_empty() {
        return (proposed, Vec::new());
    }

    let m = SnapLines::from_rect(proposed);
    let mut guides = Vec::new();
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;

    let (x_delta, x_guide) = best_axis_snap(&m, &trects, &scope, GuideAxis::Vertical);
    if let Some(g) = x_guide {
        guides.push(g);
        dx = x_delta;
    }

    let shifted = WorldRect::new(proposed.x + dx, proposed.y, proposed.w, proposed.h);
    let m2 = SnapLines::from_rect(shifted);
    let (y_delta, y_guide) = best_axis_snap(&m2, &trects, &scope, GuideAxis::Horizontal);
    if let Some(g) = y_guide {
        guides.push(g);
        dy = y_delta;
    }

    let mut snapped = WorldRect::new(proposed.x + dx, proposed.y + dy, proposed.w, proposed.h);

    if let Some((sdx, sdy, spacing_guides)) = snap_equal_spacing(&snapped, &trects, &scope) {
        snapped.x += sdx;
        snapped.y += sdy;
        guides.extend(spacing_guides);
    }

    (snapped, guides)
}

/// Align a free point (curve end, corner handle) to nearby object edges.
pub fn snap_point(
    p: Pos2,
    exclude: &[NodeId],
    all: &[(NodeId, WorldRect)],
    scope: SnapScope,
) -> (Pos2, Vec<SnapGuide>) {
    let proposed = WorldRect::new(p.x, p.y, 0.0, 0.0);
    let (snapped, guides) = snap_bbox_scoped(proposed, exclude, all, scope);
    (Pos2::new(snapped.x, snapped.y), guides)
}

/// When the moving box sits between two others, snap so gaps match.
fn snap_equal_spacing(
    moving: &WorldRect,
    statics: &[SnapLines],
    scope: &SnapScope,
) -> Option<(f32, f32, Vec<SnapGuide>)> {
    let m = SnapLines::from_rect(*moving);
    let mut guides = Vec::new();
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    let mut found = false;

    let row: Vec<&SnapLines> = statics
        .iter()
        .filter(|t| eligible(&m, t, statics, scope, GuideAxis::Vertical))
        .collect();
    for a in &row {
        for c in &row {
            if a.right >= c.left || m.left <= a.right || m.right >= c.left {
                continue;
            }
            let gap_left = m.left - a.right;
            let gap_right = c.left - m.right;
            if gap_left <= 0.0 || gap_right <= 0.0 {
                continue;
            }
            let diff = gap_left - gap_right;
            if diff.abs() < scope.threshold {
                dx = -diff * 0.5;
                guides.push(SnapGuide {
                    axis: GuideAxis::Horizontal,
                    pos: m.cy,
                    span_start: a.right,
                    span_end: c.left,
                    origin: m.cx,
                });
                found = true;
                break;
            }
        }
    }

    let m2 = SnapLines::from_rect(WorldRect::new(moving.x + dx, moving.y, moving.w, moving.h));
    let col: Vec<&SnapLines> = statics
        .iter()
        .filter(|t| eligible(&m2, t, statics, scope, GuideAxis::Horizontal))
        .collect();
    for a in &col {
        for c in &col {
            if a.bottom >= c.top || m2.top <= a.bottom || m2.bottom >= c.top {
                continue;
            }
            let gap_top = m2.top - a.bottom;
            let gap_bottom = c.top - m2.bottom;
            if gap_top <= 0.0 || gap_bottom <= 0.0 {
                continue;
            }
            let diff = gap_top - gap_bottom;
            if diff.abs() < scope.threshold {
                dy = -diff * 0.5;
                guides.push(SnapGuide {
                    axis: GuideAxis::Vertical,
                    pos: m2.cx,
                    span_start: a.bottom,
                    span_end: c.top,
                    origin: m2.cy,
                });
                found = true;
                break;
            }
        }
    }

    found.then_some((dx, dy, guides))
}

/// Which edges of a resize are free to snap (anchor edges are fixed).
#[derive(Clone, Copy)]
pub struct ResizeSnapEdges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl ResizeSnapEdges {
    /// Edges of a DragScale rect the cursor owns. The press corner stays put
    /// so a growing rectangle snaps like a resize, not like a 0-size point
    /// that leaves the target's row and goes silent.
    pub fn for_draw(start: Pos2, rect: WorldRect) -> Self {
        let slop = 0.5;
        Self {
            left: (start.x - rect.x).abs() > slop,
            right: (start.x - (rect.x + rect.w)).abs() > slop,
            top: (start.y - rect.y).abs() > slop,
            bottom: (start.y - (rect.y + rect.h)).abs() > slop,
        }
    }

    pub fn for_handle(handle: u8) -> Self {
        match handle {
            0 => Self {
                left: true,
                right: false,
                top: true,
                bottom: false,
            },
            1 => Self {
                left: false,
                right: false,
                top: true,
                bottom: false,
            },
            2 => Self {
                left: false,
                right: true,
                top: true,
                bottom: false,
            },
            3 => Self {
                left: false,
                right: true,
                top: false,
                bottom: false,
            },
            4 => Self {
                left: false,
                right: true,
                top: false,
                bottom: true,
            },
            5 => Self {
                left: false,
                right: false,
                top: false,
                bottom: true,
            },
            6 => Self {
                left: true,
                right: false,
                top: false,
                bottom: true,
            },
            _ => Self {
                left: true,
                right: false,
                top: false,
                bottom: false,
            },
        }
    }
}

/// Snap only the moving edges of a resize rect.
pub fn snap_resize_rect(
    proposed: WorldRect,
    exclude: &[NodeId],
    all: &[(NodeId, WorldRect)],
    threshold: f32,
    edges: ResizeSnapEdges,
) -> (WorldRect, Vec<SnapGuide>) {
    snap_resize_rect_scoped(proposed, exclude, all, SnapScope::open(threshold), edges)
}

/// [`snap_resize_rect`] with viewport / lane / reach culling.
pub fn snap_resize_rect_scoped(
    proposed: WorldRect,
    exclude: &[NodeId],
    all: &[(NodeId, WorldRect)],
    scope: SnapScope,
    edges: ResizeSnapEdges,
) -> (WorldRect, Vec<SnapGuide>) {
    let trects = collect_targets(exclude, all);
    if trects.is_empty() {
        return (proposed, Vec::new());
    }

    let mut r = proposed;
    let mut guides = Vec::new();
    let moving = SnapLines::from_rect(r);

    if edges.left {
        if let Some((d, t)) = nearest_edge_from(&moving, r.x, &trects, &scope, GuideAxis::Vertical)
        {
            r.x += d;
            r.w -= d;
            guides.push(guide_between(
                GuideAxis::Vertical,
                r.x,
                &SnapLines::from_rect(r),
                &t,
            ));
        }
    }
    if edges.right {
        let right = r.x + r.w;
        let moving = SnapLines::from_rect(r);
        if let Some((d, t)) =
            nearest_edge_from(&moving, right, &trects, &scope, GuideAxis::Vertical)
        {
            r.w += d;
            guides.push(guide_between(
                GuideAxis::Vertical,
                r.x + r.w,
                &SnapLines::from_rect(r),
                &t,
            ));
        }
    }
    if edges.top {
        let moving = SnapLines::from_rect(r);
        if let Some((d, t)) =
            nearest_edge_from(&moving, r.y, &trects, &scope, GuideAxis::Horizontal)
        {
            r.y += d;
            r.h -= d;
            guides.push(guide_between(
                GuideAxis::Horizontal,
                r.y,
                &SnapLines::from_rect(r),
                &t,
            ));
        }
    }
    if edges.bottom {
        let bottom = r.y + r.h;
        let moving = SnapLines::from_rect(r);
        if let Some((d, t)) =
            nearest_edge_from(&moving, bottom, &trects, &scope, GuideAxis::Horizontal)
        {
            r.h += d;
            guides.push(guide_between(
                GuideAxis::Horizontal,
                r.y + r.h,
                &SnapLines::from_rect(r),
                &t,
            ));
        }
    }

    (r, guides)
}

/// Opposite corner of `rect` from `start` — the DragScale end after an
/// edge snap, so `place_rect(start, end)` reconstructs the snapped box.
pub fn draw_end_from_rect(start: Pos2, rect: WorldRect) -> Pos2 {
    let x = if (start.x - rect.x).abs() <= 0.5 {
        rect.x + rect.w
    } else {
        rect.x
    };
    let y = if (start.y - rect.y).abs() <= 0.5 {
        rect.y + rect.h
    } else {
        rect.y
    };
    Pos2::new(x, y)
}

fn nearest_edge_from(
    moving: &SnapLines,
    val: f32,
    targets: &[SnapLines],
    scope: &SnapScope,
    axis: GuideAxis,
) -> Option<(f32, SnapLines)> {
    let mut best: Option<(f32, f32, SnapLines)> = None;
    for t in targets {
        if !eligible(moving, t, targets, scope, axis) {
            continue;
        }
        let lines = match axis {
            GuideAxis::Vertical => t.x_candidates(),
            GuideAxis::Horizontal => t.y_candidates(),
        };
        for &tp in &lines {
            let delta = tp - val;
            let dist = delta.abs();
            if dist < scope.threshold && best.as_ref().map(|b| dist < b.1).unwrap_or(true) {
                best = Some((delta, dist, *t));
            }
        }
    }
    best.map(|(d, _, t)| (d, t))
}

/// Map a member's origin through a group-box scale, keeping width and height.
/// Used by Ctrl+Alt+Shift group-grip: the selection AABB changes, items only
/// translate. Callers must then [`pin_group_union`] so the opposite handle
/// of the stack stays put — scaling origins alone walks that corner on
/// Nw / Ne / Sw (the member that defined it still has the same size).
pub fn remap_group_keep_size(rect: WorldRect, sx: f32, sy: f32, anchor: (f32, f32)) -> WorldRect {
    WorldRect::new(
        anchor.0 + (rect.x - anchor.0) * sx,
        anchor.1 + (rect.y - anchor.1) * sy,
        rect.w,
        rect.h,
    )
}

/// Scale a member's rect through a group-box scale (size and origin).
pub fn remap_group_scale(rect: WorldRect, sx: f32, sy: f32, anchor: (f32, f32)) -> WorldRect {
    WorldRect::new(
        anchor.0 + (rect.x - anchor.0) * sx,
        anchor.1 + (rect.y - anchor.1) * sy,
        rect.w * sx,
        rect.h * sy,
    )
}

/// Translate remapped members so the union's opposite handle matches the
/// original group box. Without this, keep-size remap from Nw / Ne / Sw
/// slides the whole stack because member extents do not scale.
pub fn pin_group_union(
    rects: &mut [WorldRect],
    original: WorldRect,
    handle: u8,
    from_center: bool,
) {
    let Some(union) = union_rect(rects) else {
        return;
    };
    let old = resize_anchor(original, handle, from_center);
    let new = resize_anchor(union, handle, from_center);
    let dx = old.0 - new.0;
    let dy = old.1 - new.1;
    if dx.abs() < 1e-5 && dy.abs() < 1e-5 {
        return;
    }
    for r in rects {
        r.x += dx;
        r.y += dy;
    }
}

/// Apply a group-box scale to every member, then pin the union's opposite
/// handle. `keep_size` is the Ctrl+Alt+Shift mode.
pub fn apply_group_box_scale(
    rects: &[WorldRect],
    gb: WorldRect,
    sx: f32,
    sy: f32,
    handle: u8,
    from_center: bool,
    keep_size: bool,
) -> Vec<WorldRect> {
    let anchor = resize_anchor(gb, handle, from_center);
    let mut out: Vec<WorldRect> = rects
        .iter()
        .map(|r| {
            if keep_size {
                remap_group_keep_size(*r, sx, sy, anchor)
            } else {
                remap_group_scale(*r, sx, sy, anchor)
            }
        })
        .collect();
    pin_group_union(&mut out, gb, handle, from_center);
    out
}

/// Union bounding box of several rects.
pub fn union_rect(rects: &[WorldRect]) -> Option<WorldRect> {
    let mut iter = rects.iter();
    let first = *iter.next()?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.w;
    let mut max_y = first.y + first.h;
    for r in iter {
        min_x = min_x.min(r.x);
        min_y = min_y.min(r.y);
        max_x = max_x.max(r.x + r.w);
        max_y = max_y.max(r.y + r.h);
    }
    Some(WorldRect::new(min_x, min_y, max_x - min_x, max_y - min_y))
}

/// Snap a rect's origin to the board grid (size unchanged).
pub fn snap_rect_origin(mut r: WorldRect, enabled: bool) -> WorldRect {
    if !enabled {
        return r;
    }
    r.x = (r.x / GRID_WORLD).round() * GRID_WORLD;
    r.y = (r.y / GRID_WORLD).round() * GRID_WORLD;
    r
}

/// Snap rotation to the nearest 45° when within `threshold_deg`.
pub fn snap_rotation_deg(deg: f32, threshold_deg: f32) -> f32 {
    let snapped = (deg / 45.0).round() * 45.0;
    let mut delta = deg - snapped;
    while delta > 180.0 {
        delta -= 360.0;
    }
    while delta < -180.0 {
        delta += 360.0;
    }
    if delta.abs() <= threshold_deg {
        snapped
    } else {
        deg
    }
}

fn pointer_local(pointer: Pos2, rect: WorldRect, rotation_deg: f32) -> Pos2 {
    if rotation_deg.abs() < f32::EPSILON {
        return pointer;
    }
    let (cx, cy) = rect.center();
    let rad = (-rotation_deg).to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = pointer.x - cx;
    let dy = pointer.y - cy;
    Pos2::new(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
}

/// Local-space point of a resize handle (0–7: Nw N Ne E Se S Sw W).
pub fn handle_local(rect: WorldRect, handle: u8) -> (f32, f32) {
    let (cx, cy) = rect.center();
    let (left, right) = (rect.x, rect.x + rect.w);
    let (top, bottom) = (rect.y, rect.y + rect.h);
    match handle {
        0 => (left, top),
        1 => (cx, top),
        2 => (right, top),
        3 => (right, cy),
        4 => (right, bottom),
        5 => (cx, bottom),
        6 => (left, bottom),
        _ => (left, cy),
    }
}

/// Opposite handle — the world-space point that must stay put during a
/// non-center resize, and the origin of a group-box scale.
pub fn resize_anchor(rect: WorldRect, handle: u8, from_center: bool) -> (f32, f32) {
    if from_center {
        return rect.center();
    }
    handle_local(rect, handle.wrapping_add(4) % 8)
}

/// After a local-space resize, translate so the opposite handle stays put
/// in world space. Rotation is about the live rect center: keeping only the
/// local AABB edge fixed walks the far world edge (most visible at 180°).
fn pin_rotated_resize(
    before: WorldRect,
    mut r: WorldRect,
    handle: u8,
    rotation_deg: f32,
) -> WorldRect {
    if rotation_deg.abs() < f32::EPSILON {
        return r;
    }
    let old_world = orbit_point(
        before.center(),
        resize_anchor(before, handle, false),
        rotation_deg,
    );
    let new_world = orbit_point(r.center(), resize_anchor(r, handle, false), rotation_deg);
    r.x += old_world.0 - new_world.0;
    r.y += old_world.1 - new_world.1;
    r
}

/// Resize from a handle (0–7: corners then edge midpoints). Operates in the
/// node's local axes when `rotation_deg` is non-zero, then pins the opposite
/// handle in world space so the grabbed edge is the one that moves. Shift
/// locks aspect ratio; Ctrl resizes from center (PowerPoint / Office).
pub fn resize_from_handle(
    before: WorldRect,
    pointer: Pos2,
    handle: u8,
    min_size: f32,
    lock_aspect: bool,
    from_center: bool,
    rotation_deg: f32,
) -> WorldRect {
    let local = pointer_local(pointer, before, rotation_deg);
    let aspect = (before.w / before.h.max(0.001)).max(0.001);

    if from_center {
        let cx = before.x + before.w * 0.5;
        let cy = before.y + before.h * 0.5;
        let mut half_w = (local.x - cx).abs().max(min_size * 0.5);
        let mut half_h = (local.y - cy).abs().max(min_size * 0.5);
        if lock_aspect {
            if half_w / aspect > half_h {
                half_h = half_w / aspect;
            } else {
                half_w = half_h * aspect;
            }
        }
        return WorldRect::new(cx - half_w, cy - half_h, half_w * 2.0, half_h * 2.0);
    }

    let is_corner = matches!(handle, 0 | 2 | 4 | 6);

    let r = match handle {
        0 => {
            let ax = before.x + before.w;
            let ay = before.y + before.h;
            let mut w = (ax - local.x).max(min_size);
            let mut h = (ay - local.y).max(min_size);
            if lock_aspect && is_corner {
                if w / aspect > h {
                    h = w / aspect;
                } else {
                    w = h * aspect;
                }
            }
            WorldRect::new(ax - w, ay - h, w, h)
        }
        1 => {
            let bottom = before.y + before.h;
            let h = (bottom - local.y).max(min_size);
            if lock_aspect {
                let w = (h * aspect).max(min_size);
                WorldRect::new(before.x + (before.w - w) * 0.5, bottom - h, w, h)
            } else {
                WorldRect::new(before.x, bottom - h, before.w, h)
            }
        }
        2 => {
            let ax = before.x;
            let ay = before.y + before.h;
            let mut w = (local.x - ax).max(min_size);
            let mut h = (ay - local.y).max(min_size);
            if lock_aspect && is_corner {
                if w / aspect > h {
                    h = w / aspect;
                } else {
                    w = h * aspect;
                }
            }
            WorldRect::new(ax, ay - h, w, h)
        }
        3 => {
            let w = (local.x - before.x).max(min_size);
            if lock_aspect {
                let h = (w / aspect).max(min_size);
                WorldRect::new(before.x, before.y + (before.h - h) * 0.5, w, h)
            } else {
                WorldRect::new(before.x, before.y, w, before.h)
            }
        }
        4 => {
            let ax = before.x;
            let ay = before.y;
            let mut w = (local.x - ax).max(min_size);
            let mut h = (local.y - ay).max(min_size);
            if lock_aspect && is_corner {
                if w / aspect > h {
                    h = w / aspect;
                } else {
                    w = h * aspect;
                }
            }
            WorldRect::new(ax, ay, w, h)
        }
        5 => {
            let h = (local.y - before.y).max(min_size);
            if lock_aspect {
                let w = (h * aspect).max(min_size);
                WorldRect::new(before.x + (before.w - w) * 0.5, before.y, w, h)
            } else {
                WorldRect::new(before.x, before.y, before.w, h)
            }
        }
        6 => {
            let ax = before.x + before.w;
            let ay = before.y;
            let mut w = (ax - local.x).max(min_size);
            let mut h = (local.y - ay).max(min_size);
            if lock_aspect && is_corner {
                if w / aspect > h {
                    h = w / aspect;
                } else {
                    w = h * aspect;
                }
            }
            WorldRect::new(ax - w, ay, w, h)
        }
        _ => {
            let ax = before.x + before.w;
            let w = (ax - local.x).max(min_size);
            if lock_aspect {
                let h = (w / aspect).max(min_size);
                WorldRect::new(ax - w, before.y + (before.h - h) * 0.5, w, h)
            } else {
                WorldRect::new(ax - w, before.y, w, before.h)
            }
        }
    };
    pin_rotated_resize(before, r, handle, rotation_deg)
}

/// Rotate `p` about `center` by `delta_deg` (clockwise in y-down world
/// space — same convention as `WorldRect::corners_rotated`).
pub fn orbit_point(center: (f32, f32), p: (f32, f32), delta_deg: f32) -> (f32, f32) {
    let rad = delta_deg.to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = p.0 - center.0;
    let dy = p.1 - center.1;
    (
        center.0 + dx * cos - dy * sin,
        center.1 + dx * sin + dy * cos,
    )
}

pub(crate) fn segments_intersect(
    a1: (f32, f32),
    a2: (f32, f32),
    b1: (f32, f32),
    b2: (f32, f32),
) -> bool {
    fn orient(p: (f32, f32), q: (f32, f32), r: (f32, f32)) -> f32 {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    }
    let d1 = orient(b1, b2, a1);
    let d2 = orient(b1, b2, a2);
    let d3 = orient(a1, a2, b1);
    let d4 = orient(a1, a2, b2);
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

/// Does an axis-aligned marquee rect intersect a node's *rotated* rect?
/// Corner-in-rect, rect-corner-in-polygon, or any edge crossing counts.
pub fn marquee_intersects_rotated(marquee: WorldRect, rect: WorldRect, rotation_deg: f32) -> bool {
    let corners = rect.corners_rotated(rotation_deg);
    if corners.iter().any(|(x, y)| marquee.contains(*x, *y)) {
        return true;
    }
    let mc = [
        (marquee.x, marquee.y),
        (marquee.x + marquee.w, marquee.y),
        (marquee.x + marquee.w, marquee.y + marquee.h),
        (marquee.x, marquee.y + marquee.h),
    ];
    if mc
        .iter()
        .any(|(x, y)| rect.contains_rotated(*x, *y, rotation_deg))
    {
        return true;
    }
    for i in 0..4 {
        let a1 = corners[i];
        let a2 = corners[(i + 1) % 4];
        for j in 0..4 {
            if segments_intersect(a1, a2, mc[j], mc[(j + 1) % 4]) {
                return true;
            }
        }
    }
    false
}

// ---------- ortho (F8 + one-shot Shift) — constraints spec §1 ----------

/// The one constraint-layer predicate: Shift *inverts* the persistent F8
/// ortho state while held (Rhino semantics).
pub fn effective_ortho(ortho: bool, shift_down: bool) -> bool {
    ortho ^ shift_down
}

/// The unit 45°-step axis nearest to `d` (undefined input → +x).
pub fn ortho_axis(d: Vec2) -> Vec2 {
    if d.length_sq() <= f32::EPSILON {
        return Vec2::new(1.0, 0.0);
    }
    let step = std::f32::consts::FRAC_PI_4;
    let snapped = (d.y.atan2(d.x) / step).round() * step;
    Vec2::new(snapped.cos(), snapped.sin())
}

/// Snap a drag vector onto the nearest 45°-step axis: the result points
/// along the axis with magnitude = the projection of `d` onto it.
pub fn ortho_snap_vec(d: Vec2) -> Vec2 {
    if d.length_sq() <= f32::EPSILON {
        return d;
    }
    let axis = ortho_axis(d);
    axis * d.dot(axis)
}

/// Snap a point onto a 45° ray from `origin` (draft segments, wire drags).
pub fn ortho_snap_point(origin: Pos2, p: Pos2) -> Pos2 {
    origin + ortho_snap_vec(p - origin)
}

/// Shift-constrain a rubber-band draw rect (square for shapes/frames).
pub fn constrain_draw_rect(raw: WorldRect, tool_square: bool, shift: bool) -> WorldRect {
    if !shift || !tool_square {
        return raw.normalized();
    }
    let r = raw.normalized();
    let side = r.w.max(r.h);
    WorldRect::new(r.x, r.y, side, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_aligns_left_edges() {
        let all = vec![
            (NodeId(1), WorldRect::new(100.0, 50.0, 80.0, 60.0)),
            (NodeId(2), WorldRect::new(200.0, 200.0, 40.0, 40.0)),
        ];
        let proposed = WorldRect::new(103.0, 10.0, 50.0, 50.0);
        let (snapped, guides) = snap_bbox(proposed, &[NodeId(2)], &all, 6.0);
        assert!((snapped.x - 100.0).abs() < 0.01, "x={}", snapped.x);
        assert!(!guides.is_empty());
    }

    #[test]
    fn resize_locks_aspect_with_shift() {
        let before = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        let r = resize_from_handle(before, Pos2::new(300.0, 50.0), 4, 8.0, true, false, 0.0);
        assert!((r.w / r.h - 2.0).abs() < 0.05, "w={} h={}", r.w, r.h);
    }

    /// Every corner: opposite handle stays, aspect holds, grabbed corner moves.
    #[test]
    fn every_corner_uniform_scale_pins_the_opposite() {
        let before = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        let corners = [0u8, 2, 4, 6];
        let pointers = [
            Pos2::new(-50.0, -25.0), // Nw further out
            Pos2::new(250.0, -25.0), // Ne
            Pos2::new(250.0, 125.0), // Se
            Pos2::new(-50.0, 125.0), // Sw
        ];
        for (handle, pointer) in corners.into_iter().zip(pointers) {
            let r = resize_from_handle(before, pointer, handle, 8.0, true, false, 0.0);
            let old_a = resize_anchor(before, handle, false);
            let new_a = resize_anchor(r, handle, false);
            assert!(
                (old_a.0 - new_a.0).abs() < 0.05 && (old_a.1 - new_a.1).abs() < 0.05,
                "handle {handle}: opposite walked {old_a:?} → {new_a:?}"
            );
            assert!(
                (r.w / r.h - 2.0).abs() < 0.05,
                "handle {handle}: aspect broke w={} h={}",
                r.w,
                r.h
            );
            let old_m = handle_local(before, handle);
            let new_m = handle_local(r, handle);
            let moved = (new_m.0 - old_m.0).abs() + (new_m.1 - old_m.1).abs();
            assert!(moved > 1.0, "handle {handle}: grabbed corner did not move");
        }
    }

    /// Ctrl+Alt+Shift (keep size): every corner pins the union's opposite
    /// handle. Without the pin, Nw / Ne / Sw slide the whole stack.
    #[test]
    fn every_corner_keep_size_pins_the_stack() {
        let members = [
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            WorldRect::new(120.0, 0.0, 80.0, 60.0),
        ];
        let gb = union_rect(&members).unwrap();
        for handle in [0u8, 2, 4, 6] {
            let out = apply_group_box_scale(&members, gb, 2.0, 2.0, handle, false, true);
            assert!(
                out.iter()
                    .zip(members.iter())
                    .all(|(a, b)| (a.w - b.w).abs() < 1e-4 && (a.h - b.h).abs() < 1e-4),
                "handle {handle}: member size changed"
            );
            let union = union_rect(&out).unwrap();
            let old_a = resize_anchor(gb, handle, false);
            let new_a = resize_anchor(union, handle, false);
            assert!(
                (old_a.0 - new_a.0).abs() < 0.05 && (old_a.1 - new_a.1).abs() < 0.05,
                "handle {handle}: stack translated {old_a:?} → {new_a:?}"
            );
            let spread0 = members[1].x - members[0].x;
            let spread1 = out[1].x - out[0].x;
            assert!(
                (spread1 - spread0 * 2.0).abs() < 0.1,
                "handle {handle}: layout did not scale ({spread0} → {spread1})"
            );
        }
    }

    #[test]
    fn every_corner_group_scale_pins_and_scales() {
        let members = [
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            WorldRect::new(120.0, 0.0, 80.0, 60.0),
        ];
        let gb = union_rect(&members).unwrap();
        for handle in [0u8, 2, 4, 6] {
            let out = apply_group_box_scale(&members, gb, 2.0, 2.0, handle, false, false);
            assert!(
                out.iter()
                    .all(|r| (r.w - 160.0).abs() < 1e-3 && (r.h - 120.0).abs() < 1e-3),
                "handle {handle}: members did not scale"
            );
            let union = union_rect(&out).unwrap();
            let old_a = resize_anchor(gb, handle, false);
            let new_a = resize_anchor(union, handle, false);
            assert!(
                (old_a.0 - new_a.0).abs() < 0.05 && (old_a.1 - new_a.1).abs() < 0.05,
                "handle {handle}: opposite walked {old_a:?} → {new_a:?}"
            );
        }
    }

    /// Documents the defect `pin_group_union` exists to fix: remapping
    /// origins from Nw / Ne / Sw walks the supposed-fixed corner.
    #[test]
    fn keep_size_without_pin_walks_nw_ne_sw() {
        let members = [
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            WorldRect::new(120.0, 0.0, 80.0, 60.0),
        ];
        let gb = union_rect(&members).unwrap();
        for handle in [0u8, 2, 6] {
            let anchor = resize_anchor(gb, handle, false);
            let remapped: Vec<WorldRect> = members
                .iter()
                .map(|r| remap_group_keep_size(*r, 2.0, 2.0, anchor))
                .collect();
            let union = union_rect(&remapped).unwrap();
            let old_a = resize_anchor(gb, handle, false);
            let new_a = resize_anchor(union, handle, false);
            let walked = (old_a.0 - new_a.0).abs() + (old_a.1 - new_a.1).abs();
            assert!(
                walked > 1.0,
                "handle {handle}: expected the unpinned stack to walk, got {old_a:?} → {new_a:?}"
            );
        }
    }

    #[test]
    fn every_edge_keep_size_pins_the_stack() {
        let members = [
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            WorldRect::new(120.0, 0.0, 80.0, 60.0),
        ];
        let gb = union_rect(&members).unwrap();
        // Horizontal edges scale X; vertical edges scale Y.
        let cases = [(1u8, 1.0, 2.0), (3, 2.0, 1.0), (5, 1.0, 2.0), (7, 2.0, 1.0)];
        for (handle, sx, sy) in cases {
            let out = apply_group_box_scale(&members, gb, sx, sy, handle, false, true);
            assert!(
                out.iter()
                    .zip(members.iter())
                    .all(|(a, b)| (a.w - b.w).abs() < 1e-4 && (a.h - b.h).abs() < 1e-4),
                "handle {handle}: member size changed"
            );
            let union = union_rect(&out).unwrap();
            let old_a = resize_anchor(gb, handle, false);
            let new_a = resize_anchor(union, handle, false);
            assert!(
                (old_a.0 - new_a.0).abs() < 0.05 && (old_a.1 - new_a.1).abs() < 0.05,
                "handle {handle}: stack translated {old_a:?} → {new_a:?}"
            );
        }
    }

    #[test]
    fn every_corner_center_scale_keeps_the_center() {
        let before = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        let (cx, cy) = before.center();
        let pointers = [
            Pos2::new(-50.0, -25.0),
            Pos2::new(250.0, -25.0),
            Pos2::new(250.0, 125.0),
            Pos2::new(-50.0, 125.0),
        ];
        for (handle, pointer) in [0u8, 2, 4, 6].into_iter().zip(pointers) {
            let r = resize_from_handle(before, pointer, handle, 8.0, true, true, 0.0);
            let (nx, ny) = r.center();
            assert!(
                (nx - cx).abs() < 0.05 && (ny - cy).abs() < 0.05,
                "handle {handle}: center walked ({cx},{cy}) → ({nx},{ny})"
            );
            assert!(
                (r.w / r.h - 2.0).abs() < 0.05,
                "handle {handle}: aspect broke w={} h={}",
                r.w,
                r.h
            );
        }
    }

    /// Shift on a corner is free: a one-axis drag must not lock the other.
    #[test]
    fn every_corner_free_scale_is_independent() {
        let before = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        // Each pointer moves only the grabbed corner's X (or Y for Ne/Nw
        // we move X and keep the top/bottom).
        let cases = [
            (0u8, Pos2::new(-50.0, 0.0), 250.0, 100.0),
            (2, Pos2::new(250.0, 0.0), 250.0, 100.0),
            (4, Pos2::new(250.0, 100.0), 250.0, 100.0),
            (6, Pos2::new(-50.0, 100.0), 250.0, 100.0),
        ];
        for (handle, pointer, exp_w, exp_h) in cases {
            let r = resize_from_handle(before, pointer, handle, 8.0, false, false, 0.0);
            let old_a = resize_anchor(before, handle, false);
            let new_a = resize_anchor(r, handle, false);
            assert!(
                (old_a.0 - new_a.0).abs() < 0.05 && (old_a.1 - new_a.1).abs() < 0.05,
                "handle {handle}: opposite walked {old_a:?} → {new_a:?}"
            );
            assert!(
                (r.w - exp_w).abs() < 0.05 && (r.h - exp_h).abs() < 0.05,
                "handle {handle}: expected {exp_w}×{exp_h}, got {}×{}",
                r.w,
                r.h
            );
        }
    }

    #[test]
    fn group_center_scale_keeps_union_center() {
        let members = [
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            WorldRect::new(120.0, 40.0, 80.0, 60.0),
        ];
        let gb = union_rect(&members).unwrap();
        let (cx, cy) = gb.center();
        for handle in [0u8, 2, 4, 6] {
            let out = apply_group_box_scale(&members, gb, 2.0, 2.0, handle, true, false);
            let union = union_rect(&out).unwrap();
            let (nx, ny) = union.center();
            assert!(
                (nx - cx).abs() < 0.05 && (ny - cy).abs() < 0.05,
                "handle {handle}: union center walked ({cx},{cy}) → ({nx},{ny})"
            );
        }
    }

    #[test]
    fn every_corner_group_free_scale_pins() {
        let members = [
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            WorldRect::new(120.0, 40.0, 80.0, 60.0),
        ];
        let gb = union_rect(&members).unwrap();
        for handle in [0u8, 2, 4, 6] {
            let out = apply_group_box_scale(&members, gb, 2.0, 1.0, handle, false, false);
            assert!(
                out.iter()
                    .all(|r| (r.w - 160.0).abs() < 1e-3 && (r.h - 60.0).abs() < 1e-3),
                "handle {handle}: expected 2× width, 1× height"
            );
            let union = union_rect(&out).unwrap();
            let old_a = resize_anchor(gb, handle, false);
            let new_a = resize_anchor(union, handle, false);
            assert!(
                (old_a.0 - new_a.0).abs() < 0.05 && (old_a.1 - new_a.1).abs() < 0.05,
                "handle {handle}: opposite walked {old_a:?} → {new_a:?}"
            );
        }
    }

    /// Uniform corner scale from a mostly-horizontal drag still grows both
    /// axes — no corner is allowed to lock to one direction.
    #[test]
    fn every_corner_uniform_grows_both_axes() {
        let before = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        let pointers = [
            Pos2::new(-100.0, 0.0),
            Pos2::new(300.0, 0.0),
            Pos2::new(300.0, 100.0),
            Pos2::new(-100.0, 100.0),
        ];
        for (handle, pointer) in [0u8, 2, 4, 6].into_iter().zip(pointers) {
            let r = resize_from_handle(before, pointer, handle, 8.0, true, false, 0.0);
            assert!(
                r.w > before.w + 1.0 && r.h > before.h + 1.0,
                "handle {handle}: locked to one axis ({}×{})",
                r.w,
                r.h
            );
            assert!((r.w / r.h - 2.0).abs() < 0.05);
        }
    }

    fn world_aabb(rect: WorldRect, rot: f32) -> (f32, f32, f32, f32) {
        let cs = rect.corners_rotated(rot);
        let xs = cs.map(|c| c.0);
        let ys = cs.map(|c| c.1);
        (
            xs.into_iter().fold(f32::INFINITY, f32::min),
            xs.into_iter().fold(f32::NEG_INFINITY, f32::max),
            ys.into_iter().fold(f32::INFINITY, f32::min),
            ys.into_iter().fold(f32::NEG_INFINITY, f32::max),
        )
    }

    #[test]
    fn rotated_180_edge_resize_moves_the_grabbed_world_edge() {
        let before = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let rot = 180.0;
        let (_, _, top0, bot0) = world_aabb(before, rot);
        // After 180° the visual top is local S (handle 5). Drag it further up.
        let r = resize_from_handle(
            before,
            Pos2::new(50.0, top0 - 20.0),
            5,
            8.0,
            false,
            false,
            rot,
        );
        let (_, _, top1, bot1) = world_aabb(r, rot);
        assert!(
            (bot1 - bot0).abs() < 0.05,
            "opposite (visual bottom) walked: {bot0} → {bot1}"
        );
        assert!(
            (top1 - (top0 - 20.0)).abs() < 0.05,
            "grabbed visual top should follow the pointer: {top0} → {top1}"
        );
    }

    #[test]
    fn rotated_90_edge_resize_moves_the_grabbed_world_edge() {
        let before = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let rot = 90.0;
        let (left0, right0, _, _) = world_aabb(before, rot);
        // After 90° CW the visual right is local N (handle 1).
        let r = resize_from_handle(
            before,
            Pos2::new(right0 + 20.0, 50.0),
            1,
            8.0,
            false,
            false,
            rot,
        );
        let (left1, right1, _, _) = world_aabb(r, rot);
        assert!(
            (left1 - left0).abs() < 0.05,
            "opposite (visual left) walked: {left0} → {left1}"
        );
        assert!(
            (right1 - (right0 + 20.0)).abs() < 0.05,
            "grabbed visual right should follow the pointer: {right0} → {right1}"
        );
    }

    #[test]
    fn rotation_snaps_to_45() {
        assert!((snap_rotation_deg(44.0, ROTATION_SNAP_DEG) - 45.0).abs() < 0.01);
        assert!((snap_rotation_deg(10.0, ROTATION_SNAP_DEG) - 10.0).abs() < 0.01);
    }

    #[test]
    fn orbit_rotates_about_center() {
        let (x, y) = orbit_point((0.0, 0.0), (10.0, 0.0), 90.0);
        assert!((x - 0.0).abs() < 1e-4, "x={x}");
        assert!((y - 10.0).abs() < 1e-4, "y={y}");
        // Full turn is the identity.
        let (x, y) = orbit_point((5.0, 5.0), (8.0, 2.0), 360.0);
        assert!((x - 8.0).abs() < 1e-3 && (y - 2.0).abs() < 1e-3);
    }

    #[test]
    fn marquee_hits_rotated_node() {
        // A tall thin node rotated 90° lies horizontally: a marquee over its
        // extended end must hit it even though the stored rect misses.
        let node = WorldRect::new(45.0, 0.0, 10.0, 100.0); // center (50, 50)
        let marquee = WorldRect::new(80.0, 40.0, 40.0, 20.0);
        assert!(!marquee_intersects_rotated(marquee, node, 0.0));
        assert!(marquee_intersects_rotated(marquee, node, 90.0));
    }

    #[test]
    fn effective_ortho_truth_table() {
        assert!(!effective_ortho(false, false));
        assert!(effective_ortho(false, true)); // one-shot Shift
        assert!(effective_ortho(true, false)); // persistent F8
        assert!(!effective_ortho(true, true)); // Shift inverts (Rhino)
    }

    #[test]
    fn ortho_snaps_to_45_degree_steps() {
        // Near-horizontal snaps to the x axis, keeping the projection.
        let v = ortho_snap_vec(Vec2::new(10.0, 1.0));
        assert!((v.y).abs() < 1e-5 && (v.x - 10.0).abs() < 0.2, "{v:?}");
        // Exact diagonal is a fixed point.
        let v = ortho_snap_vec(Vec2::new(5.0, 5.0));
        assert!((v.x - 5.0).abs() < 1e-4 && (v.y - 5.0).abs() < 1e-4);
        // Near-vertical snaps to the y axis.
        let v = ortho_snap_vec(Vec2::new(-1.0, 10.0));
        assert!((v.x).abs() < 1e-5 && (v.y - 10.0).abs() < 0.2, "{v:?}");
        // Point form pivots about the origin.
        let p = ortho_snap_point(Pos2::new(100.0, 100.0), Pos2::new(112.0, 101.0));
        assert!((p.y - 100.0).abs() < 1e-4);
    }

    #[test]
    fn draw_shift_makes_square() {
        let raw = WorldRect::new(0.0, 0.0, 120.0, 40.0);
        let r = constrain_draw_rect(raw, true, true);
        assert!((r.w - r.h).abs() < 0.01);
        assert!((r.w - 120.0).abs() < 0.01);
    }

    #[test]
    fn remap_group_keep_size_translates_without_scaling() {
        let left = WorldRect::new(0.0, 0.0, 80.0, 60.0);
        let right = WorldRect::new(120.0, 0.0, 80.0, 60.0);
        // Grow the group 200 → 300 from the left edge (sx = 1.5).
        let a = remap_group_keep_size(left, 1.5, 1.0, (0.0, 30.0));
        let b = remap_group_keep_size(right, 1.5, 1.0, (0.0, 30.0));
        assert!((a.w - 80.0).abs() < 1e-4 && (a.h - 60.0).abs() < 1e-4);
        assert!((b.w - 80.0).abs() < 1e-4 && (b.h - 60.0).abs() < 1e-4);
        assert!((a.x - 0.0).abs() < 1e-4);
        assert!((b.x - 180.0).abs() < 1e-4);
    }

    fn nearby_scope(threshold: f32) -> SnapScope {
        SnapScope {
            threshold,
            reach: 360.0,
            lane: 140.0,
            view: WorldRect::new(-200.0, -200.0, 2000.0, 2000.0),
        }
    }

    #[test]
    fn far_row_does_not_align() {
        let all = vec![
            (NodeId(1), WorldRect::new(100.0, 800.0, 80.0, 40.0)),
            (NodeId(2), WorldRect::new(200.0, 10.0, 40.0, 40.0)),
        ];
        let proposed = WorldRect::new(103.0, 10.0, 50.0, 50.0);
        let (snapped, guides) = snap_bbox_scoped(proposed, &[NodeId(2)], &all, nearby_scope(6.0));
        assert!(
            (snapped.x - proposed.x).abs() < 0.01,
            "must not snap to an object in another row, x={}",
            snapped.x
        );
        assert!(guides.is_empty());
    }

    #[test]
    fn occluded_target_is_ignored() {
        // Same column: a neighbor sits between the mover and a far box
        // whose left edge is 3 px off. Lane is wide enough to see both;
        // occlusion must refuse the far alignment.
        let scope = SnapScope {
            threshold: 6.0,
            reach: 1000.0,
            lane: 500.0,
            view: WorldRect::new(-200.0, -200.0, 2000.0, 2000.0),
        };
        let all = vec![
            (NodeId(1), WorldRect::new(110.0, 80.0, 40.0, 40.0)),
            (NodeId(2), WorldRect::new(103.0, 400.0, 40.0, 40.0)),
            (NodeId(3), WorldRect::new(100.0, 0.0, 40.0, 40.0)),
        ];
        let proposed = WorldRect::new(100.0, 0.0, 40.0, 40.0);
        let (snapped, _) = snap_bbox_scoped(proposed, &[NodeId(3)], &all, scope);
        assert!(
            (snapped.x - 100.0).abs() < 0.01,
            "must not snap through the closer neighbor, x={}",
            snapped.x
        );
    }

    #[test]
    fn snap_point_aligns_to_nearby_edge() {
        let all = vec![(NodeId(1), WorldRect::new(100.0, 40.0, 80.0, 40.0))];
        let (p, guides) = snap_point(Pos2::new(103.0, 55.0), &[], &all, nearby_scope(6.0));
        assert!((p.x - 100.0).abs() < 0.01, "x={}", p.x);
        assert!(
            guides.iter().any(|g| g.axis == GuideAxis::Vertical),
            "expected a vertical alignment guide"
        );
    }

    #[test]
    fn resize_right_edge_snaps() {
        let all = vec![(NodeId(1), WorldRect::new(200.0, 0.0, 40.0, 40.0))];
        let proposed = WorldRect::new(0.0, 0.0, 196.0, 40.0);
        let (r, guides) = snap_resize_rect(
            proposed,
            &[],
            &all,
            6.0,
            ResizeSnapEdges {
                left: false,
                right: true,
                top: false,
                bottom: false,
            },
        );
        assert!((r.w - 200.0).abs() < 0.01, "w={}", r.w);
        assert!(!guides.is_empty());
    }

    #[test]
    fn draw_second_corner_snaps_live_rect_not_cursor_point() {
        // Target occupies y=0..40. The cursor is at y=280 — snap_point
        // treats that as another row and stays quiet. The growing rect
        // still overlaps the target's row, so a resize-style snap fires.
        let all = vec![(NodeId(1), WorldRect::new(200.0, 0.0, 80.0, 40.0))];
        let start = Pos2::new(0.0, 0.0);
        let cursor = Pos2::new(197.0, 280.0);
        let proposed = WorldRect::new(start.x, start.y, cursor.x - start.x, cursor.y - start.y);
        let (_p, point_guides) = snap_point(cursor, &[], &all, nearby_scope(6.0));
        assert!(
            point_guides.is_empty(),
            "a 0-size cursor below the row must not be the draw snap"
        );
        let (r, guides) = snap_resize_rect_scoped(
            proposed,
            &[],
            &all,
            nearby_scope(6.0),
            ResizeSnapEdges::for_draw(start, proposed),
        );
        assert!(
            (r.x + r.w - 200.0).abs() < 0.01,
            "live right edge should snap to 200, got {}",
            r.x + r.w
        );
        assert!(!guides.is_empty());
        let end = draw_end_from_rect(start, r);
        assert!((end.x - 200.0).abs() < 0.01);
    }
}
