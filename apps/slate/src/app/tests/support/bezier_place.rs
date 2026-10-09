//! Bezier draft placement.

use super::*;

pub(crate) fn bezier_board(tag: &str) -> Harness {
    let mut h = line_board(tag);
    h.app.set_board_tool(board::BoardTool::BezierSpan);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h.frame();
    h
}

pub(crate) fn bezier_draft(h: &Harness) -> Vec<(Pos2, board_path::BezierHandles)> {
    match &h.app.board_path_draft {
        Some(board_path::BoardPathDraft::Bezier { anchors, .. }) => anchors.clone(),
        other => panic!("expected a Bézier draft, got {other:?}"),
    }
}

/// One frame of ordered pointer events: press at the first world point, move
/// through the rest, release at the last.
pub(crate) fn press_drag_release(h: &mut Harness, world: &[Pos2], modifiers: egui::Modifiers) {
    let xf = h.app.board_xf();
    let pts: Vec<Pos2> = world.iter().map(|w| xf.w2s(*w)).collect();
    let (first, last) = (pts[0], *pts.last().unwrap());
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(first)));
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
        for p in &pts[1..] {
            i.events.push(egui::Event::PointerMoved(*p));
        }
        i.events.push(egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
    });
}

pub(crate) fn bezier_click(h: &mut Harness, world: Pos2) {
    press_drag_release(h, &[world], egui::Modifiers::NONE);
}

/// Place one draft anchor without pointer routing (press, then release).
pub(crate) fn bezier_place(h: &mut Harness, press: Pos2, release: Pos2) {
    h.app.bezier_anchor_press(press);
    h.app.bezier_anchor_release(press, release, false);
}

/// A Select-tool drag through the board's gesture entry points: begin at the
/// press origin, one live update, end at the release.
pub(crate) fn select_drag(h: &mut Harness, from: Pos2, to: Pos2, modifiers: egui::Modifiers) {
    let xf = h.app.board_xf();
    h.app.board_drag = h.app.begin_gesture_for_test(xf.w2s(from), from, modifiers);
    h.app.update_gesture_for_test(to, modifiers);
    h.app.end_gesture_for_test(to, Some(xf.w2s(to)), modifiers);
    h.frame();
}

// ---------- Line tool golden paths (contracts/line.md GP1–GP6) ----------
