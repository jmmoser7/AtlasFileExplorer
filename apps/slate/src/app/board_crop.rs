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

use slate_doc::scene::{Crop, Node, NodeKind, WorldRect};

/// Minimum crop-window size in world units (matches `MIN_DRAW` in board.rs).
pub const MIN_CROP_WORLD: f32 = 8.0;

/// The world rect the full uncropped image occupies, derived from the node
/// rect (the crop window) and its UV crop.
pub fn content_rect(rect: WorldRect, crop: Crop) -> WorldRect {
    slate_doc::geom::image_content_rect(rect, crop)
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
    slate_doc::geom::world_to_local_about(px, py, cx, cy, rotation_deg)
}

/// Rotate a world-space delta into the node's local axes.
pub fn delta_local(dx: f32, dy: f32, rotation_deg: f32) -> (f32, f32) {
    slate_doc::geom::world_to_local_about(dx, dy, 0.0, 0.0, rotation_deg)
}

/// Drag a crop-window edge/corner (`handle` 0–7: Nw N Ne E Se S Sw W, same
/// order as `board_handles::ResizeHandle`) to the pointer at `local`
/// (already in the node's local axes). The content stays fixed: the node
/// rect and the crop change together so only the mask moves. The window is
/// clamped inside the content rect and above a minimum size.
pub fn edge_drag(rect: WorldRect, crop: Crop, handle: u8, local: (f32, f32)) -> (WorldRect, Crop) {
    let content = content_rect(rect, crop);
    let (px, py) = local;
    // Keep the ordinary crop-window gesture's five-percent UX floor
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

/// Place `crop` on a node whose picture currently occupies `rect` under
/// `old`. The picture stays fixed in the world; the frame becomes the window
/// that shows `crop`.
pub fn place_crop(rect: WorldRect, old: Crop, crop: Crop) -> (WorldRect, Crop) {
    let content = content_rect(rect, old);
    let crop = crop.clamped();
    let window = WorldRect::new(
        content.x + crop.x * content.w,
        content.y + crop.y * content.h,
        crop.w * content.w,
        crop.h * content.h,
    );
    (window, crop_from_rects(window, content))
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

/// Clipped frames stay fixed: their crop grips follow the full source image.
/// Return the content box with its center rotated about the frame, so the
/// existing rotated handle owner paints and picks the same ghosted pixels.
pub fn handle_rect(node: &Node) -> WorldRect {
    if node.clip.is_none() {
        return node.rect;
    }
    let NodeKind::Image(image) = &node.kind else {
        return node.rect;
    };
    let content = content_rect(node.rect, image.crop);
    let center = node
        .rect
        .rotate_point([content.center().0, content.center().1], node.rotation_deg);
    WorldRect::new(
        center[0] - content.w * 0.5,
        center[1] - content.h * 0.5,
        content.w,
        content.h,
    )
}

/// Scale source pixels uniformly while the authored shape mask stays fixed.
/// The opposite content corner stays anchored until the cover constraint
/// requires sliding it. A source can never shrink enough to expose a gap.
pub fn content_scale_drag(frame: WorldRect, crop: Crop, handle: u8, local: (f32, f32)) -> Crop {
    let content = content_rect(frame, crop);
    let left = matches!(handle, 0 | 6 | 7);
    let right = matches!(handle, 2..=4);
    let top = matches!(handle, 0..=2);
    let bottom = matches!(handle, 4..=6);
    let sx = if left {
        (content.x + content.w - local.0) / content.w
    } else if right {
        (local.0 - content.x) / content.w
    } else {
        0.0
    };
    let sy = if top {
        (content.y + content.h - local.1) / content.h
    } else if bottom {
        (local.1 - content.y) / content.h
    } else {
        0.0
    };
    let min = (frame.w / content.w).max(frame.h / content.h);
    let max = (crop.w / Crop::MIN_SPAN)
        .min(crop.h / Crop::MIN_SPAN)
        .max(min);
    let scale = sx.max(sy).clamp(min, max);
    let w = (content.w * scale).max(frame.w);
    let h = (content.h * scale).max(frame.h);
    let x = if left {
        content.x + content.w - w
    } else if right {
        content.x
    } else {
        content.center().0 - w * 0.5
    };
    let y = if top {
        content.y + content.h - h
    } else if bottom {
        content.y
    } else {
        content.center().1 - h * 0.5
    };
    let content = WorldRect::new(
        x.clamp(frame.x + frame.w - w, frame.x),
        y.clamp(frame.y + frame.h - h, frame.y),
        w,
        h,
    );
    crop_from_rects(frame, content)
}

/// Apply the primary gesture's relative pan and uniform source scale to another
/// fixed mask. Each source keeps its own pixel aspect and continues covering it.
pub fn peer_content_crop(before: Crop, after: Crop, peer: Crop) -> Crop {
    let before = before.clamped();
    let after = after.clamped();
    let peer = peer.clamped();
    let scale = (after.w / before.w).min(after.h / before.h).clamp(
        (Crop::MIN_SPAN / peer.w).max(Crop::MIN_SPAN / peer.h),
        (1.0 / peer.w).min(1.0 / peer.h),
    );
    let w = peer.w * scale;
    let h = peer.h * scale;
    let cx = peer.x
        + peer.w * 0.5
        + ((after.x + after.w * 0.5) - (before.x + before.w * 0.5)) / before.w * peer.w;
    let cy = peer.y
        + peer.h * 0.5
        + ((after.y + after.h * 0.5) - (before.y + before.h * 0.5)) / before.h * peer.h;
    Crop {
        x: (cx - w * 0.5).clamp(0.0, 1.0 - w),
        y: (cy - h * 0.5).clamp(0.0, 1.0 - h),
        w,
        h,
    }
}

/// Crop-window handles on ordinary images; source-scale handles on shape fills.
pub fn edge_drag_node(before: &Node, handle: u8, local: (f32, f32)) -> (WorldRect, Crop) {
    let NodeKind::Image(image) = &before.kind else {
        return (before.rect, Crop::full());
    };
    if before.clip.is_some() {
        (
            before.rect,
            content_scale_drag(before.rect, image.crop, handle, local),
        )
    } else {
        edge_drag(before.rect, image.crop, handle, local)
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
    fn place_crop_copies_the_window_and_keeps_each_picture_put() {
        let a = WorldRect::new(0.0, 0.0, 200.0, 100.0);
        let (ar, ac) = edge_drag(a, Crop::full(), 7, (50.0, 50.0));
        let b = WorldRect::new(400.0, 10.0, 80.0, 40.0);
        let (br, bc) = place_crop(b, Crop::full(), ac);
        assert!(approx(bc.x, ac.x) && approx(bc.w, ac.w));
        assert!(approx(bc.y, ac.y) && approx(bc.h, ac.h));
        assert!(approx_rect(content_rect(ar, ac), a));
        assert!(approx_rect(content_rect(br, bc), b));
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

    #[test]
    fn shape_content_grips_scale_uniformly_and_cannot_shrink_below_cover() {
        let frame = WorldRect::new(100.0, 100.0, 100.0, 100.0);
        let crop = slate_doc::geom::cover_crop(frame, 100.0, Crop::full());
        for handle in 0..8 {
            let small = content_scale_drag(frame, crop, handle, frame.center());
            let content = content_rect(frame, small);
            assert!(approx(content.w / content.h, 100.0));
            assert!(content.w + 1e-3 >= frame.w && content.h + 1e-3 >= frame.h);
            assert!(content.x <= frame.x + 1e-3 && content.y <= frame.y + 1e-3);
            assert!(content.x + content.w + 1e-3 >= frame.x + frame.w);
            assert!(content.y + content.h + 1e-3 >= frame.y + frame.h);
        }
        let scale = content_scale_drag(frame, crop, 4, (10000.0, 500.0));
        assert!(scale.w < crop.w && scale.h < crop.h);
        assert!(approx(
            content_rect(frame, scale).w / content_rect(frame, scale).h,
            100.0
        ));
    }

    #[test]
    fn clipped_peers_keep_their_own_source_aspect_during_scale_and_pan() {
        let primary = Crop {
            x: 0.25,
            y: 0.25,
            w: 0.5,
            h: 0.5,
        };
        let changed = Crop {
            x: 0.5,
            y: 0.375,
            w: 0.25,
            h: 0.25,
        };
        let peer = Crop {
            x: 0.1,
            y: 0.0,
            w: 0.8,
            h: 1.0,
        };
        let result = peer_content_crop(primary, changed, peer);
        assert!(approx(result.w, 0.4) && approx(result.h, 0.5));
        assert!(approx(result.w / result.h, peer.w / peer.h));
        assert!(approx(result.x, 0.5) && approx(result.y, 0.25));
        let covering = peer_content_crop(changed, primary, peer);
        assert!(approx(covering.w / covering.h, peer.w / peer.h));
        assert!(covering.x >= 0.0 && covering.y >= 0.0);
        assert!(covering.x + covering.w <= 1.0 && covering.y + covering.h <= 1.0);
    }
}
