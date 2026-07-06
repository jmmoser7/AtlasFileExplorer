//! Smart guides for the Board canvas — object-to-object alignment and spacing.
//!
//! Defaults mirror professional tools (PowerPoint Smart Guides, Miro Align
//! objects, InDesign Smart Guides): edge/center alignment and equal spacing
//! are on by default; hold Ctrl while dragging to temporarily disable snapping.

use eframe::egui::Pos2;
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

/// A temporary alignment line shown while snapping (not persisted).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapGuide {
    pub axis: GuideAxis,
    /// x for vertical guides, y for horizontal guides.
    pub pos: f32,
    pub span_start: f32,
    pub span_end: f32,
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

/// Collect snap targets from every node except those being manipulated.
fn target_lines(
    exclude: &[NodeId],
    all: &[(NodeId, WorldRect)],
) -> (Vec<f32>, Vec<f32>, Vec<SnapLines>) {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut rects = Vec::new();
    for (id, r) in all {
        if exclude.contains(id) {
            continue;
        }
        let s = SnapLines::from_rect(*r);
        xs.extend_from_slice(&s.x_candidates());
        ys.extend_from_slice(&s.y_candidates());
        rects.push(s);
    }
    (xs, ys, rects)
}

fn best_axis_snap(
    moving: &[f32; 3],
    targets: &[f32],
    threshold: f32,
    axis: GuideAxis,
    moving_span: (f32, f32),
    target_span: (f32, f32),
) -> (f32, Option<SnapGuide>) {
    let mut best_delta = 0.0f32;
    let mut best_dist = threshold;
    let mut best_guide = None;

    for &mp in moving {
        for &tp in targets {
            let delta = tp - mp;
            let dist = delta.abs();
            if dist < best_dist {
                best_dist = dist;
                best_delta = delta;
                let span = (
                    moving_span.0.min(target_span.0),
                    moving_span.1.max(target_span.1),
                );
                best_guide = Some(SnapGuide {
                    axis,
                    pos: tp,
                    span_start: span.0,
                    span_end: span.1,
                });
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
    let (tx, ty, trects) = target_lines(exclude, all);
    if tx.is_empty() && ty.is_empty() {
        return (proposed, Vec::new());
    }

    let m = SnapLines::from_rect(proposed);
    let mut guides = Vec::new();
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;

    let m_x_span = (m.top, m.bottom);

    // Pick the closest X alignment among left/center/right ↔ targets.
    let (x_delta, x_guide) = best_axis_snap(
        &m.x_candidates(),
        &tx,
        threshold,
        GuideAxis::Vertical,
        m_x_span,
        m_x_span,
    );
    if let Some(g) = x_guide {
        // Extend guide span across the matched target rect when possible.
        let mut span = (g.span_start, g.span_end);
        for t in &trects {
            if t.x_candidates().iter().any(|&x| (x - g.pos).abs() < 0.01) {
                span.0 = span.0.min(t.top);
                span.1 = span.1.max(t.bottom);
            }
        }
        guides.push(SnapGuide {
            span_start: span.0,
            span_end: span.1,
            ..g
        });
        dx = x_delta;
    }

    let shifted = WorldRect::new(proposed.x + dx, proposed.y, proposed.w, proposed.h);
    let m2 = SnapLines::from_rect(shifted);
    let (y_delta, y_guide) = best_axis_snap(
        &m2.y_candidates(),
        &ty,
        threshold,
        GuideAxis::Horizontal,
        (m2.left, m2.right),
        (m2.left, m2.right),
    );
    if let Some(g) = y_guide {
        let mut span = (g.span_start, g.span_end);
        for t in &trects {
            if t.y_candidates().iter().any(|&y| (y - g.pos).abs() < 0.01) {
                span.0 = span.0.min(t.left);
                span.1 = span.1.max(t.right);
            }
        }
        guides.push(SnapGuide {
            span_start: span.0,
            span_end: span.1,
            ..g
        });
        dy = y_delta;
    }

    let mut snapped = WorldRect::new(proposed.x + dx, proposed.y + dy, proposed.w, proposed.h);

    // Equal spacing (InDesign Smart Spacing / PowerPoint distribute hint).
    if let Some((sdx, sdy, spacing_guides)) = snap_equal_spacing(&snapped, &trects, threshold) {
        snapped.x += sdx;
        snapped.y += sdy;
        guides.extend(spacing_guides);
    }

    (snapped, guides)
}

/// When the moving box sits between two others, snap so gaps match.
fn snap_equal_spacing(
    moving: &WorldRect,
    statics: &[SnapLines],
    threshold: f32,
) -> Option<(f32, f32, Vec<SnapGuide>)> {
    let m = SnapLines::from_rect(*moving);
    let mut guides = Vec::new();
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    let mut found = false;

    for a in statics {
        for c in statics {
            if a.right >= c.left || m.left <= a.right || m.right >= c.left {
                continue;
            }
            let gap_left = m.left - a.right;
            let gap_right = c.left - m.right;
            if gap_left <= 0.0 || gap_right <= 0.0 {
                continue;
            }
            let diff = gap_left - gap_right;
            if diff.abs() < threshold {
                dx = -diff * 0.5;
                guides.push(SnapGuide {
                    axis: GuideAxis::Horizontal,
                    pos: m.cy,
                    span_start: a.right,
                    span_end: c.left,
                });
                found = true;
                break;
            }
        }
    }

    let m2 = SnapLines::from_rect(WorldRect::new(moving.x + dx, moving.y, moving.w, moving.h));
    for a in statics {
        for c in statics {
            if a.bottom >= c.top || m2.top <= a.bottom || m2.bottom >= c.top {
                continue;
            }
            let gap_top = m2.top - a.bottom;
            let gap_bottom = c.top - m2.bottom;
            if gap_top <= 0.0 || gap_bottom <= 0.0 {
                continue;
            }
            let diff = gap_top - gap_bottom;
            if diff.abs() < threshold {
                dy = -diff * 0.5;
                guides.push(SnapGuide {
                    axis: GuideAxis::Vertical,
                    pos: m2.cx,
                    span_start: a.bottom,
                    span_end: c.top,
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
    let (tx, ty, trects) = target_lines(exclude, all);
    if tx.is_empty() && ty.is_empty() {
        return (proposed, Vec::new());
    }

    let mut r = proposed;
    let mut guides = Vec::new();

    if edges.left {
        let (d, tp) = nearest_edge_snap(r.x, &tx, threshold);
        if d != 0.0 {
            r.x += d;
            r.w -= d;
            if let Some(pos) = tp {
                guides.push(vertical_guide(pos, r.y, r.y + r.h, &trects));
            }
        }
    }
    if edges.right {
        let right = r.x + r.w;
        let (d, tp) = nearest_edge_snap(right, &tx, threshold);
        if d != 0.0 {
            r.w += d;
            if let Some(pos) = tp {
                guides.push(vertical_guide(pos, r.y, r.y + r.h, &trects));
            }
        }
    }
    if edges.top {
        let (d, tp) = nearest_edge_snap(r.y, &ty, threshold);
        if d != 0.0 {
            r.y += d;
            r.h -= d;
            if let Some(pos) = tp {
                guides.push(horizontal_guide(pos, r.x, r.x + r.w, &trects));
            }
        }
    }
    if edges.bottom {
        let bottom = r.y + r.h;
        let (d, tp) = nearest_edge_snap(bottom, &ty, threshold);
        if d != 0.0 {
            r.h += d;
            if let Some(pos) = tp {
                guides.push(horizontal_guide(pos, r.x, r.x + r.w, &trects));
            }
        }
    }

    (r, guides)
}

fn nearest_edge_snap(val: f32, targets: &[f32], threshold: f32) -> (f32, Option<f32>) {
    let mut best = (0.0f32, threshold, None);
    for &tp in targets {
        let delta = tp - val;
        if delta.abs() < best.1 {
            best = (delta, delta.abs(), Some(tp));
        }
    }
    (best.0, best.2)
}

fn vertical_guide(pos: f32, span_start: f32, span_end: f32, trects: &[SnapLines]) -> SnapGuide {
    let mut span = (span_start, span_end);
    for t in trects {
        if t.x_candidates().iter().any(|&x| (x - pos).abs() < 0.01) {
            span.0 = span.0.min(t.top);
            span.1 = span.1.max(t.bottom);
        }
    }
    SnapGuide {
        axis: GuideAxis::Vertical,
        pos,
        span_start: span.0,
        span_end: span.1,
    }
}

fn horizontal_guide(pos: f32, span_start: f32, span_end: f32, trects: &[SnapLines]) -> SnapGuide {
    let mut span = (span_start, span_end);
    for t in trects {
        if t.y_candidates().iter().any(|&y| (y - pos).abs() < 0.01) {
            span.0 = span.0.min(t.left);
            span.1 = span.1.max(t.right);
        }
    }
    SnapGuide {
        axis: GuideAxis::Horizontal,
        pos,
        span_start: span.0,
        span_end: span.1,
    }
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

/// Snap a point to the board grid.
pub fn snap_point_to_grid(p: Pos2, enabled: bool) -> Pos2 {
    if !enabled {
        return p;
    }
    Pos2::new(
        (p.x / GRID_WORLD).round() * GRID_WORLD,
        (p.y / GRID_WORLD).round() * GRID_WORLD,
    )
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

/// Resize from a handle (0–7: corners then edge midpoints). Operates in the
/// node's local axes when `rotation_deg` is non-zero. Shift locks aspect ratio;
/// Ctrl resizes from center (PowerPoint / Office convention).
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
    let mut r = before;

    match handle {
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
            r = WorldRect::new(ax - w, ay - h, w, h);
        }
        1 => {
            let bottom = before.y + before.h;
            let mut h = (bottom - local.y).max(min_size);
            let mut w = before.w;
            if lock_aspect {
                w = (h * aspect).max(min_size);
                r = WorldRect::new(before.x + (before.w - w) * 0.5, bottom - h, w, h);
            } else {
                r = WorldRect::new(before.x, bottom - h, before.w, h);
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
            r = WorldRect::new(ax, ay - h, w, h);
        }
        3 => {
            let mut w = (local.x - before.x).max(min_size);
            let mut h = before.h;
            if lock_aspect {
                h = (w / aspect).max(min_size);
                r = WorldRect::new(before.x, before.y + (before.h - h) * 0.5, w, h);
            } else {
                r = WorldRect::new(before.x, before.y, w, before.h);
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
            r = WorldRect::new(ax, ay, w, h);
        }
        5 => {
            let mut h = (local.y - before.y).max(min_size);
            let mut w = before.w;
            if lock_aspect {
                w = (h * aspect).max(min_size);
                r = WorldRect::new(before.x + (before.w - w) * 0.5, before.y, w, h);
            } else {
                r = WorldRect::new(before.x, before.y, before.w, h);
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
            r = WorldRect::new(ax - w, ay, w, h);
        }
        _ => {
            let ax = before.x + before.w;
            let mut w = (ax - local.x).max(min_size);
            let mut h = before.h;
            if lock_aspect {
                h = (w / aspect).max(min_size);
                r = WorldRect::new(ax - w, before.y + (before.h - h) * 0.5, w, h);
            } else {
                r = WorldRect::new(ax - w, before.y, w, before.h);
            }
        }
    }

    r
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

fn segments_intersect(a1: (f32, f32), a2: (f32, f32), b1: (f32, f32), b2: (f32, f32)) -> bool {
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
    fn draw_shift_makes_square() {
        let raw = WorldRect::new(0.0, 0.0, 120.0, 40.0);
        let r = constrain_draw_rect(raw, true, true);
        assert!((r.w - r.h).abs() < 0.01);
        assert!((r.w - 120.0).abs() < 0.01);
    }
}
