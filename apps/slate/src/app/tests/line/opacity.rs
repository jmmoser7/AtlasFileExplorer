//! Shift-right-drag scrubs stroke opacity.

use super::*;

#[test]
fn shift_right_drag_scrubs_opacity_and_escape_restores() {
    let mut h = Harness::new("brush_opacity");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.shift_down = true;
    h.app.brush_opacity = 1.0;
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(10.0, 10.0)), true, false));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Opacity { .. })
    ));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(10.0, 60.0)), true, false));
    assert!((h.app.brush_opacity - 0.5).abs() < 0.02);
    h.app.cancel_brush_hud();
    assert!((h.app.brush_opacity - 1.0).abs() < 1e-4);
}
