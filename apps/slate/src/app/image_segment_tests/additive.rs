//! Additive object selection takes priority over a hover mask on an image.
use super::*;

#[test]
fn shift_click_on_a_stacked_image_adds_the_object_instead_of_pinning_its_mask() {
    let (mut h, image) = fixture("stacked-image-shift");
    let original = h.app.doc().scene.node(image).unwrap().clone();
    let added = h
        .app
        .doc_mut()
        .scene
        .build_node(original.rect, original.kind.clone());
    let top = added.id;
    h.app.add_nodes(vec![added]);
    h.app.board_sel = [image].into();
    let point = inject(&mut h, top);
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.modifiers = egui::Modifiers::SHIFT;
            input.events.push(egui::Event::PointerMoved(point));
            input.events.push(egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::SHIFT,
            });
        });
    }
    assert_eq!(h.app.board_sel, [image, top].into());
    assert!(h.app.image_segments.pinned.is_none());
    assert!(h.app.image_segments.drag.is_none());
}
