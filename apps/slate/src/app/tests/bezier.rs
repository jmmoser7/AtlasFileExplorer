//! Bezier span drafting and editing tests.

use super::*;

/// Bézier D04 / GP1: a stationary click places a corner anchor with zero
/// handles, so a span can hold straight segments and sharp corners.
#[test]
fn bezier_stationary_click_places_a_corner_anchor_without_handles() {
    let mut h = bezier_board("bezier_click_corner");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    for p in pts {
        bezier_click(&mut h, p);
    }
    let anchors = bezier_draft(&h);
    assert_eq!(anchors.len(), 3, "each stationary click places one anchor");
    for ((p, handles), want) in anchors.iter().zip(pts) {
        assert!(near(*p, want), "{p:?} != {want:?}");
        assert_eq!(
            *handles,
            board_path::BezierHandles::default(),
            "a click has no weight"
        );
    }
    assert!(h.app.path_tool_try_finish());
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("a shape")
    };
    let path = s.path.as_ref().unwrap();
    assert!(
        path.segs
            .iter()
            .all(|seg| matches!(seg, slate_doc::scene::PathSeg::Line { .. })),
        "handle-less anchors commit straight segments: {:?}",
        path.segs
    );
}

/// Bézier D04: two quick clicks in different places are two anchors (the
/// stationary-click test above); a double-click on the anchor it just placed
/// finishes the span.
#[test]
fn bezier_double_click_on_the_placed_anchor_finishes_the_span() {
    let mut h = bezier_board("bezier_double_click_finish");
    let depth = h.app.tab().journal.undo_depth();
    let (a, b) = (Pos2::new(0.0, 0.0), Pos2::new(120.0, 40.0));
    bezier_click(&mut h, a);
    let t = h.ctx.input(|i| i.time);
    h.frame_with(|i| i.time = Some(t + 1.0));
    bezier_click(&mut h, b);
    assert_eq!(bezier_draft(&h).len(), 2);
    bezier_click(&mut h, b);
    assert!(
        h.app.board_path_draft.is_none(),
        "the double-click finished"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("a shape")
    };
    assert_eq!(
        s.path.as_ref().unwrap().segs.len(),
        1,
        "no duplicate anchor"
    );
}

/// Bézier GP2 through real pointer events: press-drag still authors
/// symmetric handles.
#[test]
fn bezier_press_drag_still_authors_symmetric_handles() {
    let mut h = bezier_board("bezier_drag_handles");
    press_drag_release(
        &mut h,
        &[
            Pos2::new(0.0, 0.0),
            Pos2::new(20.0, 0.0),
            Pos2::new(40.0, 0.0),
        ],
        egui::Modifiers::NONE,
    );
    let anchors = bezier_draft(&h);
    assert_eq!(anchors.len(), 1);
    assert!(near(anchors[0].0, Pos2::ZERO));
    assert!((anchors[0].1.handle_out - EVec2::new(40.0, 0.0)).length() < 0.01);
    assert!((anchors[0].1.handle_in - EVec2::new(-40.0, 0.0)).length() < 0.01);
}

/// Bézier D12: Ctrl+Z while drawing removes the last placed anchor and
/// Ctrl+Y / Ctrl+Shift+Z re-adds it, without touching the document journal.
/// With no anchors left, Ctrl+Z exits drawing; document undo then resumes.
#[test]
fn bezier_ctrl_z_while_drawing_removes_anchors_without_journaling() {
    let mut h = bezier_board("bezier_draft_undo");
    let rect = add_rect(&mut h.app, 400.0, 300.0);
    let depth = h.app.tab().journal.undo_depth();
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    for p in pts {
        bezier_place(&mut h, p, p);
    }
    assert_eq!(bezier_draft(&h).len(), 3);
    let ctrl = egui::Modifiers::CTRL;
    let ctrl_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;

    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert_eq!(bezier_draft(&h).len(), 2, "Ctrl+Z removes the last anchor");
    press_key_with(&mut h, egui::Key::Y, ctrl);
    let anchors = bezier_draft(&h);
    assert_eq!(anchors.len(), 3, "Ctrl+Y re-adds it");
    assert!(near(anchors[2].0, pts[2]));
    press_key_with(&mut h, egui::Key::Z, ctrl);
    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert_eq!(bezier_draft(&h).len(), 1);
    press_key_with(&mut h, egui::Key::Z, ctrl_shift);
    assert_eq!(bezier_draft(&h).len(), 2, "Ctrl+Shift+Z re-adds too");
    press_key_with(&mut h, egui::Key::Z, ctrl);
    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert!(
        bezier_draft(&h).is_empty(),
        "drawing continues at zero anchors"
    );
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth,
        "draft undo never journals"
    );
    assert!(h.app.doc().scene.node(rect).is_some());

    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert!(h.app.board_path_draft.is_none(), "Ctrl+Z at zero exits");
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.doc().scene.node(rect).is_some());

    press_key_with(&mut h, egui::Key::Z, ctrl);
    assert!(
        h.app.doc().scene.node(rect).is_none(),
        "document undo resumes once drawing ends"
    );
}

/// Bézier D17: while drawing, a press on an existing anchor or handle knob
/// drags it instead of placing an anchor. Alt on a knob breaks symmetry.
#[test]
fn bezier_press_on_a_draft_anchor_or_handle_edits_it_instead_of_placing() {
    let mut h = bezier_board("bezier_draft_edit");
    let depth = h.app.tab().journal.undo_depth();
    bezier_place(&mut h, Pos2::ZERO, Pos2::new(40.0, 0.0));
    bezier_place(&mut h, Pos2::new(200.0, 0.0), Pos2::new(200.0, 0.0));
    assert_eq!(bezier_draft(&h).len(), 2);

    press_drag_release(
        &mut h,
        &[
            Pos2::new(200.0, 0.0),
            Pos2::new(200.0, 40.0),
            Pos2::new(200.0, 80.0),
        ],
        egui::Modifiers::NONE,
    );
    let a = bezier_draft(&h);
    assert_eq!(a.len(), 2, "pressing an anchor edits it; no third anchor");
    assert!(near(a[1].0, Pos2::new(200.0, 80.0)), "{:?}", a[1].0);

    press_drag_release(
        &mut h,
        &[
            Pos2::new(40.0, 0.0),
            Pos2::new(40.0, 30.0),
            Pos2::new(40.0, 60.0),
        ],
        egui::Modifiers::NONE,
    );
    let a = bezier_draft(&h);
    assert_eq!(a.len(), 2);
    assert!(near(a[0].0, Pos2::ZERO), "a handle drag leaves its anchor");
    assert!((a[0].1.handle_out - EVec2::new(40.0, 60.0)).length() < 0.01);
    let mirrored = -EVec2::new(40.0, 60.0).normalized() * 40.0;
    assert!(
        (a[0].1.handle_in - mirrored).length() < 0.01,
        "the opposite handle stays collinear at its own length: {:?}",
        a[0].1.handle_in
    );

    let in_knob = a[0].0 + a[0].1.handle_in;
    press_drag_release(
        &mut h,
        &[in_knob, Pos2::new(-10.0, -15.0), Pos2::new(-10.0, -30.0)],
        egui::Modifiers::ALT,
    );
    let a = bezier_draft(&h);
    assert_eq!(a.len(), 2);
    assert!((a[0].1.handle_in - EVec2::new(-10.0, -30.0)).length() < 0.01);
    assert!(
        (a[0].1.handle_out - EVec2::new(40.0, 60.0)).length() < 0.01,
        "Alt leaves the opposite handle alone"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.doc().scene.nodes.is_empty());
}

/// Bézier D13 / P1.curve.grips: a single-selected Bézier path shows its
/// anchors and handles; each drag is one journaled patch; Alt on a handle
/// breaks symmetry.
#[test]
fn bezier_single_selection_grips_edit_the_curve_one_patch_per_drag() {
    let mut h = bezier_board("bezier_select_grips");
    let (a, b, c) = (
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 80.0),
    );
    h.app.bezier_anchor_press(a);
    h.app.bezier_anchor_release(a, a, false);
    h.app.bezier_anchor_press(b);
    h.app
        .bezier_anchor_release(b, b + EVec2::new(40.0, 0.0), false);
    h.app.bezier_anchor_press(c);
    h.app.bezier_anchor_release(c, c, false);
    assert!(h.app.path_tool_try_finish());
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    let id = h.app.doc().scene.nodes[0].id;
    assert_eq!(h.app.board_sel.len(), 1);
    assert!(h.app.board_sel.contains(&id));
    h.frame();
    let depth = h.app.tab().journal.undo_depth();

    select_drag(&mut h, c, Pos2::new(200.0, 140.0), egui::Modifiers::NONE);
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    assert_eq!(anchors.len(), 3);
    assert!(
        near(kpt(anchors[0].point), a),
        "an anchor drag is not a move"
    );
    assert!(near(kpt(anchors[1].point), b));
    assert!(near(kpt(anchors[2].point), Pos2::new(200.0, 140.0)));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(h.app.board_sel.contains(&id));

    select_drag(
        &mut h,
        Pos2::new(140.0, 0.0),
        Pos2::new(140.0, 60.0),
        egui::Modifiers::NONE,
    );
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    let out = kpt(anchors[1].handle_out.unwrap());
    let inn = kpt(anchors[1].handle_in.unwrap());
    assert!(near(out, Pos2::new(140.0, 60.0)), "{out:?}");
    let mirrored = b - EVec2::new(40.0, 60.0).normalized() * 40.0;
    assert!(near(inn, mirrored), "symmetric: {inn:?} != {mirrored:?}");
    assert!(near(kpt(anchors[1].point), b));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

    select_drag(&mut h, inn, Pos2::new(80.0, -40.0), egui::Modifiers::ALT);
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    assert!(near(
        kpt(anchors[1].handle_in.unwrap()),
        Pos2::new(80.0, -40.0)
    ));
    assert!(
        near(kpt(anchors[1].handle_out.unwrap()), Pos2::new(140.0, 60.0)),
        "Alt breaks symmetry"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 3);
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "no duplicate on Alt");

    h.app.board_undo();
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    assert!(near(kpt(anchors[1].handle_in.unwrap()), mirrored));
}

/// Bézier D12: Esc with two or more anchors commits the open curve as exactly
/// one journaled add.
#[test]
fn bezier_escape_commits_an_open_curve_as_one_journaled_add() {
    let mut h = bezier_board("bezier_escape_commit");
    let depth = h.app.tab().journal.undo_depth();
    h.app.bezier_anchor_press(Pos2::ZERO);
    h.app.bezier_anchor_release(Pos2::ZERO, Pos2::ZERO, false);
    let b = Pos2::new(120.0, 0.0);
    h.app.bezier_anchor_press(b);
    h.app
        .bezier_anchor_release(b, Pos2::new(160.0, 20.0), false);
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.board_path_draft.is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "Esc commits the curve");
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        depth + 1,
        "one journaled add"
    );
    let NodeKind::Shape(s) = &h.app.doc().scene.nodes[0].kind else {
        panic!("a shape")
    };
    assert!(!s.path.as_ref().unwrap().closed, "an open curve");
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty());
}

/// Bézier D12: Esc with fewer than two anchors cancels.
#[test]
fn bezier_escape_with_one_anchor_cancels() {
    let mut h = bezier_board("bezier_escape_cancel");
    let depth = h.app.tab().journal.undo_depth();
    h.app.bezier_anchor_press(Pos2::ZERO);
    h.app.bezier_anchor_release(Pos2::ZERO, Pos2::ZERO, false);
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.board_path_draft.is_none());
    assert!(h.app.doc().scene.nodes.is_empty());
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// P1.wire.ports: open shapes offer no wire ports and are not wire targets;
/// a closed polyline keeps its ports.
#[test]
fn open_shapes_offer_no_wire_ports_while_a_closed_polyline_keeps_them() {
    let mut h = line_board("open_shape_ports");
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    let open_pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    let (r, d) = board_path::points_to_path_data(&open_pts, false);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Polyline, r, d, false);
    let open = h.app.doc().scene.nodes.last().unwrap().id;
    let tri = [
        Pos2::new(300.0, 0.0),
        Pos2::new(420.0, 0.0),
        Pos2::new(300.0, 120.0),
    ];
    let (r, d) = board_path::points_to_path_data(&tri, true);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Polyline, r, d, true);
    let closed = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();

    let open_node = h.app.doc().scene.node(open).unwrap().clone();
    for (side, t) in [
        (slate_doc::scene::Side::Start, 0.0),
        (slate_doc::scene::Side::Mid, 0.5),
        (slate_doc::scene::Side::End, 1.0),
    ] {
        let at = slate_doc::connector_anchor_on(&open_node, side, t);
        let screen = xf.w2s(Pos2::new(at[0], at[1]));
        assert_eq!(
            h.app.wire_grip_at(screen, &xf),
            None,
            "open polyline {side:?} must not offer a port"
        );
    }

    let closed_node = h.app.doc().scene.node(closed).unwrap().clone();
    let ports = h.app.wire_host(&closed_node).ports();
    assert_eq!(ports.len(), 3, "a closed polyline keeps its ports");
    for port in &ports {
        let screen = xf.w2s(Pos2::new(port.point[0], port.point[1]));
        assert_eq!(
            h.app.wire_grip_at(screen, &xf).map(|g| g.0),
            Some(closed),
            "closed polyline {:?}",
            port.side
        );
    }

    let start = Pos2::new(ports[0].point[0], ports[0].point[1]);
    let mut wd = h
        .app
        .try_begin_wire_drag(xf.w2s(start), start, egui::Modifiers::NONE)
        .expect("a closed polyline still starts a wire");
    h.app.wire_drag_update(&mut wd, open_pts[1], false);
    assert!(
        wd.snap.is_none_or(|(id, _, _)| id != open),
        "an open shape is not a wire target: {:?}",
        wd.snap
    );
}
