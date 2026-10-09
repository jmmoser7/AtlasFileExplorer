//! Group and rotated resize handles.

use super::*;

/// Ctrl+Alt+Shift on a group edge: members keep size and only translate.
#[test]
fn group_reposition_keeps_member_size() {
    let mut h = align_board("group_reposition");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 120.0, 0.0);
    select_ids(&mut h.app, &[a, b]);
    let gb = h.app.board_group_bounds().expect("group bounds");
    let before: Vec<_> = [a, b]
        .iter()
        .map(|id| h.app.doc().scene.node(*id).unwrap().clone())
        .collect();
    h.app.board_drag = Some(board::BoardDrag::GroupResize {
        ids: vec![a, b],
        before,
        group_before: gb,
        handle: board_handles::ResizeHandle::E as u8,
        dup: false,
    });
    let mods = egui::Modifiers {
        ctrl: true,
        alt: true,
        shift: true,
        ..Default::default()
    };
    h.app.update_gesture_for_test(Pos2::new(300.0, 30.0), mods);
    let ra = h.app.doc().scene.node(a).unwrap().rect;
    let rb = h.app.doc().scene.node(b).unwrap().rect;
    assert!((ra.w - 80.0).abs() < 1e-3 && (ra.h - 60.0).abs() < 1e-3);
    assert!((rb.w - 80.0).abs() < 1e-3 && (rb.h - 60.0).abs() < 1e-3);
    assert!((ra.x - 0.0).abs() < 1e-2, "left item stays, got {}", ra.x);
    assert!(
        (rb.x - 180.0).abs() < 1e-2,
        "right item translates, got {}",
        rb.x
    );
}

/// Every group-box handle × (scale | Ctrl+Alt+Shift): opposite union
/// handle stays put, and keep-size never changes member w/h. Nw / Ne / Sw
/// used to translate the whole stack; some corners looked one-axis-locked.
#[test]
fn group_every_handle_scale_and_reposition() {
    use super::super::super::board_snap;
    use board_handles::ResizeHandle;
    use slate_doc::scene::WorldRect;

    fn union_of(app: &SlateApp, ids: &[NodeId]) -> WorldRect {
        let rects: Vec<WorldRect> = ids
            .iter()
            .map(|id| app.doc().scene.node(*id).unwrap().rect)
            .collect();
        board_snap::union_rect(&rects).unwrap()
    }

    fn almost(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05
    }

    // Pointers that double a 200×100 group box uniformly (corners) or on
    // one axis (edges). The second member is offset on both axes so every
    // handle has layout to scale — a flat row made N/S look like a no-op.
    let cases: [(u8, Pos2); 8] = [
        (ResizeHandle::Nw as u8, Pos2::new(-200.0, -100.0)),
        (ResizeHandle::N as u8, Pos2::new(100.0, -100.0)),
        (ResizeHandle::Ne as u8, Pos2::new(400.0, -100.0)),
        (ResizeHandle::E as u8, Pos2::new(400.0, 50.0)),
        (ResizeHandle::Se as u8, Pos2::new(400.0, 200.0)),
        (ResizeHandle::S as u8, Pos2::new(100.0, 200.0)),
        (ResizeHandle::Sw as u8, Pos2::new(-200.0, 200.0)),
        (ResizeHandle::W as u8, Pos2::new(-200.0, 50.0)),
    ];

    for (handle, pointer) in cases {
        for reposition in [false, true] {
            let mut h = align_board(&format!("grp_{handle}_{reposition}"));
            let a = add_rect(&mut h.app, 0.0, 0.0);
            let b = add_rect(&mut h.app, 120.0, 40.0);
            select_ids(&mut h.app, &[a, b]);
            let gb = h.app.board_group_bounds().expect("group bounds");
            let before: Vec<_> = [a, b]
                .iter()
                .map(|id| h.app.doc().scene.node(*id).unwrap().clone())
                .collect();
            h.app.board_drag = Some(board::BoardDrag::GroupResize {
                ids: vec![a, b],
                before,
                group_before: gb,
                handle,
                dup: false,
            });
            let mut mods = egui::Modifiers::default();
            if reposition {
                mods.ctrl = true;
                mods.alt = true;
                mods.shift = true;
            }
            h.app.update_gesture_for_test(pointer, mods);
            let ra = h.app.doc().scene.node(a).unwrap().rect;
            let rb = h.app.doc().scene.node(b).unwrap().rect;
            if reposition {
                assert!(
                    (ra.w - 80.0).abs() < 1e-3 && (ra.h - 60.0).abs() < 1e-3,
                    "handle {handle} keep-size: A sized {}×{}",
                    ra.w,
                    ra.h
                );
                assert!(
                    (rb.w - 80.0).abs() < 1e-3 && (rb.h - 60.0).abs() < 1e-3,
                    "handle {handle} keep-size: B sized {}×{}",
                    rb.w,
                    rb.h
                );
            } else {
                assert!(
                    ra.w > 80.0 + 1.0 || ra.h > 60.0 + 1.0,
                    "handle {handle} scale: members did not grow"
                );
            }
            let union = union_of(&h.app, &[a, b]);
            let old_a = board_snap::resize_anchor(gb, handle, false);
            let new_a = board_snap::resize_anchor(union, handle, false);
            assert!(
                almost(old_a, new_a),
                "handle {handle} reposition={reposition}: opposite walked {old_a:?} → {new_a:?}"
            );
            let old_g = board_snap::handle_local(gb, handle);
            let new_g = board_snap::handle_local(union, handle);
            let grabbed = (old_g.0 - new_g.0).abs() + (old_g.1 - new_g.1).abs();
            assert!(
                grabbed > 1.0,
                "handle {handle} reposition={reposition}: grabbed handle did not move"
            );
        }
    }
}

/// After a 180° rotate, grabbing the visual top edge must move that edge
/// — not the far (visual bottom) edge. Rotation is about the live center,
/// so local AABB math alone walks the opposite world edge.
#[test]
fn rotated_180_resize_moves_the_grabbed_edge() {
    let mut h = web_board("rot180_resize");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    if let Some(n) = h.app.doc_mut().scene.node_mut(id) {
        n.rect = slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 80.0);
        n.rotation_deg = 180.0;
    }
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
    // After 180°, corners[2]→[3] (local S) is the visual top. Stay off the
    // midpoint so a wire grip does not swallow the press.
    let screen = geom.corners[2] + (geom.corners[3] - geom.corners[2]) * 0.25;
    let world = xf.s2w(screen);
    let drag = h.app.begin_transform_drag(screen, world);
    let ok = matches!(&drag, Some(board::BoardDrag::Resize { handle: 5, .. }));
    assert!(ok, "visual top after 180° is local S");
    h.app.board_drag = drag;

    let top0 = n
        .rect
        .corners_rotated(n.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::INFINITY, f32::min);
    let bot0 = n
        .rect
        .corners_rotated(n.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    let (cx, _) = n.rect.center();
    let mods = egui::Modifiers {
        alt: true,
        ..Default::default()
    }; // skip object-snap so the pin is the only translation
    h.app
        .update_gesture_for_test(Pos2::new(cx, top0 - 20.0), mods);

    let after = h.app.doc().scene.node(id).unwrap();
    let top1 = after
        .rect
        .corners_rotated(after.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::INFINITY, f32::min);
    let bot1 = after
        .rect
        .corners_rotated(after.rotation_deg)
        .into_iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (bot1 - bot0).abs() < 0.05,
        "visual bottom walked: {bot0} → {bot1}"
    );
    assert!(
        (top1 - (top0 - 20.0)).abs() < 0.05,
        "visual top should follow the pointer: {top0} → {top1}"
    );
}

/// A wide 2+ group box still offers a 45° corner-resize cursor (P1.node.transform).
#[test]
fn group_box_corner_is_diagonal_resize() {
    let mut h = web_board("group_corner_cursor");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 160.0, 0.0);
    select_ids(&mut h.app, &[a, b]);
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let gb = h.app.board_group_bounds().expect("group bounds");
    let geom = board_handles::selection_geom(&xf, gb, 0.0);
    let hit = h.app.transform_hit_at(geom.corners[0]);
    assert!(
        matches!(
            hit,
            Some((
                None,
                board_handles::BoardHitTarget::Resize(board_handles::ResizeHandle::Nw)
            ))
        ),
        "group-box corner must be a corner resize, got {hit:?}"
    );
    assert_eq!(
        board_handles::cursor_for_resize(board_handles::ResizeHandle::Nw, &geom),
        egui::CursorIcon::ResizeNorthWest
    );
}
