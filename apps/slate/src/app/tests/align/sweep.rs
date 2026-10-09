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
    let kept = add_rect(&mut h.app, 400.0, 400.0);
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

/// A frame that covers the viewport selects its members on drag. A smaller
/// frame still moves.
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
