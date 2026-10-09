//! Edge mirror, corner scale, and mirror commands.

use super::*;

/// Corner-scaling an image near a neighbour's edge lands on that edge and
/// keeps the aspect. Preview and commit share that rect. Alt does not snap.
#[test]
fn corner_scale_of_an_image_lands_on_a_neighbour_edge() {
    let mut h = web_board("corner_edge_snap");
    let id = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 50.0),
    );
    let _neighbour = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(220.0, 0.0, 40.0, 80.0),
    );
    h.app.board_sel = std::iter::once(id).collect();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.app.board_osnap.enabled = false;
    h.app.board_snap_grid = false;
    h.app.board_smart_guides = true;

    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let se = xf.w2s(Pos2::new(
        node.rect.x + node.rect.w,
        node.rect.y + node.rect.h,
    ));
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    h.app.board_drag = h
        .app
        .begin_gesture_for_test(se, xf.s2w(se), egui::Modifiers::NONE);
    assert!(
        matches!(
            h.app.board_drag,
            Some(board::BoardDrag::Resize { handle: 4, .. })
        ),
        "SE corner starts a resize"
    );
    let raw = Pos2::new(214.0, 107.0);
    h.app.update_gesture_for_test(raw, alt);
    let held = h.app.doc().scene.node(id).unwrap().rect;
    assert!(
        (held.x + held.w - 220.0).abs() > 1.0,
        "alt suspends, right={}",
        held.x + held.w
    );
    h.app.end_gesture_for_test(raw, Some(xf.w2s(raw)), alt);
    h.app.board_undo();
    assert!(
        (h.app.doc().scene.node(id).unwrap().rect.w - 100.0).abs() < 0.01,
        "undo restores the image"
    );

    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let se = xf.w2s(Pos2::new(
        node.rect.x + node.rect.w,
        node.rect.y + node.rect.h,
    ));
    let mods = egui::Modifiers::NONE;
    h.app.board_drag = h.app.begin_gesture_for_test(se, xf.s2w(se), mods);
    assert!(
        matches!(
            h.app.board_drag,
            Some(board::BoardDrag::Resize { handle: 4, .. })
        ),
        "SE corner starts a resize"
    );
    let target = Pos2::new(214.0, 107.0);
    h.app.update_gesture_for_test(target, mods);
    let live = h.app.doc().scene.node(id).unwrap().rect;
    assert!(
        (live.x + live.w - 220.0).abs() < 0.01,
        "live right {}",
        live.x + live.w
    );
    assert!(
        (live.w / live.h - 2.0).abs() < 1e-3,
        "aspect {}",
        live.w / live.h
    );
    assert!(
        h.app
            .board_snap_guides
            .iter()
            .any(|g| g.axis == board_snap::GuideAxis::Vertical && (g.pos - 220.0).abs() < 0.01),
        "guide while snapped: {:?}",
        h.app.board_snap_guides
    );
    h.app
        .end_gesture_for_test(target, Some(xf.w2s(target)), mods);
    let committed = h.app.doc().scene.node(id).unwrap().rect;
    assert!(
        (committed.x - live.x).abs() < 0.01 && (committed.w - live.w).abs() < 0.01,
        "commit matches preview"
    );
    assert!((committed.w / committed.h - 2.0).abs() < 1e-3);
}

/// Dragging a picture's right edge past its left edge mirrors it: the width
/// stays positive, the flip is authored state, and one undo restores both.
#[test]
fn dragging_a_pictures_edge_past_its_opposite_mirrors_it() {
    let mut h = web_board("edge_cross_mirror");
    let id = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let xf = h.app.board_xf();
    // Off the edge midpoint so a wire grip does not steal the press.
    let edge = xf.w2s(Pos2::new(200.0, 12.0));
    let mods = egui::Modifiers::default();
    h.app.board_drag = h.app.begin_gesture_for_test(edge, xf.s2w(edge), mods);
    assert!(matches!(
        h.app.board_drag,
        Some(board::BoardDrag::Resize { handle: 3, .. })
    ));
    let undo_depth = h.app.tab().journal.undo_depth();
    // Through the far edge and back out again: the flip follows the pointer.
    h.app.update_gesture_for_test(Pos2::new(-30.0, 12.0), mods);
    assert_eq!(picture_flips(&h, id), (true, false));
    h.app.update_gesture_for_test(Pos2::new(120.0, 12.0), mods);
    assert_eq!(picture_flips(&h, id), (false, false));
    let past = Pos2::new(-60.0, 12.0);
    h.app.update_gesture_for_test(past, mods);
    h.app.end_gesture_for_test(past, Some(xf.w2s(past)), mods);

    let after = h.app.doc().scene.node(id).unwrap().clone();
    assert!(
        (after.rect.x + 60.0).abs() < 0.5 && (after.rect.w - 60.0).abs() < 0.5,
        "{:?}",
        after.rect
    );
    assert!(after.rect.w > 0.0 && after.rect.h > 0.0);
    assert_eq!(picture_flips(&h, id), (true, false));
    assert_eq!(
        h.app.tab().journal.undo_depth(),
        undo_depth + 1,
        "one step per drag"
    );

    h.app.board_undo();
    let undone = h.app.doc().scene.node(id).unwrap();
    assert_eq!(undone.rect, before.rect);
    assert_eq!(picture_flips(&h, id), (false, false));
}

/// A shape that cannot mirror keeps the old clamp at the minimum size.
#[test]
fn dragging_a_rect_edge_past_its_opposite_still_clamps() {
    let mut h = web_board("edge_cross_rect");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let xf = h.app.board_xf();
    let edge = xf.w2s(Pos2::new(80.0, 12.0));
    let mods = egui::Modifiers::default();
    h.app.board_drag = h.app.begin_gesture_for_test(edge, xf.s2w(edge), mods);
    let past = Pos2::new(-60.0, 12.0);
    h.app.update_gesture_for_test(past, mods);
    h.app.end_gesture_for_test(past, Some(xf.w2s(past)), mods);
    let after = h.app.doc().scene.node(id).unwrap().rect;
    assert_eq!(after.x, 0.0, "{after:?}");
}

/// Mirror horizontal / vertical flip pictures and paths across the board
/// axis through each node's center; rectangles are left alone. Undo restores.
#[test]
fn mirror_commands_toggle_pictures_and_paths_and_undo() {
    use slate_doc::scene::{PathData, PathSeg, ShapeKind, ShapeNode};
    let mut h = web_board("mirror_commands");
    let pic = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    h.app.patch_nodes(&[pic], |n| n.rotation_deg = 30.0);
    let path = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.1, 0.2],
                segs: vec![PathSeg::Line { to: [0.9, 0.7] }],
                ..PathData::default()
            })),
            text: None,
        }),
    );
    let path = h.app.add_nodes(vec![path])[0];
    let rect = add_rect(&mut h.app, 500.0, 0.0);
    let rect_before = h.app.doc().scene.node(rect).unwrap().clone();
    h.app.board_sel = [pic, path, rect].into_iter().collect();

    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.mirror.horizontal"),
        None
    ));
    assert_eq!(picture_flips(&h, pic), (true, false));
    assert_eq!(h.app.doc().scene.node(pic).unwrap().rotation_deg, -30.0);
    let start = |h: &Harness| match &h.app.doc().scene.node(path).unwrap().kind {
        slate_doc::NodeKind::Shape(s) => s.path.as_ref().unwrap().start,
        _ => panic!("path"),
    };
    assert!((start(&h)[0] - 0.9).abs() < 1e-6, "{:?}", start(&h));
    assert_eq!(h.app.doc().scene.node(rect).unwrap(), &rect_before);

    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.mirror.vertical"),
        None
    ));
    assert_eq!(picture_flips(&h, pic), (true, true));

    h.app.board_undo();
    assert_eq!(picture_flips(&h, pic), (true, false));
    h.app.board_undo();
    assert_eq!(picture_flips(&h, pic), (false, false));
    assert_eq!(h.app.doc().scene.node(pic).unwrap().rotation_deg, 30.0);
    assert!((start(&h)[0] - 0.1).abs() < 1e-6);

    h.app.board_sel = std::iter::once(rect).collect();
    assert!(!h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.mirror.horizontal"),
        None
    ));
}
