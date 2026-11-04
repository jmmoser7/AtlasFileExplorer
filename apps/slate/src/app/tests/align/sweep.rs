//! Marquee sweep, frame drag, and shift-press selection.

use super::*;

/// GP1 — left-to-right over the middle of a line misses it.
#[test]
fn sweep_gp1_window_misses_a_crossing_line() {
    let mut h = align_board("sweep_gp1");
    let line = add_stroke(&mut h.app, 0.0, 50.0);
    sweep(
        &mut h,
        Pos2::new(-80.0, 20.0),
        Pos2::new(40.0, 80.0),
        egui::Modifiers::NONE,
    );
    assert!(!h.app.board_sel.contains(&line));
}

/// GP2 — right-to-left over that middle selects the line.
#[test]
fn sweep_gp2_crossing_hits_the_line() {
    let mut h = align_board("sweep_gp2");
    let line = add_stroke(&mut h.app, 0.0, 50.0);
    sweep(
        &mut h,
        Pos2::new(40.0, 120.0),
        Pos2::new(-80.0, 0.0),
        egui::Modifiers::NONE,
    );
    assert!(h.app.board_sel.contains(&line));
}

/// GP3 — window takes a fully inside rect and skips an edge overlap.
#[test]
fn sweep_gp3_window_takes_only_the_contained_rect() {
    let mut h = align_board("sweep_gp3");
    let inside = add_rect(&mut h.app, 10.0, 10.0);
    let overlap = add_rect(&mut h.app, 90.0, 10.0);
    sweep(
        &mut h,
        Pos2::new(-40.0, -40.0),
        Pos2::new(120.0, 100.0),
        egui::Modifiers::NONE,
    );
    assert!(h.app.board_sel.contains(&inside));
    assert!(!h.app.board_sel.contains(&overlap));
}

/// GP4 — Shift crossing adds without dropping the current selection.
#[test]
fn sweep_gp4_shift_adds() {
    let mut h = align_board("sweep_gp4");
    // A single selected curve owns a vertex window (P1.curve.grips).
    let kept = deck_frame(&mut h, WorldRect::new(400.0, 400.0, 80.0, 60.0), 0);
    let line = add_stroke(&mut h.app, 0.0, 50.0);
    h.app.board_sel.insert(kept);
    let mut shift = egui::Modifiers::NONE;
    shift.shift = true;
    sweep(&mut h, Pos2::new(40.0, 120.0), Pos2::new(-80.0, 0.0), shift);
    assert!(h.app.board_sel.contains(&kept));
    assert!(h.app.board_sel.contains(&line));
}

/// GP5 — a window with no modifier replaces the selection.
#[test]
fn sweep_gp5_window_replaces() {
    let mut h = align_board("sweep_gp5");
    let inside = add_rect(&mut h.app, 10.0, 10.0);
    let other = add_rect(&mut h.app, 400.0, 400.0);
    h.app.board_sel.insert(inside);
    h.app.board_sel.insert(other);
    sweep(
        &mut h,
        Pos2::new(-40.0, -40.0),
        Pos2::new(120.0, 100.0),
        egui::Modifiers::NONE,
    );
    assert!(h.app.board_sel.contains(&inside));
    assert!(!h.app.board_sel.contains(&other));
}

/// A single-click brush dab answers a left-to-right window sweep, a
/// right-to-left crossing sweep that only grazes its rim, and a click on its
/// ink away from the center.
#[test]
fn a_brush_dab_answers_sweeps_both_ways_and_a_click() {
    let mut h = align_board("brush_dab_sweep");
    h.app.brush_width = 20.0;
    h.app.finish_freehand_brush(vec![Pos2::new(100.0, 100.0)]);
    let dab = h.app.doc().scene.nodes.last().unwrap().id;
    h.frame();
    let none = egui::Modifiers::NONE;
    let swept = |h: &mut Harness, from: Pos2, to: Pos2| {
        h.app.board_sel.clear();
        sweep(h, from, to, none);
        h.app.board_sel.contains(&dab)
    };
    assert!(
        swept(&mut h, Pos2::new(50.0, 50.0), Pos2::new(125.0, 125.0)),
        "left-to-right window around the dab"
    );
    assert!(
        swept(&mut h, Pos2::new(150.0, 150.0), Pos2::new(104.0, 104.0)),
        "right-to-left crossing through the dab's rim"
    );
    assert!(
        !swept(&mut h, Pos2::new(50.0, 50.0), Pos2::new(104.0, 104.0)),
        "a window that cuts the dab leaves it"
    );
    h.app.board_sel.clear();
    h.app.board_click_for_test(Pos2::new(106.0, 100.0), none);
    assert!(h.app.board_sel.contains(&dab), "a click on the dab's ink");
}

/// An unselected frame that covers the viewport sweeps its members. Selecting
/// it first lets its body move with the same press-time grab as a smaller frame.
#[test]
fn frame_drag_moves_until_the_frame_covers_the_viewport() {
    let mut h = align_board("frame_cover_select");
    assert_eq!(board::FramePreset::Tabloid.size(), (1224.0, 792.0));
    let frame = h.seed_frame(None);
    let inside = add_rect(&mut h.app, 100.0, 80.0);
    let outside = add_rect(&mut h.app, 2000.0, 80.0);
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::vec2(400.0, 225.0);
    let press = Pos2::new(400.0, 300.0);
    let drag =
        h.app
            .begin_gesture_for_test(h.app.board_xf().w2s(press), press, egui::Modifiers::NONE);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "a frame smaller than the viewport moves"
    );

    h.app.board_drag = None;
    h.app.board_sel.clear();
    h.app.tab_mut().cam.z = 2.0;
    let drag =
        h.app
            .begin_gesture_for_test(h.app.board_xf().w2s(press), press, egui::Modifiers::NONE);
    assert!(
        matches!(
            drag,
            Some(board::BoardDrag::Marquee { frame: Some(id), .. }) if id == frame
        ),
        "a frame covering the viewport selects inside"
    );
    h.app.board_drag = drag;
    let end = Pos2::new(120.0, 90.0);
    h.app
        .end_gesture_for_test(end, Some(h.app.board_xf().w2s(end)), egui::Modifiers::NONE);
    assert!(h.app.board_sel.contains(&inside));
    assert!(!h.app.board_sel.contains(&frame));
    assert!(!h.app.board_sel.contains(&outside));

    h.app.board_sel = [frame].into_iter().collect();
    let original = h.app.doc().scene.node(frame).unwrap().rect;
    let member = h.app.doc().scene.node(inside).unwrap().rect;
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(h.app.board_xf().w2s(press), press, egui::Modifiers::NONE);
    assert!(matches!(
        h.app.board_drag,
        Some(board::BoardDrag::Move { .. })
    ));
    let mods = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    h.app.update_gesture_for_test(press, mods);
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect, original);
    h.app
        .update_gesture_for_test(press + egui::vec2(20.0, 10.0), mods);
    assert_eq!(
        h.app.doc().scene.node(frame).unwrap().rect,
        original.translated(20.0, 10.0)
    );
    assert_eq!(
        h.app.doc().scene.node(inside).unwrap().rect,
        member.translated(20.0, 10.0)
    );
    assert!(h.app.cancel_node_drag());
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect, original);
    assert_eq!(h.app.doc().scene.node(inside).unwrap().rect, member);
}

/// Hover-resize on an unselected rectangle must not steal Shift+select.
#[test]
fn shift_press_on_unselected_rect_edge_adds_instead_of_resize() {
    let mut h = align_board("shift_rect_edge");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 200.0, 0.0);
    h.app.board_sel = [a].into_iter().collect();
    h.frame();
    h.app.shift_down = true;
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(b).unwrap().clone();
    let edge = xf.w2s(Pos2::new(n.rect.x + n.rect.w, n.rect.y + 12.0));
    let edge_world = xf.s2w(edge);
    assert!(
        h.app.begin_transform_drag(edge, edge_world).is_none(),
        "shift on an unselected rect must not start resize"
    );
    let mut mods = egui::Modifiers::NONE;
    mods.shift = true;
    let body = Pos2::new(n.rect.x + 40.0, n.rect.y + 30.0);
    let drag = h.app.begin_gesture_for_test(xf.w2s(body), body, mods);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "shift+press on the next rect moves the additive set"
    );
    assert!(h.app.board_sel.contains(&a));
    assert!(h.app.board_sel.contains(&b));
}

#[test]
fn global_sweeps_and_multi_picks_select_frames_without_hosted_children() {
    let mut h = align_board("sweep_frame_roots");
    let first = deck_frame(&mut h, WorldRect::new(0.0, 0.0, 300.0, 220.0), 0);
    let second = deck_frame(&mut h, WorldRect::new(500.0, 100.0, 300.0, 220.0), 1);
    let first_child = add_rect(&mut h.app, 60.0, 60.0);
    let second_child = add_rect(&mut h.app, 560.0, 160.0);
    let outside = add_rect(&mut h.app, 1000.0, 400.0);
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let corners = [Pos2::new(-60.0, -60.0), Pos2::new(860.0, 380.0)];
    for [start, end] in [corners, [corners[1], corners[0]]] {
        h.app.board_sel.clear();
        sweep(&mut h, start, end, egui::Modifiers::NONE);
        assert_eq!(h.app.board_sel, [first, second].into_iter().collect());
        assert_eq!(h.app.board_selected_frame_owner(first_child), Some(first));
        assert_eq!(h.app.board_selected_frame_owner(second_child), Some(second));
        assert!(!h.app.board_sel.contains(&outside));
        assert_eq!(h.app.doc().scene.nodes, before);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }

    h.app
        .board_click_for_test(Pos2::new(900.0, 500.0), egui::Modifiers::NONE);
    assert!(h.app.board_sel.is_empty());
    h.app
        .board_click_for_test(Pos2::new(240.0, 180.0), egui::Modifiers::NONE);
    h.app.board_click_for_test(
        Pos2::new(740.0, 280.0),
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(h.app.board_sel, [first, second].into_iter().collect());
    h.app.board_sel.extend([first_child, second_child]);
    h.app.collapse_frame_selection();
    assert_eq!(h.app.board_sel, [first, second].into_iter().collect());
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn selected_frames_own_child_grips_and_body_drags_until_a_blank_click() {
    use board_handles::BoardHitTarget as Hit;

    let mut h = align_board("frame_group_child_drag");
    let first = deck_frame(&mut h, WorldRect::new(0.0, 0.0, 300.0, 240.0), 0);
    let second = deck_frame(&mut h, WorldRect::new(500.0, 80.0, 300.0, 240.0), 1);
    let first_child = add_rect(&mut h.app, 80.0, 60.0);
    let second_child = add_rect(&mut h.app, 580.0, 160.0);
    let locked = add_rect(&mut h.app, 190.0, 140.0);
    let outside = add_rect(&mut h.app, 1000.0, 400.0);
    h.app.patch_nodes(&[locked], |node| node.locked = true);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_sel = [first_child].into_iter().collect();
    h.frame();
    let xf = h.app.board_xf();
    let child = h.app.doc().scene.node(first_child).unwrap().clone();
    let geometry = h.app.node_interaction_geom(&child, &xf);
    let fillet = h
        .app
        .fillet_grip_at(&child, &xf)
        .expect("child corner grip");
    let center = xf.w2s(Pos2::from(child.rect.center()));
    let probes = [
        center,
        geometry.control_corners[2],
        geometry.control_edges[1],
        fillet,
    ];
    h.app.board_sel = [first, second].into_iter().collect();
    h.frame();
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    for screen in probes {
        assert_eq!(
            h.app.transform_hit_at(screen),
            Some((Some(first), Hit::Body))
        );
        assert!(h.app.fillet_grip_hit_at(screen).is_none());
        assert!(h.app.wire_grip_at(screen, &xf).is_none());
        let world = xf.s2w(screen);
        h.app.board_drag = h
            .app
            .begin_gesture_for_test(screen, world, egui::Modifiers::NONE);
        assert!(matches!(
            &h.app.board_drag,
            Some(board::BoardDrag::Move { ids, .. })
                if ids.len() == 4 && [first, second, first_child, second_child].iter().all(|id| ids.contains(id))
        ));
        assert!(h.app.cancel_node_drag());
        assert_eq!(h.app.doc().scene.nodes, before);
    }

    let press = xf.s2w(center);
    let delta = EVec2::new(64.0, 32.0);
    let mods = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    for cancel in [true, false] {
        h.app.board_drag = h
            .app
            .begin_gesture_for_test(center, press, egui::Modifiers::NONE);
        h.app.update_gesture_for_test(press, mods);
        assert_eq!(h.app.doc().scene.nodes, before);
        h.app.update_gesture_for_test(press + delta, mods);
        for node in &before {
            let mut expected = node.clone();
            if [first, second, first_child, second_child].contains(&node.id) {
                expected.rect = expected.rect.translated(delta.x, delta.y);
            }
            assert_eq!(h.app.doc().scene.node(node.id), Some(&expected));
        }
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        if cancel {
            assert!(h.app.cancel_node_drag());
        } else {
            h.app
                .end_gesture_for_test(press + delta, Some(xf.w2s(press + delta)), mods);
            assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
            h.app.board_undo();
        }
        assert_eq!(h.app.doc().scene.nodes, before);
    }

    h.app
        .board_click_for_test(Pos2::new(900.0, 500.0), egui::Modifiers::NONE);
    assert!(h.app.board_sel.is_empty());
    assert_eq!(h.app.board_selected_frame_owner(first_child), None);
    h.app.board_click_for_test(press, egui::Modifiers::NONE);
    assert_eq!(h.app.board_sel, [first_child].into_iter().collect());
    assert!(matches!(
        h.app.transform_hit_at(geometry.control_corners[2]),
        Some((Some(id), Hit::Resize(_))) if id == first_child
    ));
    assert_eq!(
        h.app.fillet_grip_hit_at(fillet).map(|(id, _)| id),
        Some(first_child)
    );
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(!h.app.board_sel.contains(&locked));
    assert!(!h.app.board_sel.contains(&outside));
}

#[test]
fn global_frame_sweep_and_select_all_use_the_real_canvas_input_path() {
    let mut h = align_board("frame_sweep_pointer_input");
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    let origins = [(-360.0, -130.0), (-40.0, -30.0), (280.0, 70.0)];
    let frames: Vec<_> = origins
        .iter()
        .enumerate()
        .map(|(order, (x, y))| {
            deck_frame(&mut h, WorldRect::new(*x, *y, 200.0, 140.0), order as u32)
        })
        .collect();
    for (x, y) in origins {
        add_rect(&mut h.app, x + 30.0, y + 30.0);
    }
    h.frame();
    h.frame();
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    let xf = h.app.board_xf();
    let start = xf.w2s(Pos2::new(-420.0, -190.0));
    let end = xf.w2s(Pos2::new(540.0, 270.0));
    assert!(h.app.canvas_rect.contains(start) && h.app.canvas_rect.contains(end));
    h.frame_with(pointer_to(start, false));
    h.frame_with(primary_button(start, true, false));
    h.frame_with(pointer_to(start.lerp(end, 0.25), false));
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Marquee { frame: None, .. })),
        "blank drag must enter marquee: dragged={:?}, origin_layer={:?}, canvas={:?}, down={}, dragging={}",
        h.ctx.dragged_id(),
        h.ctx.layer_id_at(start),
        h.app.canvas_rect,
        h.ctx.input(|i| i.pointer.primary_down()),
        h.ctx.input(|i| i.pointer.is_decidedly_dragging())
    );
    for t in [0.5, 0.75, 1.0] {
        h.frame_with(pointer_to(start.lerp(end, t), false));
    }
    h.frame_with(primary_button(end, false, false));
    h.frame();
    let expected = frames.into_iter().collect();
    assert_eq!(h.app.board_sel, expected);
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);

    h.frame_with(primary_button(start, true, false));
    h.frame_with(primary_button(start, false, false));
    assert!(h.app.board_sel.is_empty());
    h.frame_with(|input| {
        input.modifiers.ctrl = true;
        input.events.push(key_event(egui::Key::A, input.modifiers));
    });
    assert_eq!(h.app.board_sel, expected);
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
