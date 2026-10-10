//! Placed, pasted, and generated pictures keep the source pixel aspect.

use super::*;

fn wide_png(path: &std::path::Path) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    image::RgbaImage::from_pixel(400, 100, image::Rgba([20, 80, 160, 255]))
        .save(path)
        .unwrap();
}

fn aspect_of(h: &Harness, index: usize) -> f32 {
    let rect = h.app.doc().scene.nodes[index].rect;
    rect.w / rect.h
}

/// A 400×100 picture is four times as wide as it is tall. The default card
/// is 240×180, so a fallback box fails this.
#[test]
fn placed_pasted_and_generated_images_keep_source_aspect() {
    let mut h = Harness::new("image_aspect");
    h.app.ensure_work_tab();
    h.app.leave_home();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let book = h.base.join("Book.slate");
    h.app.tab_mut().path = Some(book.clone());
    let at = Pos2::new(200.0, 200.0);

    let dropped = h.base.join("wide-drop.png");
    wide_png(&dropped);
    assert!(h.app.ingest_dropped_paths(vec![dropped], at, false, None));

    let pasted =
        atlas_core::workbook_assets::paste_dir(Some(&book), &atlas_core::index::data_dir())
            .join("paste-wide.png");
    wide_png(&pasted);
    assert!(h.app.ingest_dropped_paths(vec![pasted], at, false, None));

    let generated = atlas_core::workbook_assets::generated_dir(&book, "openai-image", "2026-10")
        .join("gen.png");
    wide_png(&generated);
    assert!(h.app.ingest_dropped_paths(vec![generated], at, false, None));

    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    for (index, kind) in ["dropped", "pasted", "generated"].iter().enumerate() {
        let ratio = aspect_of(&h, index);
        assert!(
            (ratio - 4.0).abs() < 0.05,
            "{kind} picture ratio {ratio} is not the source 4:1"
        );
    }
}
