//! Actual packaged-worker pressure and cancellation cases for video_recovery.
use super::*;

fn motion() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4")
}
fn fill_with_real_strips(pool: &VideoPool) {
    for id in 1..=4 {
        pool.request_strip(id, 900 + id, motion(), 0.0, None);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while pool.health().queued_frame_bytes < 3_000_000 {
        assert!(
            Instant::now() < deadline,
            "real strip queue never filled: {:?}",
            pool.health()
        );
        assert_eq!(pool.health().failures, 0);
        std::thread::sleep(Duration::from_millis(5));
    }
    // Deliberately leave the consumer stopped long enough for the next record
    // to encounter Full. This fills only the bounded production queue.
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(
        pool.health().dropped_frames,
        0,
        "Strip frames are not disposable"
    );
    assert!(pool.health().queued_frame_bytes <= 8 * 1024 * 1024);
}

#[test]
fn stalled_consumer_keeps_every_real_strip_index_and_done_order() {
    let pool = VideoPool::spawn();
    fill_with_real_strips(&pool);
    let mut seen = std::collections::HashMap::<u64, std::collections::HashSet<u16>>::new();
    let mut done = std::collections::HashSet::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while done.len() != 4 {
        assert!(
            Instant::now() < deadline,
            "stalled strips did not resume: {:?}",
            pool.health()
        );
        if let Some(event) = pool.try_recv() {
            match event {
                VideoEvent::Strip {
                    id, index, frame, ..
                } => {
                    assert!(!done.contains(&id), "pixel arrived after Done");
                    check_pixels(&frame);
                    seen.entry(id).or_default().insert(index);
                }
                VideoEvent::StripDone { id, .. } => {
                    assert_eq!(
                        seen.get(&id).map_or(0, |indexes| indexes.len()),
                        STRIP_FRAMES
                    );
                    done.insert(id);
                }
                VideoEvent::Failed { .. } => panic!("UI backpressure became a decoder failure"),
                _ => {}
            }
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    assert_eq!(pool.health().dropped_frames, 0);
}

#[test]
fn play_and_context_cancel_remain_responsive_while_strip_output_is_full() {
    let pool = VideoPool::spawn();
    fill_with_real_strips(&pool);
    pool.play(9, 1000, motion(), 0.0, None, 0.0, false);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut first = None;
    loop {
        assert!(
            Instant::now() < deadline,
            "Play waited behind Full strip output"
        );
        if let Some(event) = pool.try_recv() {
            match event {
                VideoEvent::Play {
                    token: 1000,
                    time,
                    frame,
                    ..
                } => {
                    check_pixels(&frame);
                    if first.is_some_and(|before| time > before) {
                        break;
                    }
                    first = Some(time);
                }
                VideoEvent::Failed { .. } => panic!("Full queue poisoned Play"),
                _ => {}
            }
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    pool.stop();
    // The interrupted original strips resume; stop draining and fill again.
    let deadline = Instant::now() + Duration::from_secs(10);
    while pool.health().queued_frame_bytes < 3_000_000 {
        assert!(
            Instant::now() < deadline,
            "interrupted strips did not resume"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(100));
    pool.cancel_all();
    let deadline = Instant::now() + Duration::from_secs(2);
    while pool.health().worker_pid.is_some() {
        assert!(
            Instant::now() < deadline,
            "Full queue blocked context cancellation"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(pool.health().queued_frame_bytes, 0);
    assert_eq!(pool.health().failures, 0);
    assert!(pool.try_recv().is_none());
    play_to_end(&pool, motion(), 1001, 0.0);
}

#[test]
fn optional_large_verified_strip_survives_byte_budget_backpressure() {
    let Some(directory) = std::env::var_os("ATLAS_VIDEO_CORPUS_DIR") else {
        eprintln!("Optional large-strip corpus not supplied; real multi-strip pressure tests remain mandatory");
        return;
    };
    let pool = VideoPool::spawn();
    pool.request_strip(
        1,
        1100,
        std::path::PathBuf::from(directory).join("test-1s.mp4"),
        0.0,
        None,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while pool.health().queued_frame_bytes < 8 * 1024 * 1024 - 320 * 320 * 4 {
        assert!(
            Instant::now() < deadline,
            "large strip did not reach byte budget: {:?}",
            pool.health()
        );
        assert_eq!(pool.health().failures, 0);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(150));
    assert!(pool.health().queued_frame_bytes <= 8 * 1024 * 1024);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut indexes = std::collections::HashSet::new();
    loop {
        match wait_event(
            &pool,
            1100,
            deadline.saturating_duration_since(Instant::now()),
        ) {
            VideoEvent::Strip { index, frame, .. } => {
                check_pixels(&frame);
                indexes.insert(index);
            }
            VideoEvent::StripDone { .. } => {
                assert_eq!(indexes.len(), STRIP_FRAMES);
                break;
            }
            VideoEvent::Failed { .. } => panic!("large-strip UI stall became codec failure"),
            _ => {}
        }
    }
    assert_eq!(pool.health().dropped_frames, 0);
}

#[test]
fn optional_adaptive_strip_finishes_all_96_targets_within_the_existing_bound() {
    let Some(directory) = std::env::var_os("ATLAS_VIDEO_CORPUS_DIR") else {
        eprintln!(
            "Optional adaptive corpus not supplied; mandatory real strip tests remain enabled"
        );
        return;
    };
    let pool = VideoPool::spawn();
    let started = Instant::now();
    pool.request_strip(
        1,
        1200,
        std::path::PathBuf::from(directory).join("400x300-red-resize-200x150-green.mp4"),
        0.0,
        None,
    );
    let deadline = started + Duration::from_secs(10);
    let mut indexes = std::collections::HashSet::new();
    let mut dimensions = std::collections::HashSet::new();
    loop {
        match wait_event(
            &pool,
            1200,
            deadline.saturating_duration_since(Instant::now()),
        ) {
            VideoEvent::Strip { index, frame, .. } => {
                check_pixels(&frame);
                indexes.insert(index);
                dimensions.insert((frame.w, frame.h));
            }
            VideoEvent::StripDone { .. } => break,
            VideoEvent::Failed { .. } => {
                panic!("adaptive forward sampling failed: {:?}", pool.health())
            }
            _ => {}
        }
    }
    assert_eq!(indexes.len(), STRIP_FRAMES);
    assert!(
        dimensions.contains(&(320, 240)) && dimensions.contains(&(200, 150)),
        "adaptive strip lost visible dimension change: {dimensions:?}"
    );
    assert_eq!(pool.health().dropped_frames, 0);
    assert!(started.elapsed() < Duration::from_secs(10));
    eprintln!(
        "adaptive96 strip elapsed={:?}; MF strip.samples receipt contains bounded seek/read counts",
        started.elapsed()
    );
}
