//! Select-tool windows collect visible curve points without selecting other nodes.
use super::*;

#[test]
fn selected_curve_vertices_window_select_and_shift_add_without_scene_mutation() {
    let mut h = grip_board("selected_curve_vertex_window");
    let (id, pts) = hud_polyline(&mut h);
    h.app.set_board_tool(board::BoardTool::Select);
    let depth = h.app.tab().journal.undo_depth();
    let start = pts[0] - EVec2::splat(20.0);
    let end = pts[1] + EVec2::splat(20.0);
    grip_drag(&mut h, start, end, egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 1])));
    grip_drag(
        &mut h,
        pts[2] - EVec2::splat(20.0),
        pts[2] + EVec2::splat(20.0),
        egui::Modifiers::SHIFT,
    );
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 1, 2])));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn window_selected_vertices_drag_as_a_group_and_undo_once() {
    let mut h = grip_board("selected_curve_vertex_group_move");
    let (id, pts) = hud_polyline(&mut h);
    h.app.set_board_tool(board::BoardTool::Select);
    grip_drag(
        &mut h,
        pts[0] - EVec2::splat(20.0),
        pts[1] + EVec2::splat(20.0),
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 1])));
    let depth = h.app.tab().journal.undo_depth();
    let delta = EVec2::new(0.0, -50.0);
    grip_drag(&mut h, pts[0], pts[0] + delta, egui::Modifiers::NONE);
    let anchors = h.app.direct_anchors_of(id).unwrap().0;
    assert!(near_eps(kpt(anchors[0].point), pts[0] + delta, 0.01));
    assert!(near_eps(kpt(anchors[1].point), pts[1] + delta, 0.01));
    assert!(near_eps(kpt(anchors[2].point), pts[2], 0.01));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    let anchors = h.app.direct_anchors_of(id).unwrap().0;
    for (anchor, point) in anchors.iter().zip(pts) {
        assert!(near_eps(kpt(anchor.point), point, 0.01));
    }
}
