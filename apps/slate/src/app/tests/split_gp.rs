//! Split golden-path tests.

use super::*;

/// GP1 — crossing lines: click the horizontal; both halves stay.
#[test]
fn split_gp1_crossing_lines() {
    let mut h = trim_board("split_gp1");
    let horiz = add_seg(&mut h.app, Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0));
    let vert = add_seg(&mut h.app, Pos2::new(50.0, 0.0), Pos2::new(50.0, 100.0));
    select_trim(&mut h.app, &[horiz, vert]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert_eq!(h.app.board_tool, board::BoardTool::Split);
    assert!(h.app.trim.as_ref().is_some_and(|s| {
        s.mode == super::super::board_trim::SliceMode::Split && s.cutters.len() == 2
    }));
    assert!(h.app.trim_click(Pos2::new(25.0, 50.0), false));
    assert_eq!(h.app.doc().scene.nodes.len(), 3, "two halves + vertical");
    assert!(h.app.doc().scene.node(vert).is_some());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

/// GP2 — rect + vertical line: click the rect; both halves stay filled.
#[test]
fn split_gp2_rect_cut_by_line() {
    let mut h = trim_board("split_gp2");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 80.0);
    let line = add_seg(&mut h.app, Pos2::new(50.0, -10.0), Pos2::new(50.0, 90.0));
    select_trim(&mut h.app, &[rect, line]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert!(h.app.trim_click(Pos2::new(20.0, 40.0), false));
    assert_eq!(h.app.doc().scene.nodes.len(), 3, "two halves + line");
    assert!(closed_shape_contains(&h.app, [20.0, 40.0]));
    assert!(closed_shape_contains(&h.app, [80.0, 40.0]));
    let closed_paths: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                &n.kind,
                slate_doc::scene::NodeKind::Shape(s)
                    if s.shape == slate_doc::scene::ShapeKind::Path
                        && s.path.as_ref().is_some_and(|p| p.closed)
            )
        })
        .map(|n| n.id)
        .collect();
    for id in closed_paths {
        assert_axis_aligned_world(&node_world_contours(&h.app, id));
    }
    h.frame();
}

/// GP3 — circle inside a rect: click the rect; disk and ring both remain.
#[test]
fn split_gp3_hole_keeps_disk_and_ring() {
    let mut h = trim_board("split_gp3");
    let rect = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    let circle = add_filled_ellipse(&mut h.app, 30.0, 30.0, 40.0, 40.0);
    select_trim(&mut h.app, &[rect, circle]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert!(h.app.trim_click(Pos2::new(10.0, 10.0), false));
    assert!(h.app.doc().scene.node(circle).is_some(), "cutter remains");
    assert!(
        closed_shape_contains(&h.app, [50.0, 50.0]),
        "disk must remain"
    );
    assert!(
        closed_shape_contains(&h.app, [5.0, 5.0]),
        "outer ring must remain"
    );
    h.frame();
}

/// GP4 — Ctrl+Shift+T arms Split; Ctrl+N still opens a tab.
#[test]
fn split_gp4_ctrl_n_still_new_tab() {
    let mut h = trim_board("split_gp4");
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.tool.split"), None);
    assert_eq!(h.app.board_tool, board::BoardTool::Split);
    let before = h.app.tabs.len();
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.new_tab"), None);
    assert_eq!(h.app.tabs.len(), before + 1);
    h.frame();
}

/// GP5 — two clicks, one undo restores only the last.
#[test]
fn split_gp5_per_click_undo() {
    let mut h = trim_board("split_gp5");
    let a = add_seg(&mut h.app, Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0));
    let b = add_seg(&mut h.app, Pos2::new(0.0, 40.0), Pos2::new(100.0, 40.0));
    let c = add_seg(&mut h.app, Pos2::new(50.0, -10.0), Pos2::new(50.0, 50.0));
    select_trim(&mut h.app, &[a, b, c]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert!(h.app.trim_click(Pos2::new(25.0, 0.0), false));
    let after_first = h.app.doc().scene.nodes.len();
    assert!(h.app.trim_click(Pos2::new(25.0, 40.0), false));
    assert!(h.app.doc().scene.nodes.len() > after_first);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), after_first);
    h.frame();
}

/// GP6 — Esc peels TrimParts → PickCutters → Select.
#[test]
fn split_gp6_esc_stack() {
    let mut h = trim_board("split_gp6");
    let cutter = add_filled_rect(&mut h.app, 0.0, 0.0, 40.0, 40.0);
    h.app.set_board_tool(board::BoardTool::Split);
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::super::board_trim::TrimPhase::PickCutters
    );
    assert!(h.app.trim_click(Pos2::new(20.0, 20.0), false));
    assert!(h.app.trim.as_ref().unwrap().cutters.contains(&cutter));
    h.app.trim_enter();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::super::board_trim::TrimPhase::TrimParts
    );
    h.app.trim_cancel_step();
    assert_eq!(
        h.app.trim.as_ref().unwrap().phase,
        super::super::board_trim::TrimPhase::PickCutters
    );
    h.app.trim_cancel_step();
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert!(h.app.trim.is_none());
    h.frame();
}
