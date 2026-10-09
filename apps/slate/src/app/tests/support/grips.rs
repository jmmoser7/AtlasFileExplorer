//! Grip boards and polyline commits.

use super::*;

pub(crate) fn press_key_with(h: &mut Harness, key: egui::Key, modifiers: egui::Modifiers) {
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
    });
    h.frame();
}

pub(crate) fn near(a: Pos2, b: Pos2) -> bool {
    (a - b).length() < 0.01
}

pub(crate) fn kpt(p: vector_ink::kurbo::Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

// ---------- parametric grips on committed curves (P1.curve.grips) ----------

pub(crate) fn grip_board(tag: &str) -> Harness {
    let mut h = line_board(tag);
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h
}

pub(crate) fn commit_polyline(h: &mut Harness, pts: &[Pos2], closed: bool) -> NodeId {
    let (r, d) = board_path::points_to_path_data(pts, closed);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Polyline, r, d, closed);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert!(
        h.app.board_sel.contains(&id),
        "a committed curve is selected"
    );
    h.frame();
    id
}

pub(crate) fn world_anchor_points(h: &Harness, id: NodeId) -> Vec<Pos2> {
    let (anchors, _) = h.app.direct_anchors_of(id).unwrap();
    anchors.iter().map(|a| kpt(a.point)).collect()
}
