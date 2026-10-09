//! Line boards and the shift-from-press check.

use super::*;

pub(crate) fn line_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Line);
    h
}

/// Shift-drag from `press` and assert the segment starts there, with no
/// stroke to extend, as one new node; every node already there is kept.
pub(crate) fn assert_shift_starts_at_the_press(h: &mut Harness, press: Pos2) {
    let before = h.app.doc().scene.nodes.clone();
    let raw = press + egui::vec2(120.0, 10.0);
    press_drag_release_frames(
        h,
        &[press, press + egui::vec2(60.0, 10.0), raw],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, _, node) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(
                near_px(from, press),
                "starts at {from:?}, the press is {press:?}"
            );
            assert_eq!(node, None, "nothing to extend");
        },
    );
    let nodes = &h.app.doc().scene.nodes;
    assert_eq!(nodes.len(), before.len() + 1, "one new stroke");
    assert_eq!(
        &nodes[..before.len()],
        &before[..],
        "every other node is unchanged"
    );
    let v = path_vertices(nodes.last().unwrap());
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(near_px(v[0], press), "{v:?}");
}
