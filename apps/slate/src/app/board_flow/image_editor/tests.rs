use super::*;

#[test]
fn image_result_drag_escape_cancels_before_selection_or_document_mutation() {
    let (mut h, host) = super::super::tests::generator_with_output("image-result-drag-cancel");
    h.app.board_sel = std::iter::once(host).collect();
    h.frame();
    let output = h.app.agent_images(host)[0].id.clone();
    h.app.agents.flow.image_result_drag = Some(ImageResultDrag {
        host,
        output,
        palette: Rect::NOTHING,
        tab: h.app.tab().id,
    });
    let nodes = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    crate::app::tests::press_key(&mut h, egui::Key::Escape);
    assert!(h.app.agents.flow.image_result_drag.is_none());
    assert!(h.app.board_sel.contains(&host));
    assert_eq!(h.app.doc().scene.nodes.len(), nodes);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
