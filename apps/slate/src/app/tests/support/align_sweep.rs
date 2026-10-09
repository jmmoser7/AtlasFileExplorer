//! Align boards and marquee sweeps.

use super::*;

pub(crate) fn align_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    h
}

pub(crate) fn select_ids(app: &mut SlateApp, ids: &[NodeId]) {
    app.board_sel = ids.iter().copied().collect();
}

pub(crate) fn sweep(h: &mut Harness, from: Pos2, to: Pos2, mods: egui::Modifiers) {
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::vec2(0.0, 0.0);
    let xf = h.app.board_xf();
    let drag = h.app.begin_gesture_for_test(xf.w2s(from), from, mods);
    let kind = match &drag {
        None => "none",
        Some(board::BoardDrag::Marquee { .. }) => "marquee",
        Some(board::BoardDrag::Move { .. }) => "move",
        Some(board::BoardDrag::LineGrip { .. }) => "line-grip",
        Some(board::BoardDrag::Wire(_)) => "wire",
        Some(board::BoardDrag::Resize { .. }) => "resize",
        Some(board::BoardDrag::GroupResize { .. }) => "group-resize",
        Some(board::BoardDrag::Draw { .. }) => "draw",
        Some(board::BoardDrag::Direct(_)) => "direct",
        Some(_) => "other",
    };
    let hit = h.app.board_pick_node(from.x, from.y);
    let rect = hit.and_then(|id| h.app.doc().scene.node(id).map(|n| (n.id, n.rect)));
    assert!(
        matches!(drag, Some(board::BoardDrag::Marquee { .. })),
        "sweep must start on empty board, got {kind} pick={rect:?} at {from:?} z={}",
        h.app.tab().cam.z
    );
    h.app.board_drag = drag;
    h.app.end_gesture_for_test(to, Some(xf.w2s(to)), mods);
}

// ---------- Trim golden paths (contracts/trim.md GP1–GP6) ----------
