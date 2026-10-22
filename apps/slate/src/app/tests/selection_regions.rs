//! Native acceptance regressions for the approved selection-region contract.
use super::*;
use board_handles::{BoardHitTarget as Hit, ResizeHandle as Handle};
use slate_doc::scene::Side;

mod low_zoom_picks;
mod overlap_filters;
mod zoom_path;

fn card(h: &mut Harness, rect: WorldRect) -> NodeId {
    let id = add_image_card(&mut h.app, rect.x, rect.y);
    h.app.patch_nodes(&[id], |node| node.rect = rect);
    id
}

#[test]
fn foreground_move_resize_and_rotation_keep_their_isolated_winners() {
    let mut h = grip_board("regions_foreground_invariance");
    let a = card(&mut h, WorldRect::new(-40.0, -40.0, 320.0, 192.0));
    let b = card(&mut h, WorldRect::new(0.0, 0.0, 240.0, 112.0));
    for (front, rear) in [(b, a), (a, b)] {
        h.app.reorder_nodes(&[front], true);
        h.app.board_sel = std::iter::once(front).collect();
        h.app.patch_nodes(&[rear], |node| node.hidden = true);
        h.frame();
        let xf = h.app.board_xf();
        let node = h.app.doc().scene.node(front).unwrap();
        let geom = h.app.node_interaction_geom(node, &xf);
        let probes = [
            (xf.w2s(Pos2::from(node.rect.center())), Hit::Body),
            (geom.control_corners[2], Hit::Resize(Handle::Se)),
            (
                geom.control_edges[1] - EVec2::new(0.0, node.rect.h * xf.z * 0.25),
                Hit::Resize(Handle::E),
            ),
            (geom.rotate_points[2], Hit::Rotate(2)),
        ];
        for (point, expected) in probes {
            assert_eq!(
                h.app.transform_hit_at(point),
                Some((Some(front), expected)),
                "isolated fixture at {point:?}"
            );
        }
        h.app.patch_nodes(&[rear], |node| node.hidden = false);
        h.frame();
        let overlapped = h
            .app
            .node_interaction_geom(h.app.doc().scene.node(front).unwrap(), &xf);
        assert_eq!(
            overlapped.control_corners, geom.control_corners,
            "underlays must not shrink foreground fields"
        );
        for (point, expected) in probes {
            assert_eq!(
                h.app.transform_hit_at(point),
                Some((Some(front), expected)),
                "overlap stole {expected:?} at {point:?}"
            );
            let drag = h
                .app
                .begin_gesture_for_test(point, xf.s2w(point), egui::Modifiers::NONE);
            match expected {
                Hit::Body => assert!(
                    matches!(drag, Some(board::BoardDrag::Move { ids, .. }) if ids == vec![front])
                ),
                Hit::Resize(_) => assert!(
                    matches!(drag, Some(board::BoardDrag::Resize { id, .. }) if id == front)
                ),
                Hit::Rotate(_) => assert!(
                    matches!(drag, Some(board::BoardDrag::Rotate { id, .. }) if id == front)
                ),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn foreground_move_core_beats_a_rear_corner_on_the_same_pixel() {
    let mut h = grip_board("regions_front_body_rear_corner");
    let rear = card(&mut h, WorldRect::new(120.0, 56.0, 120.0, 120.0));
    let front = card(&mut h, WorldRect::new(0.0, 0.0, 240.0, 112.0));
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let point = xf.w2s(Pos2::new(120.0, 56.0));
    let rear_geom =
        board_handles::selection_geom(&xf, h.app.doc().scene.node(rear).unwrap().rect, 0.0);
    assert_eq!(
        board_handles::chrome_pick(point, &rear_geom, true).map(|p| p.0),
        Some(Hit::Resize(Handle::Nw))
    );
    assert_eq!(
        h.app.transform_hit_at(point),
        Some((Some(front), Hit::Body))
    );
    let drag = h
        .app
        .begin_gesture_for_test(point, xf.s2w(point), egui::Modifiers::NONE);
    assert!(matches!(drag, Some(board::BoardDrag::Move { ids, .. }) if ids == vec![front]));
}

#[test]
fn overlapping_front_ports_require_strict_interior_at_normal_compact_and_rotated_hosts() {
    for (w, height, angle) in [(240.0, 112.0, 0.0), (80.0, 60.0, 0.0), (240.0, 112.0, 37.0)] {
        let mut h = grip_board("regions_overlap_wire_interior");
        card(&mut h, WorldRect::new(-80.0, -80.0, 400.0, 400.0));
        let front = card(&mut h, WorldRect::new(0.0, 0.0, w, height));
        h.app.patch_nodes(&[front], |node| {
            node.rotation_deg = angle;
            slate_doc::scene::set_corner(node, slate_doc::scene::Corner::Rounded { radius: 12.0 });
        });
        h.app.board_sel.clear();
        h.frame();
        let xf = h.app.board_xf();
        let host = h.app.wire_host(h.app.doc().scene.node(front).unwrap());
        let anchor = xf.w2s(Pos2::from(host.anchor(Side::Right, 0.5)));
        let outward = EVec2::from(host.outward(Side::Right, 0.5));
        let inside = anchor - outward;
        assert_eq!(
            h.app.wire_grip_at(inside, &xf).map(|p| (p.0, p.1)),
            Some((front, Side::Right))
        );
        for point in [anchor, anchor + outward] {
            assert!(
                h.app.wire_grip_at(point, &xf).is_none(),
                "outside/top-edge wire at {angle} degrees"
            );
            assert_eq!(
                h.app.transform_hit_at(point),
                Some((Some(front), Hit::Resize(Handle::E)))
            );
        }
        assert!(matches!(
            h.app
                .begin_gesture_for_test(inside, xf.s2w(inside), egui::Modifiers::NONE),
            Some(board::BoardDrag::Wire(_))
        ));
    }
}

#[test]
fn adjacent_coincident_corners_choose_geometry_then_front_order_at_multiple_zooms() {
    let mut h = grip_board("regions_adjacent_corners");
    let left = card(&mut h, WorldRect::new(0.0, 0.0, 240.0, 112.0));
    let right = card(&mut h, WorldRect::new(240.0, 0.0, 120.0, 300.0));
    h.app.board_sel.clear();
    for front in [right, left] {
        h.app.reorder_nodes(&[front], true);
        for zoom in [1.0, 2.0, 4.0] {
            h.app.tab_mut().cam.z = zoom;
            h.app.tab_mut().cam.offset = EVec2::new(240.0, 100.0);
            h.frame();
            let xf = h.app.board_xf();
            let shared = xf.w2s(Pos2::new(240.0, 0.0));
            for (point, id, handle) in [
                (shared + EVec2::new(-3.0, -3.0), left, Handle::Ne),
                (shared + EVec2::new(3.0, -3.0), right, Handle::Nw),
                (
                    shared + EVec2::new(0.0, -3.0),
                    front,
                    if front == left {
                        Handle::Ne
                    } else {
                        Handle::Nw
                    },
                ),
            ] {
                assert_eq!(
                    h.app.transform_hit_at(point),
                    Some((Some(id), Hit::Resize(handle))),
                    "zoom {zoom}, front {front:?}, probe {point:?}"
                );
            }
        }
    }
}

#[test]
fn adjacent_corner_tie_uses_the_rounded_silhouette_instead_of_box_or_center_distance() {
    let mut h = grip_board("regions_rounded_corner_tie");
    let left = card(&mut h, WorldRect::new(0.0, 0.0, 240.0, 112.0));
    let right = card(&mut h, WorldRect::new(240.0, 0.0, 120.0, 300.0));
    h.app.patch_nodes(&[left], |node| {
        slate_doc::scene::set_corner(node, slate_doc::scene::Corner::Rounded { radius: 30.0 })
    });
    h.app.reorder_nodes(&[left], true);
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let point = xf.w2s(Pos2::new(239.0, -3.0));
    assert_eq!(
        h.app.transform_hit_at(point),
        Some((Some(right), Hit::Resize(Handle::Nw))),
        "the closer square host wins even on the rounded host's side and behind it"
    );
}

#[test]
fn a_one_unit_host_recovers_wires_at_large_projected_size_and_loses_them_when_small() {
    let mut h = grip_board("regions_point_wire_recovery");
    let id = card(&mut h, WorldRect::new(0.0, 0.0, 1.0, 1.0));
    h.app.board_sel.clear();
    h.frame();
    let focus = h.app.world_to_screen(Pos2::new(0.5, 0.5));
    for zoom in [0.25, 1.0, 12.0, 128.0] {
        h.app.zoom_at(focus, zoom / h.app.tab().cam.z);
        assert!(
            (h.app.tab().cam.z - zoom).abs() < 0.001,
            "point detail must be reachable through the public zoom path"
        );
        let xf = h.app.board_xf();
        let host = h.app.wire_host(h.app.doc().scene.node(id).unwrap());
        let point = xf.w2s(Pos2::from(host.anchor(Side::Right, 0.5)));
        let picked = h.app.wire_grip_at(point, &xf);
        if zoom == 128.0 {
            assert_eq!(picked.map(|p| (p.0, p.1)), Some((id, Side::Right)));
            assert!(matches!(
                h.app
                    .begin_gesture_for_test(point, xf.s2w(point), egui::Modifiers::NONE),
                Some(board::BoardDrag::Wire(_))
            ));
        } else {
            assert!(
                picked.is_none(),
                "fine wire action remained active at {zoom}"
            );
        }
    }
}

#[test]
fn revealing_raster_inputs_does_not_move_its_compact_output_anchor() {
    let mut h = grip_board("regions_raster_output_parity");
    let id = picture_from(&mut h, "ordinary-raster.png", 0.0);
    for height in [1.0, 18.0, 60.0, 128.0, 400.0] {
        for angle in [0.0, 37.0] {
            h.app.patch_nodes(&[id], |node| {
                node.rect.h = height;
                node.rotation_deg = angle;
            });
            let node = h.app.doc().scene.node(id).unwrap();
            let before = node.clone();
            assert_eq!(
                h.app.wire_input_ports(node).len(),
                slate_doc::agent_inputs::GENERATOR_PORTS.len()
            );
            let resting = h.app.wire_host(node);
            let revealed = h.app.wire_input_host(node);
            let output = resting.anchor(Side::Right, slate_doc::agent_inputs::OUTPUT_T);
            let typed_output = revealed.anchor(Side::Right, slate_doc::agent_inputs::OUTPUT_T);
            assert!(
                Pos2::from(output).distance(Pos2::from(typed_output)) < 0.0001,
                "output jumped when inputs revealed: height {height}, angle {angle}"
            );
            assert_eq!(revealed.ports().last().unwrap().point, typed_output);
            assert_eq!(
                h.app.doc().scene.node(id),
                Some(&before),
                "port presentation is derived and cannot mutate the host"
            );
        }
    }
}

#[test]
fn compact_frame_resize_has_no_initial_jump_keeps_its_owner_and_commits_one_undo() {
    let mut h = grip_board("regions_compact_resize");
    let id = card(&mut h, WorldRect::new(0.0, 0.0, 48.0, 36.0));
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    let geom = h.app.node_interaction_geom(&before, &xf);
    let from = geom.control_corners[2];
    assert!(
        from.distance(geom.corners[2]) > 4.0,
        "fixture must use an expanded control frame"
    );
    let start = xf.s2w(from);
    h.app.board_drag = h
        .app
        .begin_gesture_for_test(from, start, egui::Modifiers::NONE);
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Resize { id: owner, .. }) if owner == id)
    );
    h.app.update_gesture_for_test(start, egui::Modifiers::NONE);
    assert_eq!(
        h.app.doc().scene.node(id),
        Some(&before),
        "a stationary press on the frame must not resize the real image"
    );
    let to = start - EVec2::new(40.0, 30.0);
    h.app.update_gesture_for_test(to, egui::Modifiers::NONE);
    let live = h.app.doc().scene.node(id).unwrap();
    assert!(
        live.rect.w < 20.0 && live.rect.h < 20.0,
        "fixture crosses the resize dormancy boundary"
    );
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Resize { id: owner, .. }) if owner == id)
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app
        .end_gesture_for_test(to, Some(xf.w2s(to)), egui::Modifiers::NONE);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
}

#[test]
fn a_real_compact_frame_drag_cancels_exactly_without_an_undo_entry() {
    let mut h = grip_board("regions_compact_cancel");
    let id = card(&mut h, WorldRect::new(0.0, 0.0, 48.0, 36.0));
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    let from = h.app.node_interaction_geom(&before, &xf).control_corners[2];
    drag_screen(&mut h, from, from + EVec2::new(70.0, 45.0), true);
    assert!(h.app.board_drag.is_none());
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.board_sel, std::iter::once(id).collect());
}

#[test]
fn a_held_compact_resize_keeps_its_owner_while_crossing_a_foreground_body() {
    let mut h = grip_board("regions_captured_resize_across_host");
    let id = card(&mut h, WorldRect::new(0.0, 0.0, 48.0, 36.0));
    let foreign = card(&mut h, WorldRect::new(200.0, 0.0, 160.0, 120.0));
    h.app.board_sel = std::iter::once(id).collect();
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let other = h.app.doc().scene.node(foreign).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    let from = h.app.node_interaction_geom(&before, &xf).control_corners[2];
    let to = Pos2::new(280.0, 60.0);
    hold_drag(&mut h, xf.s2w(from), to, egui::Modifiers::NONE);
    assert_eq!(
        h.app.transform_hit_at(xf.w2s(to)),
        Some((Some(foreign), Hit::Body)),
        "the current hover target belongs to another host"
    );
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Resize { id: owner, .. }) if owner == id),
        "captured resize must not retarget to the current hover host"
    );
    assert_ne!(h.app.doc().scene.node(id), Some(&before));
    assert_eq!(h.app.doc().scene.node(foreign), Some(&other));
    escape_then_release(&mut h, to, egui::Modifiers::NONE);
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert_eq!(h.app.doc().scene.node(foreign), Some(&other));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn a_buried_selection_moves_from_its_clear_outside_header_and_undoes_once() {
    let mut h = grip_board("regions_buried_header");
    let rear = card(&mut h, WorldRect::new(0.0, 0.0, 120.0, 80.0));
    let front = card(&mut h, WorldRect::new(-20.0, -20.0, 160.0, 120.0));
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    assert_eq!(
        h.app.transform_hit_at(xf.w2s(Pos2::ZERO)),
        Some((Some(front), Hit::Body))
    );
    h.app.board_sel = std::iter::once(rear).collect();
    h.frame();
    let before = h.app.doc().scene.node(rear).unwrap().clone();
    let covering = h.app.doc().scene.node(front).unwrap().clone();
    let geom = h.app.node_interaction_geom(&before, &xf);
    let header = h
        .app
        .interaction_move_header(&before, &geom, &xf)
        .expect("simple centered cover has space for an outside move header");
    assert!(h.app.canvas_rect.contains_rect(header));
    assert_eq!(
        h.app.transform_hit_at(header.center()),
        Some((Some(rear), Hit::Body))
    );
    let depth = h.app.tab().journal.undo_depth();
    drag_screen(
        &mut h,
        header.center(),
        header.center() + EVec2::new(40.0, 20.0),
        false,
    );
    let moved = h.app.doc().scene.node(rear).unwrap();
    assert!((moved.rect.x - before.rect.x - 40.0).abs() < 0.1);
    assert!((moved.rect.y - before.rect.y - 20.0).abs() < 0.1);
    assert_eq!(h.app.doc().scene.node(front), Some(&covering));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(rear), Some(&before));
}
