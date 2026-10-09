//! Draft-tool boards and pointer hits.

use super::*;

/// Within 0.05 world units: pointer events round-trip through screen space.
pub(crate) fn near_px(a: Pos2, b: Pos2) -> bool {
    (a - b).length() < 0.05
}

/// `to - from` lies on a 45° step.
pub(crate) fn on_45(from: Pos2, to: Pos2) -> bool {
    let d = to - from;
    let step = std::f32::consts::FRAC_PI_4;
    let a = d.y.atan2(d.x) / step;
    d.length() > 1.0 && (a - a.round()).abs() < 1.0e-3
}

/// `p` lies on the ray from `origin` along `dir`.
pub(crate) fn on_ray(origin: Pos2, dir: EVec2, p: Pos2) -> bool {
    let d = p - origin;
    (d.x * dir.y - d.y * dir.x).abs() < 0.05 && d.dot(dir) > 0.0
}

/// Draft-tool board with snaps off at zoom 1.
pub(crate) fn draft_board(tag: &str, tool: board::BoardTool) -> Harness {
    let mut h = line_board(tag);
    h.app.set_board_tool(tool);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h.frame();
    h
}

pub(crate) fn hover_at(h: &mut Harness, world: Pos2, modifiers: egui::Modifiers) {
    let s = h.app.board_xf().w2s(world);
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerMoved(s));
    });
    h.frame_with(|i| i.modifiers = modifiers);
}

/// A click, a second after the last input so it is never a double click.
pub(crate) fn click_at(h: &mut Harness, world: Pos2, modifiers: egui::Modifiers) {
    let t = h.ctx.input(|i| i.time);
    h.frame_with(|i| {
        i.time = Some(t + 1.0);
        i.modifiers = modifiers;
    });
    press_drag_release(h, &[world], modifiers);
    h.frame_with(|i| i.modifiers = modifiers);
}

/// The last placed vertex of the armed vector draft tool.
pub(crate) fn last_vertex(h: &Harness) -> Option<Pos2> {
    match &h.app.board_path_draft {
        Some(board_path::BoardPathDraft::Polyline { points, .. })
        | Some(board_path::BoardPathDraft::Arc { points, .. }) => points.last().copied(),
        Some(board_path::BoardPathDraft::Bezier { anchors, .. }) => anchors.last().map(|a| a.0),
        None => None,
    }
}

pub(crate) const VERTEX_TOOLS: [board::BoardTool; 3] = [
    board::BoardTool::Polyline,
    board::BoardTool::Arc,
    board::BoardTool::BezierSpan,
];

pub(crate) fn pen_board(tag: &str) -> Harness {
    draft_board(tag, board::BoardTool::Pen)
}

/// Re-arm the Pen after a stroke (Pen D02: one-shot). A bare P waits out
/// the type-to-command hold window, so the harness arms it directly.
pub(crate) fn arm_pen(h: &mut Harness) {
    assert_eq!(
        h.app.board_tool,
        board::BoardTool::Select,
        "the Pen is one-shot"
    );
    h.app.set_board_tool(board::BoardTool::Pen);
    h.frame();
}
