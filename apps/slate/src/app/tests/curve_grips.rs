//! Parametric curve grip tests.

use super::*;

/// User finding (2026-09-26), P1.curve.grips: reselecting a polyline exposes
/// its corner vertices and end points. Each drag moves that one point as one
/// journaled patch, and the path stays a line polyline.
#[test]
fn polyline_single_selection_grips_move_one_vertex_per_patch() {
    let mut h = grip_board("polyline_grips");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(120.0, 90.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let depth = h.app.tab().journal.undo_depth();

    let corner = Pos2::new(160.0, -30.0);
    select_drag(&mut h, pts[1], corner, egui::Modifiers::NONE);
    let got = world_anchor_points(&h, id);
    assert_eq!(got.len(), 3);
    assert!(near(got[0], pts[0]), "the other points stay: {got:?}");
    assert!(near(got[1], corner), "the corner vertex moved: {got:?}");
    assert!(near(got[2], pts[2]), "the other points stay: {got:?}");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);

    let end = Pos2::new(-20.0, 10.0);
    select_drag(&mut h, pts[0], end, egui::Modifiers::NONE);
    let got = world_anchor_points(&h, id);
    assert!(near(got[0], end), "an end point moves: {got:?}");
    assert!(near(got[1], corner), "{got:?}");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);
    let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("a shape")
    };
    assert!(
        slate_doc::geom::path_is_line_polyline(s.path.as_ref().unwrap()),
        "still a line polyline, so Corners still applies"
    );
    assert!(h.app.board_sel.contains(&id));

    h.app.board_undo();
    let got = world_anchor_points(&h, id);
    assert!(near(got[0], pts[0]), "undo restores one drag: {got:?}");
    assert!(near(got[1], corner));
}

/// A filled closed polyline shows its vertices too; a vertex that sits on
/// the bounding-box corner moves as a vertex, not as a resize.
#[test]
fn closed_polyline_grips_move_a_vertex_where_the_resize_corner_sits() {
    let mut h = grip_board("closed_polyline_grips");
    let tri = [
        Pos2::new(0.0, 0.0),
        Pos2::new(120.0, 0.0),
        Pos2::new(0.0, 90.0),
    ];
    let id = commit_polyline(&mut h, &tri, true);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.fill = Some(Rgba([200, 30, 30, 255]));
        }
    });
    h.frame();
    let depth = h.app.tab().journal.undo_depth();

    let moved = Pos2::new(-30.0, -20.0);
    select_drag(&mut h, tri[0], moved, egui::Modifiers::NONE);
    let got = world_anchor_points(&h, id);
    assert_eq!(got.len(), 3, "no duplicated seam vertex: {got:?}");
    assert!(near(got[0], moved), "{got:?}");
    assert!(near(got[1], tri[1]), "{got:?}");
    assert!(near(got[2], tri[2]), "{got:?}");
    let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("a shape")
    };
    assert!(s.path.as_ref().unwrap().closed);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}

/// User finding (2026-09-26): editing a filleted polyline re-applies the
/// authored radius to the new geometry, clamped per corner only.
#[test]
fn polyline_vertex_drag_keeps_the_authored_fillet_radius() {
    let mut h = grip_board("polyline_fillet_grips");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 0.0),
        Pos2::new(200.0, 200.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let corner = slate_doc::scene::Corner::Rounded { radius: 30.0 };
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.corner = corner;
        }
    });
    h.frame();

    let moved = Pos2::new(400.0, 40.0);
    select_drag(&mut h, pts[2], moved, egui::Modifiers::NONE);
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    assert_eq!(s.corner, corner, "the authored radius is stored unchanged");
    let drawn = slate_doc::geom::path_data_to_world_bez_with_fillet(
        s.path.as_ref().unwrap(),
        n.rect,
        n.rotation_deg,
        s.corner,
    );
    let want = cmds_bez(&slate_doc::filleted_vertex_path(
        &[[0.0, 0.0], [200.0, 0.0], [400.0, 40.0]],
        30.0,
        false,
        false,
    ));
    assert_bez_near(&drawn, &want);
}

/// User finding (2026-09-26): a reselected arc exposes its start, end and
/// through point; dragging one rebuilds the circular arc through the three.
#[test]
fn arc_single_selection_grips_edit_start_end_and_through_point() {
    let mut h = grip_board("arc_grips");
    h.app.set_board_tool(board::BoardTool::Arc);
    let (s, e, m) = (
        Pos2::new(0.0, 0.0),
        Pos2::new(200.0, 0.0),
        Pos2::new(100.0, 60.0),
    );
    for p in [s, e, m] {
        h.app.path_tool_click(p);
    }
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.board_sel.contains(&id));
    h.frame();
    let depth = h.app.tab().journal.undo_depth();

    let m2 = Pos2::new(100.0, 100.0);
    select_drag(&mut h, m, m2, egui::Modifiers::NONE);
    assert_circular_arc_through(&h, id, s, m2, e);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);

    let s2 = Pos2::new(-40.0, 20.0);
    select_drag(&mut h, s, s2, egui::Modifiers::NONE);
    assert_circular_arc_through(&h, id, s2, m2, e);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

    let e2 = Pos2::new(220.0, -30.0);
    select_drag(&mut h, e, e2, egui::Modifiers::NONE);
    let n = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Shape(shape) = &n.kind else {
        panic!("a shape")
    };
    let bez = board_path::path_data_to_world_bez(shape.path.as_ref().unwrap(), n.rect, 0.0);
    let (anchors, _) = vector_ink::anchors_from_bezpath(&bez);
    assert!(near(kpt(anchors[0].point), s2), "the start stays put");
    assert!(
        near(kpt(anchors.last().unwrap().point), e2),
        "the end moved"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 3);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

/// Line endpoints are painted and picked by the shared path-edit overlay:
/// the one 7 screen px pick radius, not a line-only radius.
#[test]
fn line_endpoint_grips_follow_the_shared_path_edit_hit_rule() {
    let mut h = grip_board("line_grip_hit");
    let id = h
        .app
        .commit_line(Pos2::new(0.0, 0.0), Pos2::new(200.0, 0.0))
        .unwrap();
    h.frame();
    let xf = h.app.board_xf();
    let end = xf.w2s(Pos2::new(200.0, 0.0));
    let r = super::super::path_edit_overlay::HIT_PX;
    assert_eq!(
        h.app.line_grip_at(id, end + EVec2::new(r - 0.5, 0.0), &xf),
        Some(1)
    );
    assert_eq!(
        h.app.line_grip_at(id, end + EVec2::new(r + 0.5, 0.0), &xf),
        None,
        "the shared pick radius"
    );
}
