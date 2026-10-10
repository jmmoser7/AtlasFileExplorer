//! Handle modifiers, anchor delete, and direct-select marquee.

use super::*;

/// User, 28 September 2026: "ctrl lmb to scale handels on both sides of
/// controle point". Ctrl+drag that doubles the dragged handle doubles the
/// opposite one; it stays collinear on a smooth anchor. Ctrl+Shift also
/// locks the direction. Esc mid-drag restores.
#[test]
fn ctrl_drag_of_a_handle_scales_both_handles_of_its_anchor() {
    let mut h = bezier_board("ctrl_drag_handle_scale");
    let (id, [_, b, _]) = handle_bezier(&mut h);
    let grab = EVec2::new(3.0, 0.0);
    let knob = b + EVec2::new(40.0, 0.0);
    let depth = h.app.tab().journal.undo_depth();
    grip_drag(
        &mut h,
        knob + grab,
        b + EVec2::new(80.0, 0.0) + grab,
        egui::Modifiers::CTRL,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(80.0, 0.0), 0.01), "{hout:?}");
    assert!(
        near_eps(hin, b - EVec2::new(80.0, 0.0), 0.01),
        "doubling one handle doubles the other: {hin:?}"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");

    // Rotating while scaling: the opposite stays collinear at 2x.
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    grip_drag(
        &mut h,
        knob + grab,
        b + EVec2::new(0.0, 80.0) + grab,
        egui::Modifiers::CTRL,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(0.0, 80.0), 0.01), "{hout:?}");
    assert!(near_eps(hin, b - EVec2::new(0.0, 80.0), 0.01), "{hin:?}");

    // Ctrl+Shift: both lengths scale by 1.5, both directions locked.
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    let ctrl_shift = egui::Modifiers::CTRL | egui::Modifiers::SHIFT;
    grip_drag(
        &mut h,
        knob + grab,
        b + EVec2::new(60.0, 30.0) + grab,
        ctrl_shift,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(60.0, 0.0), 0.01), "{hout:?}");
    assert!(near_eps(hin, b - EVec2::new(60.0, 0.0), 0.01), "{hin:?}");

    // Esc mid-drag puts both handles back and journals nothing.
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let (_, out) = handle_pair(&h, id, 1);
    let to = b + EVec2::new(120.0, 0.0);
    hold_drag(&mut h, out + grab, to + grab, egui::Modifiers::CTRL);
    assert_ne!(scene_nodes(&h), before, "the drag is live");
    escape_then_release(&mut h, to + grab, egui::Modifiers::NONE);
    assert_eq!(scene_nodes(&h), before, "Esc restores both handles");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// A Ctrl drag on a corner anchor's handle scales the opposite handle
/// along its own direction.
#[test]
fn ctrl_drag_on_a_corner_anchor_keeps_the_opposite_direction() {
    let mut h = bezier_board("ctrl_drag_corner_handle");
    let (id, [_, b, _]) = handle_bezier(&mut h);
    let grab = EVec2::new(3.0, 0.0);
    // Alt breaks the anchor into a corner with the out handle pointing down.
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0) + grab,
        b + EVec2::new(0.0, 40.0) + grab,
        egui::Modifiers::ALT,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(0.0, 40.0), 0.01), "{hout:?}");
    assert!(near_eps(hin, b - EVec2::new(40.0, 0.0), 0.01), "{hin:?}");
    grip_drag(
        &mut h,
        hout + grab,
        b + EVec2::new(0.0, 20.0) + grab,
        egui::Modifiers::CTRL,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(0.0, 20.0), 0.01), "{hout:?}");
    assert!(
        near_eps(hin, b - EVec2::new(20.0, 0.0), 0.01),
        "halved along its own direction: {hin:?}"
    );
}

/// pm3 (user pass, 28 September 2026): a plain handle drag mirrors the
/// opposite handle's direction at its own length; Alt+drag moves only the
/// dragged handle.
#[test]
fn a_plain_handle_drag_mirrors_and_alt_moves_only_the_dragged_handle() {
    let mut h = bezier_board("handle_mirror_alt");
    let (id, [_, b, _]) = handle_bezier(&mut h);
    let grab = EVec2::new(3.0, 0.0);
    let depth = h.app.tab().journal.undo_depth();
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0) + grab,
        b + EVec2::new(0.0, 60.0) + grab,
        egui::Modifiers::NONE,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(0.0, 60.0), 0.01), "{hout:?}");
    assert!(
        near_eps(hin, b - EVec2::new(0.0, 40.0), 0.01),
        "mirrored at its own length: {hin:?}"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);

    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0) + grab,
        b + EVec2::new(0.0, 60.0) + grab,
        egui::Modifiers::ALT,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(0.0, 60.0), 0.01), "{hout:?}");
    assert!(
        near_eps(hin, b - EVec2::new(40.0, 0.0), 0.01),
        "Alt leaves the opposite handle: {hin:?}"
    );
    let kind = h.app.direct_anchors_of(id).unwrap().0[1].kind;
    assert_eq!(kind, vector_ink::AnchorKind::Corner, "Alt makes a corner");
}

/// Direct Select (A) reads the same handle modifiers as the Select grips.
#[test]
fn direct_select_handle_drags_take_shift_and_ctrl() {
    let mut h = bezier_board("direct_handle_modifiers");
    let (id, [_, b, _]) = handle_bezier(&mut h);
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.app.direct.anchors = [1].into_iter().collect();
    h.frame();
    let grab = EVec2::new(3.0, 0.0);
    grip_drag(
        &mut h,
        b + EVec2::new(40.0, 0.0) + grab,
        b + EVec2::new(80.0, 0.0) + grab,
        egui::Modifiers::CTRL,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(80.0, 0.0), 0.01), "{hout:?}");
    assert!(near_eps(hin, b - EVec2::new(80.0, 0.0), 0.01), "{hin:?}");
    grip_drag(
        &mut h,
        hout + grab,
        b + EVec2::new(50.0, 40.0) + grab,
        egui::Modifiers::SHIFT,
    );
    let (hin, hout) = handle_pair(&h, id, 1);
    assert!(near_eps(hout, b + EVec2::new(50.0, 0.0), 0.01), "{hout:?}");
    assert!(near_eps(hin, b - EVec2::new(80.0, 0.0), 0.01), "{hin:?}");
    assert_eq!(h.app.direct.anchors, [1].into_iter().collect());
}

/// pm4 × the handle modifiers: a draft handle takes Ctrl (scale both) and
/// Shift (lock direction) while the span is still being drawn.
#[test]
fn draft_bezier_handles_take_shift_and_ctrl() {
    let mut h = bezier_board("draft_handle_modifiers");
    bezier_place(&mut h, Pos2::ZERO, Pos2::new(40.0, 0.0));
    bezier_place(&mut h, Pos2::new(200.0, 80.0), Pos2::new(200.0, 80.0));
    let grab = EVec2::new(3.0, 0.0);
    let a = bezier_draft(&h);
    let in0 = a[0].1.handle_in;
    assert!(in0.length() > 1.0, "the first anchor has an in handle");
    press_drag_release(
        &mut h,
        &[
            Pos2::new(40.0, 0.0) + grab,
            Pos2::new(60.0, 0.0) + grab,
            Pos2::new(80.0, 0.0) + grab,
        ],
        egui::Modifiers::CTRL,
    );
    let a = bezier_draft(&h);
    assert!((a[0].1.handle_out - EVec2::new(80.0, 0.0)).length() < 0.01);
    assert!(
        (a[0].1.handle_in - in0 * 2.0).length() < 0.01,
        "the opposite draft handle doubles: {:?}",
        a[0].1.handle_in
    );
    press_drag_release(
        &mut h,
        &[
            Pos2::new(80.0, 0.0) + grab,
            Pos2::new(90.0, 20.0) + grab,
            Pos2::new(100.0, 40.0) + grab,
        ],
        egui::Modifiers::SHIFT,
    );
    let a = bezier_draft(&h);
    assert!(
        (a[0].1.handle_out - EVec2::new(100.0, 0.0)).length() < 0.01,
        "Shift keeps the draft handle's direction: {:?}",
        a[0].1.handle_out
    );
    assert_eq!(a.len(), 2, "no anchor added");
    assert!(h.app.doc().scene.nodes.is_empty(), "still drafting");
}

/// tip32 (user: "selection of single vrtecie and delession delete ful
/// curve"): Delete with a Select-tool grip pick removes that vertex and
/// rejoins its neighbors; tip33: too few left removes the curve. One
/// Ctrl+Z each.
#[test]
fn delete_with_select_tool_grip_picks_removes_those_vertices() {
    let mut h = grip_board("delete_select_grips");
    let (id, pts) = hud_polyline(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(pts[1]), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1])));
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert!(h.app.doc().scene.node(id).is_some(), "the curve stays");
    assert_eq!(
        world_anchor_points(&h, id),
        vec![pts[0], pts[2]],
        "neighbors rejoin"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        world_anchor_points(&h, id),
        pts.to_vec(),
        "one Ctrl+Z restores"
    );

    // Two picks on a three-vertex open curve leave one: the curve goes.
    h.app.board_sel = [id].into_iter().collect();
    h.frame();
    press_primary(&mut h, xf.w2s(pts[0]), egui::Modifiers::NONE);
    press_primary(&mut h, xf.w2s(pts[2]), egui::Modifiers::SHIFT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 2])));
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert!(
        h.app.doc().scene.node(id).is_none(),
        "too few vertices removes it"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        world_anchor_points(&h, id),
        pts.to_vec(),
        "one Ctrl+Z brings it back"
    );
}

/// tip32/tip33 on Bézier spans and Pen paths, under both tools.
#[test]
fn delete_picked_anchors_on_bezier_and_pen_paths_under_both_tools() {
    let mut h = bezier_board("delete_bezier_pen");
    let (bez, [a, _, c]) = handle_bezier(&mut h);
    h.app.board_sel = [bez].into_iter().collect();
    h.frame();
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(Pos2::new(100.0, 0.0)), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((bez, vec![1])));
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    let left = world_anchor_points(&h, bez);
    assert_eq!(left.len(), 2, "Select: one anchor gone");
    assert!(near(left[0], a) && near(left[1], c));
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(world_anchor_points(&h, bez).len(), 3);

    // A Pen path of four cubic anchors.
    let mut pen = vector_ink::kurbo::BezPath::new();
    pen.move_to((0.0, 200.0));
    pen.curve_to((30.0, 170.0), (60.0, 170.0), (90.0, 200.0));
    pen.curve_to((120.0, 230.0), (150.0, 230.0), (180.0, 200.0));
    pen.curve_to((210.0, 170.0), (240.0, 170.0), (270.0, 200.0));
    let (r, d) = board_path::bezpath_to_path_data(&pen, false);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Pen, r, d, false);
    let pen_id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(pen_id));
    h.app.direct.anchors = [1, 2].into_iter().collect();
    h.frame();
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert_eq!(
        world_anchor_points(&h, pen_id).len(),
        2,
        "Direct Select: two gone"
    );
    h.app.direct.anchors = [0].into_iter().collect();
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert!(
        h.app.doc().scene.node(pen_id).is_none(),
        "one anchor left removes it"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        world_anchor_points(&h, pen_id).len(),
        2,
        "one Ctrl+Z brings it back"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(world_anchor_points(&h, pen_id).len(), 4);
}

/// pp3 guard (approved 28 September 2026): Direct Select targets a curve,
/// a marquee picks its anchors, and Esc steps back picks, then the target,
/// then the tool.
#[test]
fn direct_select_marquee_picks_and_esc_steps_back_picks_target_then_tool() {
    let mut h = grip_board("direct_marquee_esc");
    let (id, pts) = hud_polyline(&mut h);
    h.app.board_sel.clear();
    h.frame();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.tool.direct_select"),
        None
    ));
    h.frame();
    assert_eq!(h.app.board_tool, board::BoardTool::DirectSelect);
    let xf = h.app.board_xf();
    press_primary(
        &mut h,
        xf.w2s(pts[0] + EVec2::new(75.0, 0.0)),
        egui::Modifiers::NONE,
    );
    assert_eq!(h.app.direct.node, Some(id), "a click targets the curve");
    assert!(h.app.direct.anchors.is_empty());

    press_drag_release_frames(
        &mut h,
        &[
            pts[1] + EVec2::new(-20.0, -40.0),
            pts[1] + EVec2::new(40.0, 0.0),
            pts[2] + EVec2::new(20.0, 40.0),
        ],
        egui::Modifiers::NONE,
        |_| {},
    );
    h.frame();
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![1, 2])),
        "the marquee picks"
    );

    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.direct.anchors.is_empty(), "Esc drops the picks first");
    assert_eq!(h.app.direct.node, Some(id), "the target stays");
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(h.app.direct.node, None, "then the target");
    assert_eq!(h.app.board_tool, board::BoardTool::DirectSelect);
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(h.app.board_tool, board::BoardTool::Select, "then the tool");
}

#[test]
fn direct_select_deletes_an_anchor_and_rejoins_its_neighbors() {
    let mut h = Harness::new("direct_delete");
    let id = add_seg(&mut h.app, Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0));
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    // Make a three-anchor path by writing one through the direct helpers.
    let (mut anchors, closed) = h.app.direct_anchors_of(id).unwrap();
    anchors.insert(
        1,
        vector_ink::Anchor::corner(vector_ink::kurbo::Point::new(50.0, 40.0)),
    );
    h.app.patch_nodes(&[id], |_| {});
    {
        let bez = vector_ink::bezpath_from_anchors(&anchors, closed);
        let (rect, data) = board_path::bezpath_to_path_data(&bez, closed);
        h.app.patch_nodes(&[id], |n| {
            n.rect = rect;
            if let slate_doc::scene::NodeKind::Shape(s) = &mut n.kind {
                s.path = Some(data.clone().into());
            }
        });
    }
    assert_eq!(h.app.direct_anchors_of(id).unwrap().0.len(), 3);
    h.app.direct.anchors.insert(1);
    assert!(h.app.direct_delete_anchors());
    let (left, _) = h.app.direct_anchors_of(id).unwrap();
    assert_eq!(left.len(), 2, "the middle anchor is gone");
    assert!((left[1].point.x - 100.0).abs() < 1e-3, "neighbors rejoin");
    h.app.board_undo();
    assert_eq!(
        h.app.direct_anchors_of(id).unwrap().0.len(),
        3,
        "one undo restores it"
    );
}
