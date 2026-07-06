//! Pure math for the InDesign-style direct-manipulation crop mode.
//!
//! The node rect is the *frame* (the visible crop window in world space);
//! `Crop` is the normalized UV window into the source image. The "content
//! rect" is the world rect the full uncropped image would occupy if the crop
//! were reset — dragging a frame edge moves the mask over fixed content,
//! dragging inside slides the content under a fixed mask.
//!
//! Rotated nodes: all functions here operate in the node's **local
//! (unrotated) space**, where the rect x/y/w/h math is identical to the
//! unrotated case. Callers transform the pointer into local coordinates
//! about the gesture-start rect center (see [`to_local`] / [`delta_local`]),
//! matching how `board_snap::resize_from_handle` handles rotation.

use slate_doc::scene::{Crop, WorldRect};

/// Minimum crop-window size in world units (matches `MIN_DRAW` in board.rs).
pub const MIN_CROP_WORLD: f32 = 8.0;

/// The world rect the full uncropped image occupies, derived from the node
/// rect (the crop window) and its UV crop.
pub fn content_rect(rect: WorldRect, crop: Crop) -> WorldRect {
    let c = crop.clamped();
    let w = rect.w / c.w.max(1e-4);
    let h = rect.h / c.h.max(1e-4);
    WorldRect::new(rect.x - c.x * w, rect.y - c.y * h, w, h)
}

/// The UV crop that shows exactly `window` out of `content` (both world
/// rects; `window` is assumed to lie inside `content`).
pub fn crop_from_rects(window: WorldRect, content: WorldRect) -> Crop {
    let cw = content.w.max(1e-4);
    let ch = content.h.max(1e-4);
    Crop {
        x: (window.x - content.x) / cw,
        y: (window.y - content.y) / ch,
        w: window.w / cw,
        h: window.h / ch,
    }
    .clamped()
}

/// Transform a world-space point into the node's local (unrotated) axes
/// about `(cx, cy)`.
pub fn to_local(px: f32, py: f32, cx: f32, cy: f32, rotation_deg: f32) -> (f32, f32) {
    if rotation_deg.abs() < f32::EPSILON {
        return (px, py);
    }
    let rad = (-rotation_deg).to_radians();
    let (sin, cos) = rad.sin_cos();
    let dx = px - cx;
    let dy = py - cy;
    (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
}

/// Rotate a world-space delta into the node's local axes.
pub fn delta_local(dx: f32, dy: f32, rotation_deg: f32) -> (f32, f32) {
    if rotation_deg.abs() < f32::EPSILON {
        return (dx, dy);
    }
    let rad = (-rotation_deg).to_radians();
    let (sin, cos) = rad.sin_cos();
    (dx * cos - dy * sin, dx * sin + dy * cos)
}

/// Drag a crop-window edge/corner (`handle` 0–7: Nw N Ne E Se S Sw W, same
/// order as `board_handles::ResizeHandle`) to the pointer at `local`
/// (already in the node's local axes). The content stays fixed: the node
/// rect and the crop change together so only the mask moves. The window is
/// clamped inside the content rect and above a minimum size.
pub fn edge_drag(rect: WorldRect, crop: Crop, handle: u8, local: (f32, f32)) -> (WorldRect, Crop) {
    let content = content_rect(rect, crop);
    let (px, py) = local;
    // Keep the crop UV representable (`Crop::clamped` floors w/h at 0.05)
    // and never let a pre-existing tiny window block the gesture.
    let min_w = MIN_CROP_WORLD.min(rect.w).max(0.05 * content.w);
    let min_h = MIN_CROP_WORLD.min(rect.h).max(0.05 * content.h);

    let mut r = rect;
    let moves_left = matches!(handle, 0 | 6 | 7);
    let moves_right = matches!(handle, 2..=4);
    let moves_top = matches!(handle, 0..=2);
    let moves_bottom = matches!(handle, 4..=6);

    if moves_left {
        let right = rect.x + rect.w;
        let x = px.clamp(content.x, right - min_w);
        r.x = x;
        r.w = right - x;
    }
    if moves_right {
        let right = px.clamp(rect.x + min_w, content.x + content.w);
        r.w = right - rect.x;
    }
    if moves_top {
        let bottom = rect.y + rect.h;
        let y = py.clamp(content.y, bottom - min_h);
        r.y = y;
        r.h = bottom - y;
    }
    if moves_bottom {
        let bottom = py.clamp(rect.y + min_h, content.y + content.h);
        r.h = bottom - rect.y;
    }

    (r, crop_from_rects(r, content))
}

/// Slide the content under a fixed crop window: the content follows the
/// pointer, so the UV offset moves *opposite* to the drag. `delta` is the
/// pointer travel since gesture start, in the node's local axes; `rect` and
/// `crop` are the gesture-start window and crop.
pub fn pan_drag(rect: WorldRect, crop: Crop, delta: (f32, f32)) -> Crop {
    let content = content_rect(rect, crop);
    let c = crop.clamped();
    Crop {
        x: (c.x - delta.0 / content.w.max(1e-4)).clamp(0.0, 1.0 - c.w),
        y: (c.y - delta.1 / content.h.max(1e-4)).clamp(0.0, 1.0 - c.h),
        w: c.w,
        h: c.h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn approx_rect(a: WorldRect, b: WorldRect) -> bool {
        approx(a.x, b.x) && approx(a.y, b.y) && approx(a.w, b.w) && approx(a.h, b.h)
    }

    #[test]
    fn content_rect_of_full_crop_is_the_node_rect() {
        let r = WorldRect::new(10.0, 20.0, 200.0, 100.0);
        assert!(approx_rect(content_rect(r, Crop::full()), r));
    }

    #[test]
    fn content_rect_inverts_crop() {
        // Window shows the center-right quarter of the source.
        let r = WorldRect::new(100.0, 50.0, 100.0, 60.0);
        let c = Crop {
            x: 0.5,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        let content = content_rect(r, c);
        assert!(approx_rect(
            content,
            WorldRect::new(0.0, 20.0, 200.0, 120.0)
        ));
        // Round trip: deriving the crop back from the rects is the identity.
        let back = crop_from_rects(r, content);
        assert!(approx(back.x, c.x) && approx(back.y, c.y));
        assert!(approx(back.w, c.w) && approx(back.h, c.h));
    }

    #[test]
    fn edge_drag_left_moves_mask_not_content() {
        // Full crop, 200x100 window; drag the W edge 50 units inward.
        let r = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        let (nr, nc) = edge_drag(r, Crop::full(), 7, (50.0, 50.0));
        assert!(approx_rect(nr, WorldRect::new(50.0, 0.0, 150.0, 100.0)));
        // Content stayed put: crop now starts 25% into the source.
        assert!(approx(nc.x, 0.25) && approx(nc.w, 0.75));
        assert!(approx(nc.y, 0.0) && approx(nc.h, 1.0));
        // And the implied content rect is unchanged.
        assert!(approx_rect(content_rect(nr, nc), r));
    }

    #[test]
    fn edge_drag_corner_adjusts_both_axes() {
        let r = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        // Se corner dragged to (150, 80).
        let (nr, nc) = edge_drag(r, Crop::full(), 4, (150.0, 80.0));
        assert!(approx_rect(nr, WorldRect::new(0.0, 0.0, 150.0, 80.0)));
        assert!(approx(nc.w, 0.75) && approx(nc.h, 0.8));
        assert!(approx_rect(content_rect(nr, nc), r));
    }

    #[test]
    fn edge_drag_cannot_grow_past_the_content() {
        // Already cropped: window is the right half of the source.
        let r = WorldRect::new(100.0, 0.0, 100.0, 100.0);
        let c = Crop {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        // Content spans x 0..200; drag the W edge far past the content edge.
        let (nr, nc) = edge_drag(r, c, 7, (-500.0, 50.0));
        assert!(approx(nr.x, 0.0) && approx(nr.w, 200.0));
        assert!(approx(nc.x, 0.0) && approx(nc.w, 1.0));
    }

    #[test]
    fn edge_drag_respects_minimum_window_size() {
        let r = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        // Collapse the E edge onto the W edge: clamps at MIN_CROP_WORLD.
        let (nr, _) = edge_drag(r, Crop::full(), 3, (0.0, 50.0));
        assert!(nr.w >= MIN_CROP_WORLD - 1e-3);
        // A window already below the minimum never blocks the gesture.
        let tiny = WorldRect::new(0.0, 0.0, 4.0, 4.0);
        let (nr2, _) = edge_drag(tiny, Crop::full(), 3, (2.0, 2.0));
        assert!(nr2.w <= 4.0 + 1e-3 && nr2.w >= 1.9);
    }

    #[test]
    fn pan_moves_content_with_the_pointer_and_clamps() {
        let r = WorldRect::new(0.0, 0.0, 100.0, 100.0);
        let c = Crop {
            x: 0.25,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        // Content rect is 200x200. Drag right by 20 → content follows the
        // pointer → the UV window slides left by 20/200 = 0.1.
        let p = pan_drag(r, c, (20.0, 0.0));
        assert!(approx(p.x, 0.15) && approx(p.y, 0.25));
        assert!(approx(p.w, 0.5) && approx(p.h, 0.5));
        // Clamped at both extremes; w/h never change.
        let p2 = pan_drag(r, c, (1000.0, -1000.0));
        assert!(approx(p2.x, 0.0) && approx(p2.y, 0.5));
        let p3 = pan_drag(r, c, (-1000.0, 1000.0));
        assert!(approx(p3.x, 0.5) && approx(p3.y, 0.0));
    }

    #[test]
    fn local_transforms_match_rotation() {
        // Rotation is clockwise in y-down world coords (matches
        // `WorldRect::corners_rotated`): the local up direction (0,-10)
        // rotated 90° cw lands at world (10,0), so the inverse transform
        // maps world (10,0) back to local (0,-10).
        let (lx, ly) = to_local(10.0, 0.0, 0.0, 0.0, 90.0);
        assert!(approx(lx, 0.0) && approx(ly, -10.0));
        let (dx, dy) = delta_local(10.0, 0.0, 90.0);
        assert!(approx(dx, 0.0) && approx(dy, -10.0));
        // Round trip through the forward rotation used by the painter.
        let rad = 90.0f32.to_radians();
        let (sin, cos) = rad.sin_cos();
        let (wx, wy) = (lx * cos - ly * sin, lx * sin + ly * cos);
        assert!(approx(wx, 10.0) && approx(wy, 0.0));
        // Zero rotation is the identity.
        let (ix, iy) = to_local(3.0, 4.0, 100.0, 100.0, 0.0);
        assert!(approx(ix, 3.0) && approx(iy, 4.0));
    }
}
