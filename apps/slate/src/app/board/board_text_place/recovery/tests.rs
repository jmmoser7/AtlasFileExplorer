use super::*;
use crate::app::tests::Harness;
use slate_doc::scene::{ImageNode, ShapeText, TextStyle};

fn board(tag: &str) -> Harness {
    let mut harness = Harness::new(tag);
    harness.app.leave_home();
    harness.app.ensure_work_tab();
    harness
}

#[test]
fn recovery_keeps_active_new_text_without_committing_the_live_editor() {
    let mut h = board("recovery-uncommitted-text");
    h.app.place_text_at(Pos2::new(10.0, 20.0));
    let draft = h.app.text_box_draft.as_mut().unwrap();
    draft.buffer = "Words still being typed".into();
    draft.family = Typeface::Arial;
    draft.color = Rgba([31, 47, 63, 255]);
    draft.size = 37.0;
    draft.align = TextAlign::Right;
    h.app.tab_mut().dirty = false;
    let before = h.app.doc().clone();
    let undo_before = h.app.tab().journal.can_undo();
    assert!(h.app.recovery_has_text_drafts());
    let mut recovered = before.clone();
    h.app.capture_recovery_text(&mut recovered);
    assert_eq!(h.app.doc(), &before);
    assert_eq!(h.app.tab().journal.can_undo(), undo_before);
    assert!(!h.app.tab().dirty);
    assert!(h.app.text_box_draft.is_some());
    let NodeKind::Text(text) = &recovered.scene.nodes[0].kind else {
        panic!()
    };
    assert_eq!(text.text, "Words still being typed");
    assert_eq!(text.family, Typeface::Arial);
    assert_eq!(text.color, Rgba([31, 47, 63, 255]));
    assert_eq!(text.size, 37.0);
    assert_eq!(text.align, TextAlign::Right);
    h.app.commit_text_box_draft();
    assert!(!h.app.recovery_has_text_drafts());
    let mut after_commit = h.app.doc().clone();
    h.app.capture_recovery_text(&mut after_commit);
    assert_eq!(after_commit.scene.nodes.len(), 1);
    assert_eq!(after_commit.scene.nodes, recovered.scene.nodes);
}

#[test]
fn recovery_ignores_empty_or_cancelled_text_drafts() {
    let mut h = board("recovery-empty-cancelled-text");
    h.app.place_text_at(Pos2::new(1.0, 2.0));
    h.app.tab_mut().dirty = false;
    assert!(!h.app.recovery_has_text_drafts());
    let mut empty = h.app.doc().clone();
    h.app.capture_recovery_text(&mut empty);
    assert!(empty.scene.nodes.is_empty());
    h.app.commit_text_box_draft();
    assert!(!h.app.tab().dirty, "an empty compose is still a no-op");
    h.app.place_text_at(Pos2::new(3.0, 4.0));
    h.app.text_box_draft.as_mut().unwrap().buffer = "Then cancelled".into();
    let mut pending = h.app.doc().clone();
    h.app.capture_recovery_text(&mut pending);
    assert_eq!(pending.scene.nodes.len(), 1);
    h.app.cancel_text_box_draft();
    let mut after_cancel = h.app.doc().clone();
    h.app.capture_recovery_text(&mut after_cancel);
    assert!(!h.app.recovery_has_text_drafts());
    assert!(after_cancel.scene.nodes.is_empty());
}

#[test]
fn recovery_keeps_inline_words_and_grown_bounds_with_existing_styles() {
    let mut h = board("recovery-inline-edit");
    h.app.place_sticky_at(Pos2::new(20.0, 30.0));
    let id = *h.app.board_sel.iter().next().unwrap();
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Text(text) = &mut node.kind else {
            panic!()
        };
        text.text = "Earlier words".into();
        text.style.bold = true;
        text.color = Rgba([72, 84, 96, 255]);
    });
    let bounds = WorldRect::new(20.0, 30.0, 200.0, 270.0);
    h.app.text_edit = Some((id, "Uncommitted replacement".into()));
    h.app.text_edit_rect = Some((id, bounds));
    h.app.tab_mut().dirty = false;
    let original = h.app.doc().clone();
    assert!(h.app.recovery_has_text_drafts());
    let mut recovered = original.clone();
    h.app.capture_recovery_text(&mut recovered);
    assert_eq!(h.app.doc(), &original);
    assert!(!h.app.tab().dirty);
    let node = recovered.scene.node(id).unwrap();
    assert_eq!(node.rect, bounds);
    let NodeKind::Text(text) = &node.kind else {
        panic!()
    };
    assert_eq!(text.text, "Uncommitted replacement");
    assert!(text.style.bold);
    assert_eq!(text.color, Rgba([72, 84, 96, 255]));
    h.app.commit_text_edit();
    assert_eq!(h.app.doc().scene.node(id), recovered.scene.node(id));
    assert!(!h.app.recovery_has_text_drafts());
}

#[test]
fn recovery_keeps_uncommitted_shape_text_and_authored_emphasis() {
    let mut h = board("recovery-shape-edit");
    h.app.finish_draw(
        Pos2::ZERO,
        Pos2::new(240.0, 120.0),
        BoardTool::RectShape,
        egui::Modifiers::NONE,
    );
    let id = *h.app.board_sel.iter().next().unwrap();
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Shape(shape) = &mut node.kind else {
            panic!()
        };
        let mut text = ShapeText::new(Rgba([10, 20, 30, 255]));
        text.body = "Before".into();
        text.style = TextStyle {
            italic: true,
            ..Default::default()
        };
        shape.text = Some(text);
    });
    h.app.text_edit = Some((id, "Words in the rectangle".into()));
    let mut recovered = h.app.doc().clone();
    h.app.capture_recovery_text(&mut recovered);
    let NodeKind::Shape(shape) = &recovered.scene.node(id).unwrap().kind else {
        panic!()
    };
    let text = shape.text.as_ref().unwrap();
    assert_eq!(text.body, "Words in the rectangle");
    assert!(text.style.italic);
    assert_eq!(text.color, Rgba([10, 20, 30, 255]));
    h.app.commit_text_edit();
    assert_eq!(h.app.doc().scene.node(id), recovered.scene.node(id));
}

#[test]
fn recovery_keeps_linked_card_words_without_reading_or_writing_its_source() {
    let mut h = board("recovery-card-edit");
    let source = h.base.join("missing-cloud-source.txt");
    let item = h
        .app
        .doc_mut()
        .add_item(source.clone(), "source.txt", 0, 0, "none");
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(1.0, 2.0, 300.0, 90.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let bounds = WorldRect::new(1.0, 2.0, 300.0, 150.0);
    h.app.text_doc_edit = Some(TextDocEdit {
        node: id,
        item,
        path: source.clone(),
        rect: bounds,
        buffer: "Authored only in the workbook".into(),
        original: "Before".into(),
        claim_focus: true,
        auto_size: false,
    });
    let original = h.app.doc().clone();
    assert!(h.app.recovery_has_text_drafts());
    let mut recovered = original.clone();
    h.app.capture_recovery_text(&mut recovered);
    assert_eq!(h.app.doc(), &original);
    assert!(!source.exists());
    let node = recovered.scene.node(id).unwrap();
    let NodeKind::Image(image) = &node.kind else {
        panic!()
    };
    assert_eq!(
        image.text_override.as_deref(),
        Some("Authored only in the workbook")
    );
    assert_eq!(node.rect, bounds);
    h.app.commit_text_doc_edit();
    assert_eq!(h.app.doc().scene.node(id), recovered.scene.node(id));
}

#[test]
fn unchanged_inline_buffer_does_not_require_a_recovery_snapshot() {
    let mut h = board("recovery-unchanged-inline");
    h.app.place_sticky_at(Pos2::ZERO);
    let id = *h.app.board_sel.iter().next().unwrap();
    h.app.text_edit = Some((id, String::new()));
    h.app.tab_mut().dirty = false;
    assert!(!h.app.recovery_has_text_drafts());
    let original = h.app.doc().clone();
    let mut recovered = original.clone();
    h.app.capture_recovery_text(&mut recovered);
    assert_eq!(recovered, original);
}

#[test]
fn recovery_keeps_new_text_in_its_image_layer_with_the_commit_identity_and_transform() {
    use crate::app::board_image_layers::{ImagePaintSession, ImageStripFocus};
    use slate_doc::image_paint::{layers_for_result, ImageResultId};
    let mut h = board("recovery-image-layer-text");
    let source = h.base.join("missing-cloud-image.png");
    let item = h.app.doc_mut().add_item(source, "image.png", 0, 0, "image");
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(10.0, 20.0, 400.0, 300.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let host = h.app.add_nodes(vec![node])[0];
    let result = ImageResultId::Item(item);
    h.app.image_paint = Some(ImagePaintSession {
        image: host,
        result: result.clone(),
        layer_index: 0,
        focus: ImageStripFocus::Filter,
        layer_drag: None,
    });
    h.app.place_text_at(Pos2::new(80.0, 90.0));
    let draft = h.app.text_box_draft.as_mut().unwrap();
    draft.buffer = "Uncommitted image annotation".into();
    draft.rect = WorldRect::new(80.0, 90.0, 200.0, 50.0);
    let original = h.app.doc().clone();
    let mut recovered = original.clone();
    h.app.capture_recovery_text(&mut recovered);
    assert_eq!(h.app.doc(), &original);
    assert_eq!(
        recovered.scene.nodes.len(),
        1,
        "annotation stays on the image"
    );
    let NodeKind::Image(image) = &recovered.scene.node(host).unwrap().kind else {
        panic!()
    };
    let layers = layers_for_result(image, &result);
    let child = &layers[0].nodes[0];
    let NodeKind::Text(text) = &child.kind else {
        panic!()
    };
    assert_eq!(text.text, "Uncommitted image annotation");
    assert_eq!(
        child.rect,
        WorldRect::new(0.175, 70.0 / 300.0, 0.5, 50.0 / 300.0)
    );
    h.app.commit_text_box_draft();
    assert_eq!(h.app.doc().scene.nodes, recovered.scene.nodes);
    assert!(!h.app.recovery_has_text_drafts());
}

#[test]
fn readonly_inline_commit_keeps_buffer_and_bounds_for_save_copy() {
    let mut h = board("readonly-inline-buffer");
    h.app.place_sticky_at(Pos2::ZERO);
    let id = *h.app.board_sel.iter().next().unwrap();
    h.app.text_edit = Some((id, "Pending inline words".into()));
    let rect = WorldRect::new(0.0, 0.0, 200.0, 250.0);
    h.app.text_edit_rect = Some((id, rect));
    h.app.tab_mut().read_only = true;
    h.app.tab_mut().dirty = false;
    let before = h.app.doc().clone();
    h.app.commit_text_edit();
    assert_eq!(h.app.doc(), &before);
    assert_eq!(h.app.text_edit.as_ref().unwrap().1, "Pending inline words");
    assert_eq!(h.app.text_edit_rect, Some((id, rect)));
    assert!(h.app.recovery_has_text_drafts());
}

#[test]
fn readonly_linked_card_commit_keeps_authored_override_buffer() {
    let mut h = board("readonly-linked-card-buffer");
    let source = h.base.join("unread-source.txt");
    let item = h
        .app
        .doc_mut()
        .add_item(source.clone(), "source.txt", 0, 0, "text");
    let rect = WorldRect::new(0.0, 0.0, 100.0, 80.0);
    let node = h
        .app
        .doc_mut()
        .scene
        .build_node(rect, NodeKind::Image(ImageNode::new(item)));
    let id = h.app.add_nodes(vec![node])[0];
    h.app.text_doc_edit = Some(TextDocEdit {
        node: id,
        item,
        path: source.clone(),
        rect,
        buffer: "Pending card words".into(),
        original: "Before".into(),
        claim_focus: true,
        auto_size: false,
    });
    h.app.tab_mut().read_only = true;
    h.app.tab_mut().dirty = false;
    let before = h.app.doc().clone();
    h.app.commit_text_doc_edit();
    assert_eq!(h.app.doc(), &before);
    assert_eq!(
        h.app.text_doc_edit.as_ref().unwrap().buffer,
        "Pending card words"
    );
    assert!(h.app.recovery_has_text_drafts());
    assert!(!source.exists());
}

#[test]
fn readonly_double_click_on_another_note_preserves_pending_words_and_editor() {
    let mut h = board("readonly-double-click-editor-handoff");
    h.app.place_sticky_at(Pos2::ZERO);
    let first = *h.app.board_sel.iter().next().unwrap();
    h.app.place_sticky_at(Pos2::new(400.0, 0.0));
    h.app.text_edit = Some((first, "Keep the original editor's words".into()));
    h.app.tab_mut().read_only = true;
    h.app.tab_mut().dirty = false;
    let before = h.app.doc().clone();
    h.app.board_double_click_for_test(Pos2::new(400.0, 0.0));
    assert_eq!(h.app.doc(), &before);
    assert_eq!(h.app.text_edit.as_ref().unwrap().0, first);
    assert_eq!(
        h.app.text_edit.as_ref().unwrap().1,
        "Keep the original editor's words"
    );
    assert!(h.app.recovery_has_text_drafts());
}

#[test]
fn readonly_sticky_tab_and_direct_placement_cannot_overwrite_pending_words() {
    let mut h = board("readonly-sticky-tab-editor-handoff");
    h.app.place_sticky_at(Pos2::ZERO);
    let first = *h.app.board_sel.iter().next().unwrap();
    h.app.text_edit = Some((first, "Pending sticky words".into()));
    h.app.tab_mut().read_only = true;
    h.app.tab_mut().dirty = false;
    let before = h.app.doc().clone();
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: Some(egui::Key::Tab),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    h.app.spawn_adjacent_sticky(first, 1.0);
    h.app.place_sticky_at(Pos2::new(400.0, 0.0));
    assert_eq!(h.app.doc().scene, before.scene);
    assert_eq!(h.app.text_edit.as_ref().unwrap().0, first);
    assert_eq!(h.app.text_edit.as_ref().unwrap().1, "Pending sticky words");
    assert!(h.app.recovery_has_text_drafts());
}
