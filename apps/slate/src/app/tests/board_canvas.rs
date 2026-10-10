//! Authored board canvas smoke tests.

use super::*;

#[test]
fn board_view_renders_and_survives_frames() {
    let mut h = Harness::new("board_render");
    h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.seed_frame(None);
    let items: Vec<ItemId> = h.app.doc().items.iter().map(|i| i.id).collect();
    h.app
        .place_items_on_board(&items, eframe::egui::Pos2::new(100.0, 100.0));
    for _ in 0..5 {
        h.frame();
    }
    // 1 frame + 3 images.
    assert_eq!(h.app.doc().scene.nodes.len(), 4);
}

#[test]
fn drop_on_tagged_frame_inherits_tag() {
    let mut h = Harness::new("board_inherit");
    let (big, _small, _red) = h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let _frame = h.seed_frame(Some(big));
    // The uncategorized item (index 0) dropped inside the frame.
    let item = h.app.doc().items[0].id;
    assert!(h.app.doc().item(item).unwrap().assignments.is_empty());
    h.app
        .place_items_on_board(&[item], eframe::egui::Pos2::new(400.0, 225.0));
    assert!(h.app.doc().items_with_tag(big).contains(&item));
    // Dropped outside a frame: stays untagged.
    let mut h2 = Harness::new("board_inherit2");
    let (big2, ..) = h2.seed();
    h2.seed_frame(Some(big2));
    let item2 = h2.app.doc().items[0].id;
    h2.app
        .place_items_on_board(&[item2], eframe::egui::Pos2::new(5000.0, 5000.0));
    assert!(!h2.app.doc().items_with_tag(big2).contains(&item2));
}

#[test]
fn board_undo_redo_round_trip() {
    let mut h = Harness::new("board_undo");
    h.seed();
    let frame = h.seed_frame(None);
    // Patch the frame's rect via the journaled path.
    h.app
        .patch_nodes(&[frame], |n| n.rect = n.rect.translated(100.0, 0.0));
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.x, 100.0);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.x, 0.0);
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.node(frame).unwrap().rect.x, 100.0);
    // Undo twice removes the frame entirely (creation was journaled too).
    h.app.board_undo();
    h.app.board_undo();
    assert!(h.app.doc().scene.node(frame).is_none());
    h.frame();
}

/// Review r12 finding 5 (Art. VI): a step that cannot be undone or redone
/// stays where it was, with its Ctrl+Z mark, and the user is told. The
/// next attempt fails the same way instead of reaching the step under it.
#[test]
fn a_failed_board_undo_or_redo_keeps_its_step_and_says_so() {
    fn drift(h: &mut Harness, id: NodeId) {
        let at = h.app.doc().scene.index_of(id).unwrap();
        h.app.tab_mut().doc.scene.nodes[at].id = NodeId(777);
    }
    fn toasted(h: &Harness, msg: &str) -> bool {
        h.app.toasts.iter().any(|(m, _)| m == msg)
    }

    // Undo: the frame move fails; the frame added under it must stay.
    let mut h = Harness::new("board_undo_fails");
    h.seed();
    let moved = h.seed_frame(None);
    let older = h.seed_frame(None);
    h.app
        .patch_nodes(&[moved], |n| n.rect = n.rect.translated(100.0, 0.0));
    drift(&mut h, moved);
    let (edits, depth) = (h.app.tab().edits.len(), h.app.tab().journal.undo_depth());
    for attempt in ["first", "second"] {
        h.app.toasts.clear();
        h.app.board_undo();
        let tab = h.app.tab();
        assert!(tab.doc.scene.node(older).is_some(), "{attempt}: older kept");
        assert_eq!(tab.edits.len(), edits, "{attempt}: mark stays on edits");
        assert!(tab.edit_redo.is_empty(), "{attempt}: nothing to redo");
        assert_eq!(tab.journal.undo_depth(), depth, "{attempt}");
        assert!(!tab.journal.can_redo(), "{attempt}");
        assert!(toasted(&h, "Couldn't undo that step"), "{attempt}");
    }
    h.frame();

    // Redo: the frame move fails; the frame added after it must not appear.
    let mut h = Harness::new("board_redo_fails");
    h.seed();
    let moved = h.seed_frame(None);
    h.app
        .patch_nodes(&[moved], |n| n.rect = n.rect.translated(100.0, 0.0));
    let newer = h.seed_frame(None);
    h.app.board_undo();
    h.app.board_undo();
    assert!(h.app.doc().scene.node(newer).is_none());
    drift(&mut h, moved);
    let (edits, redo) = (h.app.tab().edits.len(), h.app.tab().edit_redo.len());
    let depth = h.app.tab().journal.undo_depth();
    for attempt in ["first", "second"] {
        h.app.toasts.clear();
        h.app.board_redo();
        let tab = h.app.tab();
        assert!(tab.doc.scene.node(newer).is_none(), "{attempt}: unreached");
        assert_eq!(tab.edits.len(), edits, "{attempt}: nothing moved to edits");
        assert_eq!(tab.edit_redo.len(), redo, "{attempt}: mark stays on redo");
        assert_eq!(tab.journal.undo_depth(), depth, "{attempt}");
        assert!(tab.journal.can_redo(), "{attempt}");
        assert!(toasted(&h, "Couldn't redo that step"), "{attempt}");
    }
    h.frame();
}

#[test]
fn duplicate_and_delete_board_nodes() {
    let mut h = Harness::new("board_dup");
    h.seed();
    let frame = h.seed_frame(None);
    let dups = h.app.duplicate_board_nodes(&[frame], 24.0, 24.0);
    assert_eq!(dups.len(), 1);
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let dup_rect = h.app.doc().scene.node(dups[0]).unwrap().rect;
    assert_eq!(dup_rect.x, 24.0);
    // Selection moved to the copy.
    assert!(h.app.board_sel.contains(&dups[0]));
    h.app.delete_board_nodes(&dups);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(h.app.board_sel.is_empty());
    // Undo the delete brings it back.
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    h.frame();
}

#[test]
fn scene_persists_through_save_and_reload() {
    let mut h = Harness::new("board_persist");
    h.seed();
    h.seed_frame(None);
    let items: Vec<ItemId> = h.app.doc().items.iter().map(|i| i.id).collect();
    h.app
        .place_items_on_board(&items, eframe::egui::Pos2::new(200.0, 200.0));
    let path = h.base.join("board.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());

    let mut h2 = Harness::new("board_persist2");
    h2.app.open_doc_at(path);
    assert_eq!(h2.app.doc().scene.nodes.len(), 4);
    assert_eq!(h2.app.doc().scene.frames_in_order().len(), 1);
    h2.frame();
}

#[test]
fn presentation_mode_enters_and_exits() {
    let mut h = Harness::new("board_present");
    h.seed();
    // No frames: refuses to present.
    h.app.start_present(None);
    assert!(h.app.presenting.is_none());
    h.seed_frame(None);
    h.app.start_present(None);
    assert!(h.app.presenting.is_some());
    for _ in 0..3 {
        h.frame();
    }
    h.app.stop_present();
    assert!(h.app.presenting.is_none());
    h.frame();
}

#[test]
fn export_artifact_writes_html() {
    let mut h = Harness::new("board_export");
    h.seed();
    h.seed_frame(None);
    let items: Vec<ItemId> = h.app.doc().items.iter().map(|i| i.id).collect();
    h.app
        .place_items_on_board(&items, eframe::egui::Pos2::new(200.0, 200.0));
    let out = h.base.join("export");
    h.app.do_export(out.clone());
    h.wait_for_export();
    let deck = out.join("Untitled-slides").join("index.html");
    assert!(deck.exists(), "expected {deck:?} to exist");
    let html = std::fs::read_to_string(deck).unwrap();
    assert!(html.contains("<section"));
    h.frame();
}
