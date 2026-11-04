//! Seed real board nodes and inject decoder events; no codec or GL required.

use super::*;
use crate::app::tests::Harness;
use atlas_core::video::{VideoEvent, VideoFrame};

fn clip(harness: &mut Harness, name: &str) -> NodeId {
    harness.app.leave_home();
    harness.app.ensure_work_tab();
    let path = harness.base.join(name);
    std::fs::write(&path, b"fake clip; decoder is never started").unwrap();
    let items = harness.app.add_paths(&[path]);
    harness
        .app
        .place_items_on_board(&items, Pos2::new(400.0, 300.0));
    harness.app.doc().scene.nodes.last().unwrap().id
}

fn frame(color: u8) -> VideoFrame {
    VideoFrame {
        w: 1,
        h: 1,
        rgba: vec![color, 0, 0, 255],
    }
}

#[test]
fn video_body_hover_and_click_reach_the_player() {
    let mut h = Harness::new("video_body_input");
    let id = clip(&mut h, "movie.mp4");
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.zoom_to_rect(rect);
    h.frame();
    h.app.video_probe_for_test(id, 10.0);
    let at = h.app.board_xf().rect_w2s(rect).center();
    for _ in 0..3 {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(at)));
    }
    let (hovered, fraction) = h.app.video.hover.expect("body hover must scrub");
    assert_eq!(hovered, id);
    assert!((fraction - 0.5).abs() < 1e-4);
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
    assert!(h.app.video_playing_for_test(id), "body click must play");
}

fn seed(harness: &mut Harness, id: NodeId, color: u8) -> u64 {
    harness.app.video_probe_for_test(id, 10.0);
    let token = harness.app.video.strips[&id].token;
    harness.app.apply_video_event(VideoEvent::Strip {
        id: id.0,
        token,
        index: 0,
        time: 3.0,
        frame: frame(color),
    });
    harness.app.video.paused.insert(id, 3.0);
    harness.app.video.hover = Some((id, 0.3));
    let _ = harness
        .app
        .video_texture(&harness.ctx, id, &ImageAdjust::default())
        .unwrap();
    token
}

fn seed_play(harness: &mut Harness, id: NodeId) -> u64 {
    let token = harness.app.next_token();
    harness.app.video.play = Some(Play {
        id,
        token,
        from: 3.0,
        start: 0.0,
        end: 10.0,
        started: Instant::now(),
        looped: false,
    });
    harness.app.apply_video_event(VideoEvent::Play {
        id: id.0,
        token,
        time: 3.0,
        frame: frame(11),
    });
    token
}

#[test]
fn captured_video_relative_to_a_saved_workbook_keeps_its_resolved_source_and_playback() {
    let mut harness = Harness::new("video_workbook_relative_capture");
    harness.app.leave_home();
    harness.app.ensure_work_tab();
    harness.app.tab_mut().path = Some(harness.base.join("book.slate"));
    let relative = PathBuf::from("assets/captures/viewport.mp4");
    let absolute = harness.base.join(&relative);
    std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
    std::fs::write(&absolute, b"test owns these bytes; decoder is injected").unwrap();
    let item = harness
        .app
        .doc_mut()
        .add_item(relative, "viewport.mp4", 1, 1, "capture");
    harness
        .app
        .place_items_on_board(&[item], Pos2::new(400.0, 300.0));
    let id = harness.app.doc().scene.nodes.last().unwrap().id;
    assert_eq!(harness.app.video_at(id).unwrap().path, absolute);
    let strip = seed(&mut harness, id, 25);
    let play = seed_play(&mut harness, id);
    harness.app.sync_video_context();
    assert_eq!(harness.app.video.strips[&id].token, strip);
    assert_eq!(harness.app.video.play.as_ref().unwrap().token, play);
    assert_eq!(harness.app.video_copy(id).unwrap().2[0], 11);
    assert!(harness.app.current_video_source(id).is_some());
}

#[test]
fn workbook_node_id_collision_discards_every_old_derived_surface() {
    let mut harness = Harness::new("video_tab_identity");
    let old = clip(&mut harness, "old.mp4");
    let old_strip = seed(&mut harness, old, 11);
    let old_play = seed_play(&mut harness, old);
    harness.app.new_tab();
    let current = clip(&mut harness, "new.mp4");
    assert_eq!(
        old, current,
        "document-local identities intentionally collide"
    );
    assert!(harness.app.video_copy(current).is_none());
    assert!(harness.app.video_shown(current).is_none());
    harness.app.video_pump(&harness.ctx);
    assert!(harness.app.video.play.is_none());
    assert!(harness.app.video.live.is_none());
    assert!(harness.app.video.textures.is_empty());
    assert!(harness.app.video.paused.is_empty());
    assert!(harness.app.video.hover.is_none());

    let current_strip = seed(&mut harness, current, 22);
    assert!(current_strip > old_play && current_strip > old_strip);
    harness.app.apply_video_event(VideoEvent::Play {
        id: old.0,
        token: old_play,
        time: 8.0,
        frame: frame(99),
    });
    harness.app.apply_video_event(VideoEvent::Strip {
        id: old.0,
        token: old_strip,
        index: 0,
        time: 8.0,
        frame: frame(99),
    });
    assert!(harness.app.video.live.is_none());
    assert_eq!(harness.app.video_copy(current).unwrap().2[0], 22);
    assert_eq!(harness.app.video.paused[&current], 3.0);
}

#[test]
fn source_rebind_with_the_same_trim_requires_a_new_request_token() {
    let mut harness = Harness::new("video_source_identity");
    let id = clip(&mut harness, "old.mp4");
    let old_strip = seed(&mut harness, id, 11);
    let old_play = seed_play(&mut harness, id);
    harness.app.doc_mut().items[0].path = harness.base.join("replacement.mp4");
    assert!(harness.app.video_copy(id).is_none());
    harness.app.video_pump(&harness.ctx);
    assert!(harness.app.video.strips.is_empty());
    assert!(harness.app.video.textures.is_empty());
    assert!(harness.app.video.paused.is_empty());
    assert!(harness.app.video.play.is_none());
    let current = seed(&mut harness, id, 22);
    assert!(current > old_play && current > old_strip);
    harness.app.video.strips.get_mut(&id).unwrap().done = false;
    harness.app.apply_video_event(VideoEvent::StripDone {
        id: id.0,
        token: old_strip,
    });
    harness.app.apply_video_event(VideoEvent::Failed {
        id: id.0,
        token: old_play,
    });
    assert!(!harness.app.video.strips[&id].done);
    assert!(!harness.app.video.strips[&id].failed);
    assert_eq!(harness.app.video_copy(id).unwrap().2[0], 22);
}

#[test]
fn trim_change_and_home_leave_clear_derived_video_state() {
    let mut harness = Harness::new("video_trim_identity");
    let id = clip(&mut harness, "clip.mp4");
    seed(&mut harness, id, 11);
    seed_play(&mut harness, id);
    harness.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.video.start = 5.0;
        }
    });
    harness.app.video_pump(&harness.ctx);
    assert!(harness.app.video.sources.is_empty());
    assert!(harness.app.video.play.is_none());
    seed(&mut harness, id, 22);
    harness.app.at_home = true;
    harness.app.video_pump(&harness.ctx);
    assert!(harness.app.video.strips.is_empty());
    assert!(harness.app.video.paused.is_empty());
    assert!(harness.app.video.textures.is_empty());
    assert_eq!(harness.app.video.tab, None);
}

#[test]
fn strip_eviction_also_reclaims_paused_times_and_failed_source_identities() {
    let mut harness = Harness::new("video_bounded_identity");
    let first = clip(&mut harness, "clip-0.mp4");
    seed(&mut harness, first, 0);
    harness.app.video.hover = None;
    for index in 1..12 {
        let id = clip(&mut harness, &format!("clip-{index}.mp4"));
        seed(&mut harness, id, index);
        harness.app.video.hover = None;
        harness.app.video.strips.get_mut(&id).unwrap().failed = true;
        assert!(harness.app.video.sources.len() <= MAX_STRIPS);
        assert!(harness.app.video.paused.len() <= MAX_STRIPS);
        assert!(harness.app.video.textures.len() <= MAX_STRIPS);
    }
    assert!(!harness.app.video.sources.contains_key(&first));
    assert!(!harness.app.video.paused.contains_key(&first));
}
