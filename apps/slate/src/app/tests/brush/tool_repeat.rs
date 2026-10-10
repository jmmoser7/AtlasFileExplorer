//! Space repeats the latest tool, not a brush stroke.

use super::*;

#[test]
fn space_repeats_the_latest_tool_not_a_brush_stroke() {
    let mut h = Harness::new("repeat_tool");
    h.app.set_board_tool(board::BoardTool::Line);
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.finish_freehand_brush(vec![
        Pos2::new(0.0, 0.0),
        Pos2::new(20.0, 8.0),
        Pos2::new(40.0, 0.0),
    ]);
    h.app.set_board_tool(board::BoardTool::RectShape);
    let last = h
        .app
        .cmd_history
        .last_repeatable(&h.app.registry)
        .expect("a tool was armed");
    assert_eq!(last.0, "board.tool.rect");
}
