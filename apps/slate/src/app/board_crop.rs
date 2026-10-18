//! Pure math for the InDesign-style direct-manipulation crop mode.
//!
//! The node rect is the *frame* (the visible crop window in world space);
//! `Crop` is the normalized UV window into the source image. The "content
//! rect" is the world rect the full uncropped image would occupy if the crop
//! were reset — dragging a frame edge moves the mask over fixed content,
//! dragging inside slides the content under a fixed mask.
//!
//! Low-level crop math operates in the node's local (unrotated) space.
//! [`apply_gesture`] owns world-input conversion about the immutable start
//! pivot and compensates a resized frame's pivot so source pixels stay put.
//! Human input and agent witnesses replay that same world-space gesture.

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

/// A displayed-world input replayed from one immutable gesture-start node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CropGesture {
    Handle { handle: u8, world: (f32, f32) },
    Pan { delta_world: (f32, f32) },
}

/// Apply the human crop gesture; callers own eligibility, previews and journaling.
/// Crop coordinates already describe displayed pixels, including source mirrors.
pub fn apply_gesture(before: &Node, gesture: CropGesture) -> Option<Node> {
    let NodeKind::Image(image) = &before.kind else {
        return None;
    };
    let r = before.rect;
    if ![
        r.x,
        r.y,
        r.w,
        r.h,
        before.rotation_deg,
        image.crop.x,
        image.crop.y,
        image.crop.w,
        image.crop.h,
    ]
    .iter()
    .all(|v| v.is_finite())
        || r.w <= 0.0
        || r.h <= 0.0
    {
        return None;
    }
    let (rect, crop) = match gesture {
        CropGesture::Handle {
            handle,
            world: (x, y),
        } => {
            if handle > 7 || !x.is_finite() || !y.is_finite() {
                return None;
            }
            let (cx, cy) = r.center();
            edge_drag_node(before, handle, to_local(x, y, cx, cy, before.rotation_deg))
        }
        CropGesture::Pan {
            delta_world: (dx, dy),
        } => {
            if !dx.is_finite() || !dy.is_finite() {
                return None;
            }
            (
                r,
                pan_drag(r, image.crop, delta_local(dx, dy, before.rotation_deg)),
            )
        }
    };
    Some(with_window(before, rect, crop))
}

/// Bound visual travel to the gesture's actual source/window movement. The
/// requested gesture still owns the exact prepared Node; callers keep that
/// original Node at the final sample rather than recomputing it from this
/// world endpoint (whose inverse transform can round by a few ULPs).
pub fn effective_gesture(before: &Node, requested: CropGesture) -> Option<CropGesture> {
    let after = apply_gesture(before, requested)?;
    match requested {
        CropGesture::Handle { handle, .. } => Some(CropGesture::Handle {
            handle,
            world: gesture_contact(&after, requested)?,
        }),
        CropGesture::Pan { .. } => {
            let (NodeKind::Image(start), NodeKind::Image(end)) = (&before.kind, &after.kind) else {
                return None;
            };
            let start_crop = start.crop.clamped();
            let end_crop = end.crop.clamped();
            let content = content_rect(before.rect, start_crop);
            let local = (
                (start_crop.x - end_crop.x) * content.w,
                (start_crop.y - end_crop.y) * content.h,
            );
            Some(CropGesture::Pan {
                delta_world: delta_local(local.0, local.1, -before.rotation_deg),
            })
        }
    }
}

/// The painted handle's world contact. Pan returns None: its caller must use
/// the existing visible-body hit owner, which knows the authored clip and holes.
pub fn gesture_contact(before: &Node, gesture: CropGesture) -> Option<(f32, f32)> {
    let CropGesture::Handle { handle, .. } = gesture else {
        return None;
    };
    if handle > 7 || !matches!(before.kind, NodeKind::Image(_)) {
        return None;
    }
    let corners = handle_rect(before).corners_rotated(before.rotation_deg);
    let a = corners[handle as usize / 2];
    let b = corners[(handle as usize / 2 + 1) % 4];
    Some(if handle % 2 == 0 {
        a
    } else {
        ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
    })
}

/// Preserve the existing multi-crop policy using the same rotated-window owner.
pub fn apply_peer_crop(before: &Node, primary_before: Crop, primary_after: Crop) -> Option<Node> {
    let NodeKind::Image(image) = &before.kind else {
        return None;
    };
    let (rect, crop) = if before.clip.is_some() {
        (
            before.rect,
            peer_content_crop(primary_before, primary_after, image.crop),
        )
    } else {
        place_crop(before.rect, image.crop, primary_after)
    };
    Some(with_window(before, rect, crop))
}

/// Transfer prepared crop geometry into the current image, retaining unrelated
/// live fields when another edit overlaps a human drag. No gesture math lives here.
pub fn copy_crop_geometry(live: &mut Node, prepared: &Node) -> bool {
    if live.id != prepared.id {
        return false;
    }
    let (NodeKind::Image(current), NodeKind::Image(next)) = (&mut live.kind, &prepared.kind) else {
        return false;
    };
    live.rect = prepared.rect;
    current.crop = next.crop;
    true
}

/// Changing a rotated window also changes its rotation pivot. Compensate that
/// pivot movement so every surviving source pixel stays at its world anchor.
fn with_window(before: &Node, mut rect: WorldRect, crop: Crop) -> Node {
    if before.clip.is_none() && rect != before.rect {
        let (old_x, old_y) = before.rect.center();
        let (new_x, new_y) = rect.center();
        let [turned_x, turned_y] = before
            .rect
            .rotate_point([new_x, new_y], before.rotation_deg);
        rect.x += turned_x - old_x - (new_x - old_x);
        rect.y += turned_y - old_y - (new_y - old_y);
    }
    let mut node = before.clone();
    node.rect = rect;
    if let NodeKind::Image(image) = &mut node.kind {
        image.crop = crop;
    }
    node
}

#[cfg(test)]
mod gesture_tests;
#[cfg(test)]
mod tests;
