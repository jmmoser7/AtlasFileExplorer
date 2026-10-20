//! Customer zoom paths, persistence and tiny authored-size preservation.
use super::*;

#[test]
fn a_settled_wheel_notch_is_applied_once_and_its_inverse_restores_zoom() {
    let mut h = grip_board("regions_wheel_one_stream");
    let point = h.app.canvas_rect.center() + EVec2::new(80.0, -40.0);
    let world = h.app.screen_to_world(point);
    let mut time = h.ctx.input(|i| i.time);
    let mut notch = |h: &mut Harness, delta: f32| {
        for frame in 0..28 {
            time += 1.0 / 60.0;
            h.frame_with(|input| {
                input.time = Some(time);
                if frame == 0 {
                    input.events.push(egui::Event::PointerMoved(point));
                    input.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: EVec2::new(0.0, delta),
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            });
        }
        assert_eq!(h.ctx.input(|i| i.smooth_scroll_delta), EVec2::ZERO);
        assert!(h.app.screen_to_world(point).distance(world) < 0.001);
    };
    notch(&mut h, 120.0);
    assert!((h.app.tab().cam.z - 1.197_217_3).abs() < 0.0001);
    notch(&mut h, -120.0);
    assert!((h.app.tab().cam.z - 1.0).abs() < 0.0001);
}

#[test]
fn board_wheel_reaches_point_detail_and_wide_overview_while_anchoring_the_pointer() {
    let mut h = grip_board("regions_real_wheel_range");
    let pointer = h.app.canvas_rect.center() + EVec2::new(86.0, -54.0);
    let anchor = h.app.screen_to_world(pointer);
    let start_zoom = h.app.tab().cam.z;
    let wheel = |h: &mut Harness, delta: f32| {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(pointer));
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: EVec2::new(0.0, delta),
                modifiers: egui::Modifiers::NONE,
            });
        });
    };
    wheel(&mut h, 40.0);
    assert!(
        h.app.tab().cam.z > start_zoom && h.app.tab().cam.z < 128.0,
        "small wheel input must produce an intermediate zoom"
    );
    assert!(h.app.screen_to_world(pointer).distance(anchor) < 0.001);
    for _ in 0..10 {
        wheel(&mut h, 600.0);
        assert!(h.app.screen_to_world(pointer).distance(anchor) < 0.001);
    }
    assert!(
        (h.app.tab().cam.z - 128.0).abs() < 0.001,
        "actual board wheel input must reach the approved point-detail zoom"
    );
    // Repeated large negative notches also overcome any positive smooth-wheel
    // backlog, rather than assuming one frame consumes an arbitrary delta.
    for _ in 0..4 {
        wheel(&mut h, -10_000.0);
        assert!(h.app.screen_to_world(pointer).distance(anchor) < 0.01);
    }
    assert!((h.app.tab().cam.z - 1.0 / 128.0).abs() < 0.000001);
}

#[test]
fn legacy_nonboard_camera_modes_keep_their_previous_zoom_limits() {
    let mut h = grip_board("regions_legacy_zoom_limits");
    let pointer = h.app.canvas_rect.center();
    // Retired view values still exist in the compatibility model. Exercise
    // camera routing before a frame normalizes those values back to Board.
    for view in [
        ViewKind::Grid,
        ViewKind::Branch,
        ViewKind::Venn,
        ViewKind::Lens,
    ] {
        h.app.doc_mut().view.active_view = view;
        h.app.zoom_at(pointer, 1_000_000.0);
        assert_eq!(h.app.tab().cam.z, 3.5, "legacy {view:?} maximum changed");
        h.app.zoom_at(pointer, 0.000000001);
        assert_eq!(h.app.tab().cam.z, 0.05, "legacy {view:?} minimum changed");
    }
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.zoom_at(pointer, 1_000_000.0);
    assert_eq!(h.app.tab().cam.z, 128.0);
}

#[test]
fn saved_board_point_detail_reopens_and_a_one_unit_image_resizes_without_an_initial_jump() {
    let mut h = grip_board("regions_saved_point_detail");
    let id = card(&mut h, WorldRect::new(0.0, 0.0, 1.0, 1.0));
    h.app.board_sel.clear();
    h.frame();
    let focus = h.app.world_to_screen(Pos2::new(0.5, 0.5));
    let world = h.app.screen_to_world(focus);
    h.app.zoom_at(focus, 1_000_000.0);
    assert_eq!(h.app.tab().cam.z, 128.0);
    assert!(h.app.screen_to_world(focus).distance(world) < 0.001);
    let camera = h.app.tab().cam;
    let path = h.base.join("point-detail.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());
    assert!(path.exists());
    drop(h); // Close the first app and release its workbook write lease.
    let mut reopened = grip_board("regions_reopened_point_detail");
    reopened.app.open_doc_at(path);
    reopened.frame();
    assert_eq!(reopened.app.doc().view.active_view, ViewKind::Board);
    assert_eq!(
        reopened.app.tab().cam.z,
        128.0,
        "restore must not apply the old 3.5 maximum"
    );
    assert!((reopened.app.tab().cam.offset - camera.offset).length() < 0.001);
    assert!(!reopened.app.tab().read_only);
    reopened.app.board_osnap.enabled = false;
    reopened.app.board_smart_guides = false;
    reopened.app.board_snap_grid = false;
    let xf = reopened.app.board_xf();
    let before = reopened.app.doc().scene.node(id).unwrap().clone();
    let host = reopened.app.wire_host(&before);
    let wire_point = xf.w2s(Pos2::from(host.anchor(Side::Right, 0.5)));
    assert_eq!(
        reopened
            .app
            .wire_grip_at(wire_point, &xf)
            .map(|p| (p.0, p.1)),
        Some((id, Side::Right)),
        "the reopened point must regain a real wire action"
    );
    reopened.app.board_sel = std::iter::once(id).collect();
    let depth = reopened.app.tab().journal.undo_depth();
    let from = reopened
        .app
        .node_interaction_geom(&before, &xf)
        .control_corners[2];
    let start = xf.s2w(from);
    reopened.app.board_drag =
        reopened
            .app
            .begin_gesture_for_test(from, start, egui::Modifiers::NONE);
    assert!(
        matches!(reopened.app.board_drag, Some(board::BoardDrag::Resize { id: owner, .. }) if owner == id)
    );
    reopened
        .app
        .update_gesture_for_test(start, egui::Modifiers::NONE);
    assert_eq!(
        reopened.app.doc().scene.node(id),
        Some(&before),
        "first resize of an existing 1u image must not inflate it to the 8u creation minimum"
    );
    let to = start + EVec2::splat(0.25);
    reopened
        .app
        .update_gesture_for_test(to, egui::Modifiers::NONE);
    let live = reopened.app.doc().scene.node(id).unwrap();
    assert!((live.rect.w - 1.25).abs() < 0.001 && (live.rect.h - 1.25).abs() < 0.001);
    reopened
        .app
        .end_gesture_for_test(to, Some(xf.w2s(to)), egui::Modifiers::NONE);
    assert_eq!(reopened.app.tab().journal.undo_depth(), depth + 1);
    reopened.app.board_undo();
    assert_eq!(reopened.app.doc().scene.node(id), Some(&before));
}
