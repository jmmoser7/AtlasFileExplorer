//! Escape and release-outside restore a drag, resize, or rotate.

use super::*;

/// P0.1 — Esc mid-move puts all three nodes back, journals nothing, and
/// keeps the selection.
#[test]
fn esc_mid_move_restores_three_nodes_and_keeps_the_selection() {
    let (mut h, ids) = esc_drag_board("esc_move3");
    select_ids(&mut h.app, &ids);
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let (from, to) = (Pos2::new(40.0, 30.0), Pos2::new(140.0, 110.0));
    hold_drag(&mut h, from, to, egui::Modifiers::NONE);
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Move { .. })),
        "the move is live before Esc"
    );
    assert_ne!(scene_nodes(&h), before, "the nodes moved mid-drag");
    escape_then_release(&mut h, to, egui::Modifiers::NONE);
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before, "every node is back exactly");
    assert_eq!(h.app.tab().journal.undo_depth(), depth, "nothing journaled");
    assert_eq!(h.app.board_sel, ids.into_iter().collect());
}

/// P0.1 — Esc mid Alt-duplicate move removes the staged copies and gives
/// the selection back to the originals.
#[test]
fn esc_mid_alt_duplicate_move_leaves_no_copies() {
    let (mut h, ids) = esc_drag_board("esc_alt_dup");
    select_ids(&mut h.app, &ids[..2]);
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let (from, to) = (Pos2::new(40.0, 30.0), Pos2::new(140.0, 110.0));
    hold_drag(&mut h, from, to, alt);
    assert!(
        matches!(
            h.app.board_drag,
            Some(board::BoardDrag::Move { dup: true, .. })
        ),
        "Alt stages a duplicate move"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 5, "two copies are staged");
    escape_then_release(&mut h, to, alt);
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before, "no copies are left");
    assert_eq!(h.app.tab().journal.undo_depth(), depth, "nothing journaled");
    assert_eq!(h.app.board_sel, ids[..2].iter().copied().collect());
}

/// P0.1 — Esc mid-resize restores the node.
#[test]
fn esc_mid_resize_restores_the_node() {
    let (mut h, ids) = esc_drag_board("esc_resize");
    select_ids(&mut h.app, &ids[1..2]);
    h.frame();
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let rect = h.app.doc().scene.node(ids[1]).unwrap().rect;
    let xf = h.app.board_xf();
    let corner = board_handles::selection_geom(&xf, rect, 0.0).corners[2];
    let from = xf.s2w(corner);
    let to = from + egui::vec2(60.0, 40.0);
    hold_drag(&mut h, from, to, egui::Modifiers::NONE);
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Resize { .. })),
        "the corner starts a resize"
    );
    assert_ne!(scene_nodes(&h), before, "the node resized mid-drag");
    escape_then_release(&mut h, to, egui::Modifiers::NONE);
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.board_sel, ids[1..2].iter().copied().collect());
}

/// P0.1 — Esc mid-rotate restores the node.
#[test]
fn esc_mid_rotate_restores_the_node() {
    let (mut h, ids) = esc_drag_board("esc_rotate");
    select_ids(&mut h.app, &ids[1..2]);
    h.frame();
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let rect = h.app.doc().scene.node(ids[1]).unwrap().rect;
    let xf = h.app.board_xf();
    let zone = board_handles::selection_geom(&xf, rect, 0.0).rotate_points[2];
    let from = xf.s2w(zone);
    let to = Pos2::new(rect.x - 30.0, rect.y + rect.h + 60.0);
    hold_drag(&mut h, from, to, egui::Modifiers::NONE);
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Rotate { .. })),
        "the outside-corner zone starts a rotate"
    );
    assert_ne!(scene_nodes(&h), before, "the node turned mid-drag");
    escape_then_release(&mut h, to, egui::Modifiers::NONE);
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.board_sel, ids[1..2].iter().copied().collect());
}

/// P0.1 — Esc mid group-rotate restores every member.
#[test]
fn esc_mid_group_rotate_restores_every_node() {
    let (mut h, ids) = esc_drag_board("esc_group_rotate");
    select_ids(&mut h.app, &ids);
    h.frame();
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let gb = h.app.board_group_bounds().unwrap();
    let xf = h.app.board_xf();
    let zone = board_handles::selection_geom(&xf, gb, 0.0).rotate_points[2];
    let from = xf.s2w(zone);
    let to = Pos2::new(gb.x + gb.w * 0.5, gb.y + gb.h + 200.0);
    hold_drag(&mut h, from, to, egui::Modifiers::NONE);
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::GroupRotate { .. })),
        "the group's outside-corner zone starts a group rotate"
    );
    assert_ne!(scene_nodes(&h), before, "the group turned mid-drag");
    escape_then_release(&mut h, to, egui::Modifiers::NONE);
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.board_sel, ids.into_iter().collect());
}

/// A release the board sees with no pointer position (let go outside the
/// window, the pointer gone by frame end) restores the nodes rather than
/// leaving them displaced and unjournaled.
#[test]
fn release_outside_the_window_restores_the_moved_nodes() {
    let (mut h, ids) = esc_drag_board("release_outside");
    select_ids(&mut h.app, &ids);
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    hold_drag(
        &mut h,
        Pos2::new(40.0, 30.0),
        Pos2::new(140.0, 110.0),
        egui::Modifiers::NONE,
    );
    assert!(matches!(
        h.app.board_drag,
        Some(board::BoardDrag::Move { .. })
    ));
    assert_ne!(scene_nodes(&h), before, "the nodes moved mid-drag");
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: Pos2::new(-40.0, -40.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        i.events.push(egui::Event::PointerGone);
    });
    h.frame();
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// An Alt copy dropped on a picture: the staged copy leaves unjournaled,
/// the original stays where it was, and the drop is the one journal entry.
#[test]
fn alt_copy_dropped_on_a_picture_leaves_no_unjournaled_copy() {
    use super::super::board_image_layers::{ImageDropChoice, ImageDropOffer, ImageDropSource};
    let mut h = align_board("alt_image_drop");
    h.app.tab_mut().cam.z = 1.0;
    h.app.tab_mut().cam.offset = egui::vec2(0.0, 0.0);
    let src = picture_from(&mut h, "source.png", -300.0);
    let dst = picture_from(&mut h, "target.png", 0.0);
    h.frame();
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    h.app.alt_down = true;
    select_ids(&mut h.app, &[src]);
    let (press, over) = (Pos2::new(-220.0, 60.0), Pos2::new(80.0, 60.0));
    let xf = h.app.board_xf();
    h.app.board_drag = h.app.begin_gesture_for_test(xf.w2s(press), press, alt);
    let copy = match &h.app.board_drag {
        Some(board::BoardDrag::Move { ids, dup: true, .. }) => ids[0],
        _ => panic!("Alt stages a copy"),
    };
    h.app.update_gesture_for_test(over, alt);
    h.app.image_drop = Some(ImageDropOffer {
        target: dst,
        source: ImageDropSource::Node(copy),
        highlight: Some(ImageDropChoice::Replace),
        row: None,
    });
    h.app.end_gesture_for_test(over, Some(xf.w2s(over)), alt);
    assert!(h.app.board_drag.is_none());
    assert_eq!(picture_item(&h, dst), picture_item(&h, src));
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before.len(),
        "no copy is left"
    );
    assert_eq!(
        h.app.doc().scene.node(src),
        before.iter().find(|n| n.id == src),
        "the original stays put"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(
        scene_nodes(&h),
        before,
        "one undo restores the board exactly"
    );
}
