//! Object-snap golden-path tests.

use super::*;

/// GP1 — End: cursor near a rect corner snaps to that corner.
#[test]
fn osnap_gp1_end_on_rect_corner() {
    let mut h = osnap_board("osnap_gp1");
    add_rect(&mut h.app, 200.0, 80.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::End);
    let p = h
        .app
        .resolve_point_snap(Pos2::new(202.0, 82.0), &[], None, false, false);
    assert!((p.x - 200.0).abs() < 0.01 && (p.y - 80.0).abs() < 0.01);
    assert_eq!(
        h.app.board_osnap_hit.map(|hit| hit.kind),
        Some(slate_doc::SnapKind::End)
    );
    h.frame();
}

/// GP2 — Tangent is inert on the first pick (no prior point).
#[test]
fn osnap_gp2_tan_inert_on_first_pick() {
    let mut h = osnap_board("osnap_gp2");
    add_ellipse(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::Tangent);
    let cursor = Pos2::new(102.0, 50.0);
    let p = h.app.resolve_point_snap(cursor, &[], None, false, false);
    assert!((p.x - cursor.x).abs() < 0.01 && (p.y - cursor.y).abs() < 0.01);
    assert!(h.app.board_osnap_hit.is_none());
    h.frame();
}

/// GP3 — Tangent from a prior point onto a circle.
#[test]
fn osnap_gp3_tan_from_prior_point() {
    let mut h = osnap_board("osnap_gp3");
    add_ellipse(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::Tangent);
    let from = Pos2::new(200.0, 50.0);
    let pts = slate_doc::osnap::tangents_on_ellipse(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        0.0,
        [from.x, from.y],
    );
    assert_eq!(pts.len(), 2);
    let target = Pos2::new(pts[0][0], pts[0][1]);
    let cursor = Pos2::new(target.x + 1.0, target.y + 1.0);
    let p = h
        .app
        .resolve_point_snap(cursor, &[], Some(from), false, false);
    assert_eq!(
        h.app.board_osnap_hit.map(|hit| hit.kind),
        Some(slate_doc::SnapKind::Tangent)
    );
    let dx = p.x - 50.0;
    let dy = p.y - 50.0;
    let rad = (dx * dx + dy * dy).sqrt();
    assert!(
        (rad - 50.0).abs() < 0.8,
        "tangent snap left the circle: {p:?} r={rad}"
    );
    h.frame();
}

/// GP4 — master disable remembers End but does not fire.
#[test]
fn osnap_gp4_master_disable() {
    let mut h = osnap_board("osnap_gp4");
    add_rect(&mut h.app, 200.0, 80.0);
    h.app.board_osnap = only_kind(slate_doc::SnapKind::End);
    h.app.board_osnap.enabled = false;
    assert!(h
        .app
        .board_osnap
        .is_kind_remembered(slate_doc::SnapKind::End));
    let cursor = Pos2::new(202.0, 82.0);
    let p = h.app.resolve_point_snap(cursor, &[], None, false, false);
    assert!((p.x - cursor.x).abs() < 0.01 && (p.y - cursor.y).abs() < 0.01);
    assert!(h.app.board_osnap_hit.is_none());
    h.frame();
}

/// Armed GhostFollow: a point near an edge snaps and emits a forcefield.
#[test]
fn armed_rect_hover_snaps_and_emits_forcefield() {
    let mut h = osnap_board("armed_hover_snap");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = true;
    add_rect(&mut h.app, 200.0, 80.0);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let p = h
        .app
        .resolve_point_snap(Pos2::new(203.0, 110.0), &[], None, false, false);
    assert!(
        (p.x - 200.0).abs() < 0.01,
        "armed hotspot should snap to the edge, got {p:?}"
    );
    assert!(
        !h.app.board_snap_guides.is_empty(),
        "GhostFollow hover must emit a forcefield"
    );
    assert_eq!(h.app.board_point_snap, Some(p));
    assert_eq!(h.app.preview_snap_point(Pos2::new(203.0, 110.0)), p);
}

/// DragScale second corner: snap the live rect, not a 0-size cursor that
/// has left the target's row (the forcefield used to go quiet).
#[test]
fn draw_rect_second_corner_emits_forcefield() {
    let mut h = osnap_board("draw_rect_second_corner");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = true;
    add_rect(&mut h.app, 200.0, 0.0);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    let end = Pos2::new(197.0, 280.0);
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(10.0, 10.0), start, Default::default());
    h.app.update_gesture_for_test(end, Default::default());
    assert!(
        !h.app.board_snap_guides.is_empty(),
        "second corner must emit a forcefield even when the cursor left the row"
    );
    let r = h.app.board_draw_rect.expect("live draw rect");
    assert!(
        (r.x + r.w - 200.0).abs() < 0.5,
        "right edge should snap to 200, got {}",
        r.x + r.w
    );
}

/// Shift during DragScale is aspect, not ortho — smart guides still fire.
#[test]
fn draw_rect_shift_does_not_silence_forcefield() {
    let mut h = osnap_board("draw_rect_shift");
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = true;
    h.app.board_ortho = true;
    add_rect(&mut h.app, 200.0, 0.0);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let start = Pos2::new(0.0, 0.0);
    let end = Pos2::new(197.0, 280.0);
    h.app.board_drag =
        h.app
            .begin_gesture_for_test(Pos2::new(10.0, 10.0), start, Default::default());
    let mods = egui::Modifiers {
        shift: true,
        ..Default::default()
    };
    h.app.update_gesture_for_test(end, mods);
    assert!(
        !h.app.board_snap_guides.is_empty(),
        "Shift+square must not take the ortho path and skip forcefield"
    );
}

/// Every DragRect tool shares `resolve_draw_rect` (ellipse, frame, portals).
#[test]
fn every_drag_rect_tool_second_corner_snaps() {
    for tool in board::BoardTool::ALL {
        if !tool.places_by_drag_rect() {
            continue;
        }
        let mut h = osnap_board(&format!("draw_{tool:?}_snap"));
        h.app.board_osnap.enabled = false;
        h.app.board_smart_guides = true;
        add_rect(&mut h.app, 200.0, 0.0);
        h.app.set_board_tool(tool);
        let r = h.app.resolve_draw_rect(
            Pos2::new(0.0, 0.0),
            Pos2::new(197.0, 280.0),
            tool,
            false,
            false,
        );
        assert!(
            !h.app.board_snap_guides.is_empty(),
            "{tool:?} second corner must emit a forcefield"
        );
        assert!(
            (r.x + r.w - 200.0).abs() < 0.5,
            "{tool:?} right edge should snap to 200, got {}",
            r.x + r.w
        );
    }
}
