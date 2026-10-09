//! Committed pen, arc, and taper strokes.

use super::*;

pub(crate) fn shape_of(app: &SlateApp, id: NodeId) -> slate_doc::scene::ShapeNode {
    match &app.doc().scene.node(id).expect("node").kind {
        NodeKind::Shape(s) => s.clone(),
        other => panic!("expected a shape, got {other:?}"),
    }
}

/// The stroke of every node the vector tools commit, in commit order.
pub(crate) fn committed_vector_strokes(
    h: &mut Harness,
) -> Vec<(board::BoardTool, slate_doc::scene::Stroke)> {
    use board::BoardTool;
    let mut out = Vec::new();
    let mut last = |h: &mut Harness, tool: BoardTool| {
        let node = h.app.doc().scene.nodes.last().unwrap();
        let slate_doc::scene::NodeKind::Shape(s) = &node.kind else {
            panic!("{tool:?} commits a shape");
        };
        out.push((tool, s.stroke));
    };
    h.app.set_board_tool(BoardTool::Pen);
    h.app.finish_freehand_pen(vec![
        Pos2::new(0.0, 100.0),
        Pos2::new(40.0, 120.0),
        Pos2::new(80.0, 100.0),
    ]);
    last(h, BoardTool::Pen);
    h.app.set_board_tool(BoardTool::Line);
    h.app
        .commit_line(Pos2::new(0.0, 200.0), Pos2::new(90.0, 200.0));
    last(h, BoardTool::Line);
    h.app.set_board_tool(BoardTool::Arc);
    for p in [(0.0, 300.0), (100.0, 300.0), (50.0, 260.0)] {
        h.app.path_tool_click(Pos2::new(p.0, p.1));
    }
    last(h, BoardTool::Arc);
    h.app.set_board_tool(BoardTool::Polyline);
    for p in [(0.0, 400.0), (60.0, 430.0), (120.0, 400.0)] {
        h.app.path_tool_click(Pos2::new(p.0, p.1));
    }
    assert!(h.app.finish_path_draft());
    last(h, BoardTool::Polyline);
    h.app.set_board_tool(BoardTool::BezierSpan);
    for (press, release) in [
        ((0.0, 500.0), (30.0, 480.0)),
        ((120.0, 500.0), (150.0, 520.0)),
    ] {
        let press = Pos2::new(press.0, press.1);
        h.app.bezier_anchor_press(press);
        h.app
            .bezier_anchor_release(press, Pos2::new(release.0, release.1), false);
    }
    assert!(h.app.finish_path_draft());
    last(h, BoardTool::BezierSpan);
    out
}

pub(crate) fn last_stroke(h: &Harness) -> (NodeId, slate_doc::scene::Stroke) {
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(s) = &node.kind else {
        panic!("expected a shape");
    };
    (node.id, s.stroke)
}

pub(crate) fn restyle(h: &mut Harness, id: NodeId, width: f32, rgb: [u8; 3]) {
    h.app.patch_nodes(&[id], |n| {
        if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
            s.stroke.width = width;
            s.stroke.color = Rgba([rgb[0], rgb[1], rgb[2], 255]);
        }
    });
}

pub(crate) fn draw_pen(h: &mut Harness, y: f32) {
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.finish_freehand_pen(vec![
        Pos2::new(0.0, y),
        Pos2::new(40.0, y + 20.0),
        Pos2::new(80.0, y),
    ]);
}

pub(crate) fn draw_arc(h: &mut Harness, y: f32) {
    h.app.set_board_tool(board::BoardTool::Arc);
    for p in [(0.0, y), (100.0, y), (50.0, y - 40.0)] {
        h.app.path_tool_click(Pos2::new(p.0, p.1));
    }
}

pub(crate) const STROKE_TOOLS: [(board::BoardTool, slate_doc::StrokeTool); 5] = [
    (board::BoardTool::Pen, slate_doc::StrokeTool::Pen),
    (board::BoardTool::Line, slate_doc::StrokeTool::Line),
    (board::BoardTool::Arc, slate_doc::StrokeTool::Arc),
    (board::BoardTool::Polyline, slate_doc::StrokeTool::Polyline),
    (board::BoardTool::BezierSpan, slate_doc::StrokeTool::Bezier),
];

/// Open the brush's size HUD with Alt+right-drag and scrub right by `dx`.
pub(crate) fn width_chord(h: &mut Harness, dx: f32) {
    h.app.alt_down = true;
    assert!(
        h.app
            .drive_brush_hud(Some(Pos2::new(500.0, 500.0)), true, true),
        "{:?} opens the size HUD",
        h.app.board_tool
    );
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Size { .. })
    ));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(500.0 + dx, 450.0)), true, false));
}

pub(crate) fn release_chord(h: &mut Harness) {
    assert!(h.app.drive_brush_hud(None, false, false));
    assert!(h.app.brush_hud.is_none());
    h.app.alt_down = false;
}

pub(crate) fn taper_board(tag: &str, tool: board::BoardTool) -> Harness {
    let mut h = grip_board(tag);
    h.app.set_board_tool(tool);
    h.frame();
    h
}

pub(crate) fn pen_point_count(h: &Harness) -> usize {
    match &h.app.board_drag {
        Some(board::BoardDrag::FreehandPen { stroke }) => stroke.points.len(),
        _ => panic!("the pen stroke is live"),
    }
}
