use super::*;
use std::time::Duration;

/// A real filmstrip, a late scrub on a short clip, and a normal canvas Play click.
#[test]
#[ignore = "requires the packaged Windows Media Foundation video worker"]
fn workbook_relative_clip_scrubs_then_plays_changing_frames_across_its_loop() {
    let mut h = Harness::new("board_native_video_playback");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let workbook = h.base.join("Media review.slate");
    h.app.tab_mut().path = Some(workbook.clone());
    let filename = "video-motion.mp4";
    let original = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/atlas-core/tests/fixtures")
        .join(filename);
    std::fs::copy(original, h.base.join(filename)).unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(filename.into(), filename, 6904, 1, "video");
    h.app.place_items_on_board(&[item], Pos2::ZERO);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.video.looped = true;
        }
    });
    h.app.doc().save_to(&workbook).unwrap();
    h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&workbook).unwrap();
    h.app.video.decoder_held = false;
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.zoom_to_rect(rect);
    h.frame();
    let screen = h.app.board_xf().rect_w2s(rect);
    let at = Pos2::new(screen.left() + screen.width() * 0.7, screen.center().y);
    let moved = |input: &mut egui::RawInput| input.events.push(egui::Event::PointerMoved(at));
    h.frame_with(moved);
    let deadline = Instant::now() + Duration::from_secs(45);
    while h.app.video_window(id).is_none() || h.app.video_copy(id).is_none() {
        h.frame_with(moved);
        assert!(
            Instant::now() < deadline,
            "filmstrip did not produce a scrub frame"
        );
        assert!(
            !h.app.video.strips.get(&id).is_some_and(|s| s.failed),
            "filmstrip failed"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let start = h.app.video_shown(id).unwrap();
    assert!(
        (start - 0.7).abs() < 0.02,
        "expected late scrub, got {start}"
    );
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    let deadline = Instant::now() + Duration::from_secs(45);
    let mut first_pixels = None;
    let mut advanced = false;
    let mut wrapped = false;
    while !advanced || !wrapped {
        h.frame();
        assert!(
            h.app.video_playing_for_test(id),
            "Play stopped before a loop: {:?}",
            h.app.video.pool.as_ref().map(|p| p.health())
        );
        if let Some(live) = h.app.video.live.as_ref() {
            let first = first_pixels.get_or_insert_with(|| live.rgba.clone());
            advanced |= first != &live.rgba;
            wrapped |= live.time < start;
        }
        assert!(
            Instant::now() < deadline,
            "no changing loop after a single click"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    h.app.video_click(id);
    assert!(!h.app.video_playing_for_test(id));
}
