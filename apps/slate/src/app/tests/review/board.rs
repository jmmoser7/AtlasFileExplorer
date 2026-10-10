//! Review sheet for mixed-size PDF pages (ledger BR2).

use super::*;

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn mixed_pages() {
    let mut h = Harness::new("review_br2");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let boxes = [(612.0, 792.0), (792.0, 612.0), (400.0, 400.0)];
    let path = h.base.join("mixed.pdf");
    std::fs::write(&path, fixture_pdf_boxes(&boxes)).unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&path));
    h.app.place_items_on_board(&ids, Pos2::new(0.0, 0.0));
    h.app.documents.seed(
        path,
        crate::app::pdf::documents::DocumentPreview {
            path: h.base.join("preview.pdf"),
            revision: "review".into(),
            pages: 3,
            bytes: 64,
        },
    );
    let node = h.app.doc().scene.nodes[0].id;
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    let union = h
        .app
        .board_group_bounds()
        .unwrap_or(h.app.doc().scene.nodes[0].rect);
    h.app.tab_mut().cam.offset = egui::vec2(union.x + union.w * 0.5, union.y + union.h * 0.5);
    h.app.tab_mut().cam.z = 0.6;
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "BR2", "mixed_pages");
}
