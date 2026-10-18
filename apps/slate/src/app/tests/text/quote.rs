//! Quote entry, empty linked-text reentry, and graphical text rotation.
use super::*;

#[test]
fn quote_immediate_typing_places_a_fitted_editable_document_without_enter() {
    let mut h = quote_board("quote-immediate-fit");
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(Pos2::new(700.0, 420.0)));
        input.events.push(egui::Event::Text("\"Hello There".into()));
    });
    h.frame();
    let edit = h
        .app
        .text_doc_edit
        .as_ref()
        .expect("typing opens document compose");
    assert_eq!(edit.buffer, "Hello There");
    assert!(edit.auto_size);
    assert!(!h.app.palette_state.open);
    h.frame();
    let edit = h.app.text_doc_edit.as_ref().unwrap();
    assert!(edit.rect.w >= 64.0 && edit.rect.w < 300.0);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

#[test]
fn an_empty_quote_document_reopens_when_its_empty_preview_was_evicted() {
    let mut h = quote_board("quote-empty-reopen");
    assert!(h.app.place_text_document_at(Pos2::new(0.0, 0.0)));
    let id = h.app.text_doc_edit.as_ref().unwrap().node;
    let item = h.app.text_doc_edit.as_ref().unwrap().item;
    h.app.commit_text_doc_edit();
    h.app.cache_snippet(item, None);
    let center = h.app.doc().scene.node(id).unwrap().rect.center();
    h.app
        .board_double_click_for_test(Pos2::new(center.0, center.1));
    assert!(h
        .app
        .text_doc_edit
        .as_ref()
        .is_some_and(|edit| edit.node == id && edit.buffer.is_empty()));
}

#[test]
fn graphical_text_paints_the_authored_rotation_about_its_node_center() {
    let mut h = text_draft_board("graphical-text-rotation");
    h.app.place_text_at(Pos2::ZERO);
    h.app.text_box_draft.as_mut().unwrap().buffer = "Rotated words".into();
    h.app.commit_text_box_draft();
    let id = *h.app.board_sel.iter().next().unwrap();
    h.app.patch_nodes(&[id], |node| node.rotation_deg = 90.0);
    h.app.board_sel.clear();
    let output = h.frame_output(|_| {});
    assert!(output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text)
            if text.galley.job.text == "Rotated words"
            && (text.angle - std::f32::consts::FRAC_PI_2).abs() < 1.0e-5)
    }));
}
