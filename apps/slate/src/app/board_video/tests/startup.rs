//! Startup latency is outside the media clock; no real waits or codec are needed.
use super::*;
use std::time::Duration;

fn waiting_clip(h: &mut Harness) -> (NodeId, u64) {
    let id = clip(h, "short.mp4");
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.video.start = 2.0;
            image.video.end = Some(2.25);
            image.video.looped = false;
        }
    });
    h.app.video_probe_for_test(id, 10.0);
    h.app.video.paused.insert(id, 2.05);
    h.app.video_click(id);
    (id, h.app.video.play.as_ref().unwrap().token)
}

#[test]
fn delayed_probe_and_no_frames_cannot_spend_the_trim_window() {
    let mut h = Harness::new("video_delayed_probe");
    let (id, token) = waiting_clip(&mut h);
    let delayed = Instant::now() + Duration::from_secs(30);
    h.app.video_pump_at(&h.ctx, delayed);
    assert!(h.app.video_playing_for_test(id));
    h.app.apply_video_event(VideoEvent::Probed {
        id: id.0,
        token,
        duration: 10.0,
    });
    h.app
        .video_pump_at(&h.ctx, delayed + Duration::from_secs(30));
    let play = h
        .app
        .video
        .play
        .as_ref()
        .expect("one Play must remain pending until a frame or terminal event");
    assert!(play.started.is_none());
    assert_eq!(play.from, 2.05);
    assert_eq!(play_clock(play, delayed), 2.05);
}

#[test]
fn pause_before_first_frame_keeps_the_requested_position() {
    let mut h = Harness::new("video_pause_before_frame");
    let (id, _) = waiting_clip(&mut h);
    h.app
        .video_pump_at(&h.ctx, Instant::now() + Duration::from_secs(30));
    h.app.video_click(id);
    assert!(!h.app.video_playing_for_test(id));
    assert_eq!(h.app.video.paused[&id], 2.05);
}

#[test]
fn pending_video_waits_for_worker_results_without_continuous_repaint() {
    let mut h = Harness::new("video_wait_without_redraw");
    let (id, _) = waiting_clip(&mut h);
    let ctx = egui::Context::default();
    for _ in 0..4 {
        let _ = ctx.run(egui::RawInput::default(), |_| {});
    }
    let out = ctx.run(egui::RawInput::default(), |ctx| {
        h.app.video_pump_at(ctx, Instant::now());
    });
    assert!(h.app.video_playing_for_test(id));
    assert!(out.viewport_output[&egui::ViewportId::ROOT].repaint_delay > Duration::ZERO);
}

#[test]
fn a_failed_strip_keeps_the_independent_live_frames_and_playable_duration() {
    let mut h = Harness::new("video_preview_failure");
    let id = clip(&mut h, "movie.mp4");
    h.app.video_click(id);
    let play = h.app.video.play.as_ref().unwrap().token;
    let strip = h.app.video.strips[&id].token;
    h.app.apply_video_event(VideoEvent::Failed {
        id: id.0,
        token: strip,
    });
    assert!(h.app.video.strips[&id].failed);
    assert!(h.app.video_playing_for_test(id));
    h.app.apply_video_event(VideoEvent::Probed {
        id: id.0,
        token: play,
        duration: 1.0,
    });
    assert_eq!(h.app.video_window(id), Some((0.0, 1.0)));
    h.app.apply_video_event(VideoEvent::Play {
        id: id.0,
        token: play,
        time: 0.75,
        frame: frame(45),
    });
    h.app.video_click(id);
    assert_eq!(h.app.video_copy(id).unwrap().2[0], 45);
    assert_eq!(h.app.video_control_hint(id), "Play");
}

#[test]
fn a_failed_play_leaves_the_pending_preview_free_to_deliver_its_frames() {
    let mut h = Harness::new("video_independent_preview");
    let id = clip(&mut h, "movie.mp4");
    h.app.video_click(id);
    let play = h.app.video.play.as_ref().unwrap().token;
    let strip = h.app.video.strips[&id].token;
    h.app.apply_video_event(VideoEvent::Failed {
        id: id.0,
        token: play,
    });
    assert!(h.app.video.strips[&id].inflight);
    h.app.apply_video_event(VideoEvent::Probed {
        id: id.0,
        token: strip,
        duration: 1.0,
    });
    h.app.apply_video_event(VideoEvent::Strip {
        id: id.0,
        token: strip,
        index: 0,
        time: 0.0,
        frame: frame(27),
    });
    assert_eq!(h.app.video_copy(id).unwrap().2[0], 27);
    assert_eq!(
        h.app.video_control_hint(id),
        "Video unavailable. Click Play to retry."
    );
}

#[test]
fn first_frame_anchors_to_decoded_time_then_the_real_clock_reaches_end() {
    let mut h = Harness::new("video_first_frame_anchor");
    let (id, token) = waiting_clip(&mut h);
    h.app.apply_video_event(VideoEvent::Play {
        id: id.0,
        token,
        time: 2.1,
        frame: frame(45),
    });
    let play = h.app.video.play.as_ref().unwrap();
    assert_eq!(play.from, 2.1);
    let started = play.started.expect("first decoded frame starts media time");
    h.app
        .video_pump_at(&h.ctx, started + Duration::from_millis(50));
    assert!(h.app.video_playing_for_test(id));
    h.app
        .video_pump_at(&h.ctx, started + Duration::from_secs(1));
    assert!(!h.app.video_playing_for_test(id));
    assert_eq!(h.app.video.paused[&id], 2.25);
}
