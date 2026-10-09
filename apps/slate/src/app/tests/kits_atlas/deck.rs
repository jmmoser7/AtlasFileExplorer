//! Deck-order strokes over frames.

use super::*;

/// GP1–GP6 for the Deck tool (`docs/keymap/contracts/frame-deck.md`).
#[test]
fn deck_clicks_build_a_prefix_and_undo_one_gesture_at_a_time() {
    let mut h = kit_board("deck_gp1", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 60.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0),
        1,
    );
    let c = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(400.0, 0.0, 80.0, 60.0),
        2,
    );
    let before = h.app.tab().journal.undo_depth();
    deck_release(
        &mut h,
        Pos2::new(40.0, 30.0),
        Pos2::new(40.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    // A was already first, so the click journals nothing but stays in the session.
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    deck_release(
        &mut h,
        Pos2::new(440.0, 30.0),
        Pos2::new(440.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    deck_release(
        &mut h,
        Pos2::new(240.0, 30.0),
        Pos2::new(240.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    // C is already in the session, so this click moves it to the end.
    deck_release(
        &mut h,
        Pos2::new(440.0, 30.0),
        Pos2::new(440.0, 30.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, b, c]);
    assert_eq!(h.app.board_tool, board::BoardTool::Deck);
    assert!(h.app.presenting.is_none());
    h.app.board_undo();
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    h.app.board_undo();
    assert_eq!(visible_frames(&h), vec![a, b, c]);
    assert_eq!(h.app.tab().journal.undo_depth(), before);
}

#[test]
fn deck_stroke_orders_frames_along_the_path_without_a_node() {
    let mut h = kit_board("deck_gp2", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0),
        2,
    );
    let c = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 100.0, 80.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(400.0, 0.0, 100.0, 80.0),
        1,
    );
    let before_nodes = h.app.doc().scene.nodes.len();
    h.app.board_drag = h.app.begin_gesture_for_test(
        Pos2::new(0.0, 0.0),
        Pos2::new(50.0, 40.0),
        egui::Modifiers::default(),
    );
    h.app
        .update_gesture_for_test(Pos2::new(250.0, 40.0), egui::Modifiers::default());
    h.app.end_gesture_for_test(
        Pos2::new(450.0, 40.0),
        Some(Pos2::new(40.0, 0.0)),
        egui::Modifiers::default(),
    );
    assert_eq!(visible_frames(&h), vec![a, c, b]);
    assert_eq!(h.app.doc().scene.nodes.len(), before_nodes);
    assert!(h.app.board_drag.is_none());
}

#[test]
fn deck_travel_under_four_pixels_is_a_click() {
    let mut h = kit_board("deck_gp3", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 80.0),
        1,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(300.0, 0.0, 100.0, 80.0),
        0,
    );
    deck_release(
        &mut h,
        Pos2::new(50.0, 40.0),
        Pos2::new(350.0, 40.0),
        Pos2::new(3.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![a, b]);
}

#[test]
fn deck_escape_drops_the_stroke_then_disarms() {
    let mut h = kit_board("deck_gp4", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 60.0),
        0,
    );
    let _b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0),
        1,
    );
    let before = h.app.tab().journal.undo_depth();
    h.app.board_drag = h.app.begin_gesture_for_test(
        Pos2::new(0.0, 0.0),
        Pos2::new(40.0, 30.0),
        egui::Modifiers::default(),
    );
    h.app
        .update_gesture_for_test(Pos2::new(240.0, 30.0), egui::Modifiers::default());
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None));
    assert!(h.app.board_drag.is_none());
    assert_eq!(h.app.tab().journal.undo_depth(), before);
    assert_eq!(visible_frames(&h)[0], a);
    assert_eq!(h.app.board_tool, board::BoardTool::Deck);
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.cancel"), None));
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
}

#[test]
fn deck_stroke_skips_a_hidden_frame() {
    let mut h = kit_board("deck_gp5", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 60.0),
        0,
    );
    let hidden = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(100.0, 0.0, 80.0, 60.0),
        1,
    );
    h.app.patch_nodes(&[hidden], |n| n.hidden = true);
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0),
        2,
    );
    h.app.board_drag = h.app.begin_gesture_for_test(
        Pos2::new(0.0, 0.0),
        Pos2::new(20.0, 20.0),
        egui::Modifiers::default(),
    );
    h.app.end_gesture_for_test(
        Pos2::new(240.0, 20.0),
        Some(Pos2::new(30.0, 0.0)),
        egui::Modifiers::default(),
    );
    assert!(h.app.doc().scene.node(hidden).unwrap().hidden);
    assert_eq!(visible_frames(&h), vec![a, b]);
}

#[test]
fn deck_does_not_start_presentation_and_present_follows_the_new_order() {
    let mut h = kit_board("deck_gp6", board::BoardTool::Deck);
    let _a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 80.0, 40.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 40.0),
        1,
    );
    deck_release(
        &mut h,
        Pos2::new(240.0, 20.0),
        Pos2::new(240.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert!(h.app.presenting.is_none());
    h.app.start_present(None);
    let first = h
        .app
        .doc()
        .scene
        .frames_in_order()
        .iter()
        .find(|n| !n.hidden)
        .map(|n| n.id);
    assert_eq!(first, Some(b));
    assert!(h.app.presenting.is_some());
}

#[test]
fn deck_rearm_appends_and_a_second_click_moves_to_the_end() {
    let mut h = kit_board("deck_session", board::BoardTool::Deck);
    let a = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 40.0, 40.0),
        0,
    );
    let b = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(80.0, 0.0, 40.0, 40.0),
        1,
    );
    let c = deck_frame(
        &mut h,
        slate_doc::scene::WorldRect::new(160.0, 0.0, 40.0, 40.0),
        2,
    );
    deck_release(
        &mut h,
        Pos2::new(180.0, 20.0),
        Pos2::new(180.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![c, a, b]);
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.set_board_tool(board::BoardTool::Deck);
    deck_release(
        &mut h,
        Pos2::new(100.0, 20.0),
        Pos2::new(100.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![c, b, a]);
    deck_release(
        &mut h,
        Pos2::new(180.0, 20.0),
        Pos2::new(180.0, 20.0),
        Pos2::new(0.0, 0.0),
    );
    assert_eq!(visible_frames(&h), vec![b, c, a]);
}
