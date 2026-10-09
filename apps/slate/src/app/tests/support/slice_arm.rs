//! Arm trim or split on a selection.

use super::*;

pub(crate) fn trim_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h
}

pub(crate) fn select_trim(app: &mut SlateApp, ids: &[NodeId]) {
    app.board_sel.clear();
    for id in ids {
        app.board_sel.insert(*id);
    }
}

// ---------- Join golden paths (contracts/join.md GP1–GP6) ----------

/// An open cubic arch from (-20, 40) over y = 17.5 to (120, 40).
pub(crate) fn add_bezier_arch(app: &mut SlateApp) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let mut bez = vector_ink::kurbo::BezPath::new();
    bez.move_to((-20.0, 40.0));
    bez.curve_to((10.0, 10.0), (90.0, 10.0), (120.0, 40.0));
    let (rect, path) = board_path::bezpath_to_path_data(&bez, false);
    let node = app.doc_mut().scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(path.into()),
            text: None,
        }),
    );
    app.add_nodes(vec![node])[0]
}

/// Select `cutters` only, then arm Trim (Ctrl+T) or Split (Ctrl+Shift+T)
/// by key event.
pub(crate) fn arm_slice_by_key(h: &mut Harness, cutters: &[NodeId], split: bool) {
    select_trim(&mut h.app, cutters);
    h.frame();
    let mods = if split {
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT
    } else {
        egui::Modifiers::CTRL
    };
    press_key_with(h, egui::Key::T, mods);
    let want = if split {
        board::BoardTool::Split
    } else {
        board::BoardTool::Trim
    };
    assert_eq!(h.app.board_tool, want);
    let armed = h.app.trim.as_ref().expect("armed");
    assert_eq!(
        armed.phase,
        super::super::super::board_trim::TrimPhase::TrimParts
    );
    assert_eq!(armed.cutters, cutters);
}
