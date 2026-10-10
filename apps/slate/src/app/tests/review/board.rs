//! Review sheets for placed picture and PDF page sizes (ledger BR1, BR2).

use super::*;

fn review_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h
}

/// A left-to-right hue ramp inside a white edge, with a yellow square in the
/// top-left corner: the edge shows the picture reaches every side of its node,
/// and the square shows it is not turned or mirrored. Nothing in it is dark,
/// so no part can be mistaken for the board showing through.
fn ramp_png(path: &std::path::Path, w: u32, h: u32) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    let edge = (w.min(h) / 25).max(3);
    let mark = w.min(h) / 4;
    image::RgbaImage::from_fn(w, h, |x, y| {
        let t = x as f32 / w as f32;
        if x < edge || y < edge || x >= w - edge || y >= h - edge {
            image::Rgba([255, 255, 255, 255])
        } else if x < edge + mark && y < edge + mark {
            image::Rgba([250, 210, 40, 255])
        } else {
            image::Rgba([(255.0 * t) as u8, 120, (255.0 * (1.0 - t)) as u8, 255])
        }
    })
    .save(path)
    .unwrap();
}

/// Frame every node, then paint until each picture's thumbnail has landed.
fn shoot(h: &mut Harness, raster: &mut FrameRaster, item: &str, state: &str) {
    let nodes = &h.app.doc().scene.nodes;
    let (x0, y0, x1, y1) = nodes.iter().fold(
        (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
        |(a, b, c, d), n| {
            (
                a.min(n.rect.x),
                b.min(n.rect.y),
                c.max(n.rect.x + n.rect.w),
                d.max(n.rect.y + n.rect.h),
            )
        },
    );
    let pad = 60.0;
    h.app.zoom_to_rect(WorldRect::new(
        x0 - pad,
        y0 - pad,
        x1 - x0 + pad * 2.0,
        y1 - y0 + pad * 2.0,
    ));
    h.app.board_sel.clear();
    let keys: Vec<String> = h
        .app
        .doc()
        .items
        .iter()
        .map(|i| h.app.resolved_item_key(i))
        .collect();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline
        && !keys.iter().all(|k| h.app.thumb_pixels.contains_key(k))
    {
        h.app.pump_document_jobs(&h.ctx);
        capture_frame(h, raster, |_| {});
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    capture_frame(h, raster, |_| {});
    let out = capture_frame(h, raster, |_| {});
    review_shot(h, raster, out, item, state);
}

/// A 4:1 picture pasted from the clipboard (stored under the workbook's
/// `assets/pasted/`), beside the same picture dropped from disk and a tall
/// generated one. Each keeps its source shape.
#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn pasted_wide() {
    let mut h = review_board("review_br1");
    let mut raster = FrameRaster::new(1440, 900);
    let book = h.base.join("Book.slate");
    h.app.tab_mut().path = Some(book.clone());
    let data = atlas_core::index::data_dir();
    let pasted = atlas_core::workbook_assets::paste_dir(Some(&book), &data).join("wide.png");
    ramp_png(&pasted, 800, 200);
    let dropped = h.base.join("wide-drop.png");
    ramp_png(&dropped, 800, 200);
    let generated = atlas_core::workbook_assets::generated_dir(&book, "openai-image", "2026-10")
        .join("tall.png");
    ramp_png(&generated, 200, 600);
    for (path, at) in [
        (pasted, Pos2::new(0.0, 0.0)),
        (dropped, Pos2::new(0.0, 200.0)),
        (generated, Pos2::new(380.0, 100.0)),
    ] {
        assert!(h.app.ingest_dropped_paths(vec![path], at, false, None));
    }
    shoot(&mut h, &mut raster, "BR1", "pasted_wide");
}

/// One PDF with a portrait letter page, a landscape letter page, and a
/// square page: dropped as a card (page one, portrait), then unbundled.
#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn mixed_pages() {
    let mut h = review_board("review_br2");
    let mut raster = FrameRaster::new(1440, 900);
    let boxes = [(612.0, 792.0), (792.0, 612.0), (400.0, 400.0)];
    let path = h.base.join("mixed.pdf");
    std::fs::write(&path, fixture_pdf_boxes(&boxes)).unwrap();
    let ids = h.app.add_paths(std::slice::from_ref(&path));
    h.app.place_items_on_board(&ids, Pos2::new(0.0, 0.0));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while h.app.documents.ready(&path).is_none() && std::time::Instant::now() < deadline {
        // Unrasterized frames would drop font atlas updates the shot needs.
        capture_frame(&mut h, &mut raster, |_| {});
        h.app.pump_document_jobs(&h.ctx);
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert!(
        h.app.documents.ready(&path).is_some(),
        "pdfium opens the fixture (vendor/libpdfium.so on Linux)"
    );
    shoot(&mut h, &mut raster, "BR2", "card");
    let node = h.app.doc().scene.nodes[0].id;
    h.app.board_sel.insert(node);
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("board.media.unbundle"),
        Some(node.0.to_string())
    ));
    shoot(&mut h, &mut raster, "BR2", "mixed_pages");
}
