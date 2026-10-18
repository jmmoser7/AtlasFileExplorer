//! Existing crop math regressions, moved without assertion changes.
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
