//! Each PDF page keeps its own media-box size, on the card and unbundled.

use super::*;

fn ratio(rect: slate_doc::scene::WorldRect) -> f32 {
    rect.w / rect.h
}

fn place_pdf(h: &mut Harness, name: &str, boxes: &[(f32, f32)]) -> NodeId {
    let path = h.base.join(name);
    std::fs::write(&path, fixture_pdf_boxes(boxes)).unwrap();
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let ids = h.app.add_paths(std::slice::from_ref(&path));
    h.app.place_items_on_board(&ids, Pos2::new(200.0, 160.0));
    h.app.documents.seed(
        path,
        crate::app::pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: name.into(),
            pages: boxes.len() as u16,
            bytes: 64,
        },
    );
    h.app.doc().scene.nodes[0].id
}

#[test]
fn a_portrait_letter_page_drops_portrait() {
    let mut h = Harness::new("pdf_letter");
    let node = place_pdf(&mut h, "letter.pdf", &[(612.0, 792.0)]);
    let rect = h.app.doc().scene.node(node).unwrap().rect;
    assert!(
        rect.h > rect.w,
        "portrait letter dropped as {}x{}",
        rect.w,
        rect.h
    );
    let got = ratio(rect);
    let want = 612.0 / 792.0;
    assert!((got - want).abs() < 0.02, "ratio {got} want {want}");
}

#[test]
fn mixed_pdf_pages_keep_their_own_size_on_the_card_and_unbundled() {
    let mut h = Harness::new("pdf_mixed");
    let boxes = [(612.0, 792.0), (792.0, 612.0), (400.0, 400.0)];
    let node = place_pdf(&mut h, "mixed.pdf", &boxes);
    let card = h.app.doc().scene.node(node).unwrap().rect;
    assert!(
        (ratio(card) - 612.0 / 792.0).abs() < 0.02,
        "card {}",
        ratio(card)
    );
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    let mut got: Vec<f32> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .map(|n| ratio(n.rect))
        .collect();
    got.sort_by(f32::total_cmp);
    let mut want = [612.0 / 792.0, 1.0, 792.0 / 612.0];
    want.sort_by(f32::total_cmp);
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 0.02, "pages {got:?} want {want:?}");
    }
}
