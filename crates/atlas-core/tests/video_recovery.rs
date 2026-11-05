#![cfg(windows)]
use atlas_core::video::{VideoEvent, VideoFrame, VideoPool, STRIP_FRAMES};
use std::time::{Duration, Instant};

#[test]
fn h264_video_decodes_scrub_and_advancing_playback_frames() {
    assert!(std::path::Path::new(env!("CARGO_BIN_EXE_slate-video-worker")).is_file());
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let pool = VideoPool::spawn();
    pool.request_strip(1, 1, path.clone(), 0.0, None);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "video scrub decoder timed out");
        if let Some(event) = pool.try_recv() {
            match event {
                VideoEvent::Strip { frame, .. } => {
                    assert_eq!(frame.rgba.len(), (frame.w * frame.h * 4) as usize);
                    break;
                }
                VideoEvent::Failed { .. } => panic!("H.264 scrub decoding failed"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    pool.play(1, 2, path, 0.0, None, 0.0, false);
    let mut first = None;
    loop {
        assert!(
            Instant::now() < deadline,
            "video playback decoder timed out"
        );
        if let Some(event) = pool.try_recv() {
            match event {
                VideoEvent::Play {
                    token: 2,
                    time,
                    frame,
                    ..
                } => {
                    if let Some((before, pixels)) = &first {
                        if time > *before && frame.rgba != *pixels {
                            pool.stop();
                            break;
                        }
                    } else {
                        first = Some((time, frame.rgba));
                    }
                }
                VideoEvent::Failed { token: 2, .. } => panic!("H.264 playback failed"),
                VideoEvent::Ended { token: 2, .. } => panic!("video ended without advancing"),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_event(pool: &VideoPool, token: u64, timeout: Duration) -> VideoEvent {
    let deadline = Instant::now() + timeout;
    loop {
        assert!(
            Instant::now() < deadline,
            "video token {token} timed out; health={:?}",
            pool.health()
        );
        if let Some(event) = pool.try_recv() {
            let current = match &event {
                VideoEvent::Probed { token, .. }
                | VideoEvent::Strip { token, .. }
                | VideoEvent::StripDone { token, .. }
                | VideoEvent::Play { token, .. }
                | VideoEvent::Ended { token, .. }
                | VideoEvent::Failed { token, .. } => *token,
            };
            if current == token {
                return event;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn check_pixels(frame: &VideoFrame) {
    assert_eq!(frame.rgba.len(), frame.w as usize * frame.h as usize * 4);
    assert!(frame.rgba.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
}
fn play_to_end(
    pool: &VideoPool,
    path: std::path::PathBuf,
    token: u64,
    from: f32,
) -> (f32, Vec<u8>) {
    pool.play(1, token, path, 0.0, None, from, false);
    let mut first = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "play-to-end exceeded deadline");
        match wait_event(
            pool,
            token,
            deadline.saturating_duration_since(Instant::now()),
        ) {
            VideoEvent::Play { time, frame, .. } => {
                check_pixels(&frame);
                first.get_or_insert((time, frame.rgba));
            }
            VideoEvent::Ended { .. } => return first.expect("ended without any decoded frame"),
            VideoEvent::Failed { .. } => {
                panic!("play failed for token {token}: {:?}", pool.health())
            }
            _ => {}
        }
    }
}

#[test]
fn malformed_clip_and_replay_do_not_poison_next_real_clip() {
    let directory = TestDirectory::new();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let first = directory.path().join("first.mp4");
    let second = directory.path().join("second.mp4");
    std::fs::copy(&source, &first).unwrap();
    std::fs::copy(&source, &second).unwrap();
    let malformed = directory.path().join("malformed.mp4");
    std::fs::write(&malformed, b"deliberately invalid video container").unwrap();
    let pool = VideoPool::spawn();
    pool.play(1, 10, malformed, 0.0, None, 0.0, false);
    loop {
        if matches!(
            wait_event(&pool, 10, Duration::from_secs(10)),
            VideoEvent::Failed { .. }
        ) {
            break;
        }
    }
    assert!(pool.health().failures >= 1);
    let normal = play_to_end(&pool, first.clone(), 11, 0.0);
    let replay = play_to_end(&pool, first, 12, f32::MAX);
    assert!(
        replay.0 < 0.2,
        "replay must restart from trim in, got {}",
        replay.0
    );
    assert_eq!(
        normal.1, replay.1,
        "replay first pixels must match initial playback"
    );
    play_to_end(&pool, second, 13, f32::NAN);
    pool.cancel_all();
}

#[test]
fn full_filmstrip_retains_all_96_sample_positions_with_bounded_delivery() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let pool = VideoPool::spawn();
    pool.request_strip(1, 20, path, 0.0, None);
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = std::collections::HashSet::new();
    loop {
        assert!(Instant::now() < deadline, "full filmstrip timed out");
        match wait_event(
            &pool,
            20,
            deadline.saturating_duration_since(Instant::now()),
        ) {
            VideoEvent::Strip { index, frame, .. } => {
                check_pixels(&frame);
                seen.insert(index);
            }
            VideoEvent::StripDone { .. } => break,
            VideoEvent::Failed { .. } => panic!("filmstrip failed: {:?}", pool.health()),
            _ => {}
        }
    }
    assert_eq!(seen.len(), STRIP_FRAMES);
    assert!(pool.health().queued_frame_bytes <= 8 * 1024 * 1024);
}

#[test]
fn optional_verified_real_corpus_decodes_tiny_frames_and_dynamic_format_changes() {
    let Some(directory) = std::env::var_os("ATLAS_VIDEO_CORPUS_DIR") else {
        eprintln!("Optional verified external corpus not supplied; builtin H264 recovery tests remain mandatory");
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let pool = VideoPool::spawn();
    for (index, name) in [
        "2x2-green.mp4",
        "400x300-red-resize-200x150-green.mp4",
        "test-1s.mp4",
        "test.mp4",
    ]
    .iter()
    .enumerate()
    {
        let token = 100 + index as u64;
        let path = directory.join(name);
        assert!(
            path.is_file(),
            "missing requested verified corpus file {name}"
        );
        pool.play(index as u64 + 1, token, path, 0.0, None, 0.0, false);
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut sizes = std::collections::HashSet::new();
        let mut colors = std::collections::HashSet::new();
        let mut below_platform_minimum = false;
        let mut reference_transitions = 0;
        loop {
            assert!(
                Instant::now() < deadline,
                "corpus playback {name} timed out"
            );
            match wait_event(
                &pool,
                token,
                deadline.saturating_duration_since(Instant::now()),
            ) {
                VideoEvent::Play { time, frame, .. } => {
                    check_pixels(&frame);
                    sizes.insert((frame.w, frame.h));
                    let px = &frame.rgba[..4];
                    let reference_transition = name.starts_with("400x300")
                        && (frame.w, frame.h) == (200, 150)
                        && (time - 0.983).abs() < 0.0001;
                    if reference_transition {
                        recovery_pixels::assert_reference_transition(&frame);
                        reference_transitions += 1;
                        assert!(
                            reference_transitions <= 1,
                            "duplicateencodedtransitionframe"
                        );
                    } else {
                        colors.insert(if px[0] > px[1] { "red" } else { "green" });
                        if name.starts_with("400x300") {
                            recovery_pixels::assert_solid(&frame, (frame.w, frame.h) == (400, 300));
                        }
                    }
                    if *name == "2x2-green.mp4" {
                        assert_eq!((frame.w, frame.h), (2, 2));
                        assert!(
                            frame
                                .rgba
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .all(|px| px[1] > 100 && px[0] < 20 && px[2] < 20),
                            "tiny clip rows/channels corrupt"
                        );
                    }
                }
                VideoEvent::Ended { .. } => break,
                VideoEvent::Failed { .. } if *name == "2x2-green.mp4" => {
                    // Microsoft's H264 MFT documents a48x48minimum. A codec
                    // that accepts this tiny stream must still pass exact
                    // dimensions/colors; rejection must recover to real video.
                    below_platform_minimum = true;
                    eprintln!("2x2 H264 rejected below documented48x48minimum; validating fresh normal decode");
                    break;
                }
                VideoEvent::Failed { .. } => {
                    panic!("verified corpus failed for {name}: {:?}", pool.health())
                }
                _ => {}
            }
        }
        if below_platform_minimum {
            let normal = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/video-motion.mp4");
            let recovered = play_to_end(&pool, normal, 400, 0.0);
            assert!(
                !recovered.1.is_empty(),
                "tiny-clip rejection poisoned normal decoding"
            );
            continue;
        }
        assert!(!sizes.is_empty(), "no real frames for {name}");
        if name.starts_with("400x300") {
            assert!(
                sizes.contains(&(400, 300)) && sizes.contains(&(200, 150)),
                "dynamic dimensions not preserved: {sizes:?}"
            );
            assert!(
                colors.contains("red") && colors.contains("green"),
                "dynamic color transition not decoded: {colors:?}"
            );
        }
    }
}

struct TestDirectory(std::path::PathBuf);
impl TestDirectory {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "atlas-video-recovery-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn initial_strip_play_pause_then_scrub_resumes_original_strip_token() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let pool = VideoPool::spawn();
    pool.request_strip(1, 30, path.clone(), 0.0, None);
    loop {
        match wait_event(&pool, 30, Duration::from_secs(10)) {
            VideoEvent::Strip { .. } => break,
            VideoEvent::Failed { .. } => panic!("initial strip failed"),
            _ => {}
        }
    }
    pool.play(1, 31, path, 0.0, None, 0.0, false);
    loop {
        match wait_event(&pool, 31, Duration::from_secs(10)) {
            VideoEvent::Play { .. } => break,
            VideoEvent::Failed { .. } | VideoEvent::Ended { .. } => {
                panic!("initial playback failed")
            }
            _ => {}
        }
    }
    pool.stop();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut seen = std::collections::HashSet::new();
    loop {
        match wait_event(
            &pool,
            30,
            deadline.saturating_duration_since(Instant::now()),
        ) {
            VideoEvent::Strip { index, frame, .. } => {
                check_pixels(&frame);
                seen.insert(index);
            }
            VideoEvent::StripDone { .. } => break,
            VideoEvent::Failed { .. } => panic!("paused strip failed instead of resuming"),
            _ => {}
        }
    }
    assert_eq!(
        seen.len(),
        STRIP_FRAMES,
        "strip must restart all indexes with original token"
    );
    assert_eq!(pool.health().failures, 0);
}

#[cfg(debug_assertions)]
#[test]
fn helper_hang_crash_bad_packet_and_exit_are_reaped_before_next_clip_decodes() {
    use atlas_core::video::VideoWorkerFault;
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    for (index, fault) in [
        VideoWorkerFault::Hang,
        VideoWorkerFault::Crash,
        VideoWorkerFault::Malformed,
        VideoWorkerFault::Exit,
    ]
    .into_iter()
    .enumerate()
    {
        let pool = VideoPool::spawn_with_diagnostic_fault(fault);
        let token = 500 + index as u64 * 2;
        pool.play(1, token, source.clone(), 0.0, None, 0.0, false);
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut pid = None;
        loop {
            if let Some(current) = pool.health().worker_pid {
                pid = Some(current);
            }
            assert!(
                Instant::now() < deadline,
                "worker fault {fault:?} was not bounded"
            );
            if let Some(VideoEvent::Failed { token: got, .. }) = pool.try_recv() {
                assert_eq!(got, token);
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let health = pool.health();
        assert_eq!(health.restarts, 1, "fault {fault:?}: {health:?}");
        assert_eq!(health.failures, 1, "fault {fault:?}: {health:?}");
        assert_eq!(
            health.worker_pid, None,
            "failed helper must already be reaped"
        );
        if matches!(fault, VideoWorkerFault::Hang) {
            assert!(pid.is_some(), "must observe hung helper PID");
        }
        if let Some(pid) = pid {
            assert_process_is_gone(pid);
        }
        let recovered = play_to_end(&pool, source.clone(), token + 1, 0.0);
        assert!(!recovered.1.is_empty());
    }
}

#[cfg(debug_assertions)]
fn assert_process_is_gone(pid: u32) {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    if let Ok(handle) = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        let mut code = 259u32; // STILL_ACTIVE.
        let result = unsafe { GetExitCodeProcess(handle, &mut code) };
        unsafe { CloseHandle(handle) }.unwrap();
        result.unwrap();
        assert_ne!(code, 259, "owned decoder helper {pid} survived recovery");
    }
}

#[test]
fn context_cancel_invalidates_pending_strips_and_clears_queued_pixels() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let pool = VideoPool::spawn();
    pool.request_strip(1, 700, path.clone(), 0.0, None);
    loop {
        match wait_event(&pool, 700, Duration::from_secs(10)) {
            VideoEvent::Strip { .. } => break,
            VideoEvent::Failed { .. } => panic!("context fixture failed"),
            _ => {}
        }
    }
    pool.request_strip(2, 701, path.clone(), 0.0, None);
    pool.cancel_all();
    let deadline = Instant::now() + Duration::from_secs(2);
    while pool.health().worker_pid.is_some() {
        assert!(
            Instant::now() < deadline,
            "context cancel did not reap active helper"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(40));
    assert!(
        pool.try_recv().is_none(),
        "cancelled context emitted stale events"
    );
    assert_eq!(pool.health().queued_frame_bytes, 0);
    assert_eq!(pool.health().failures, 0);
    play_to_end(&pool, path, 702, 0.0);
}

#[path = "../src/video/recovery_timing_tests.rs"]
mod recovery_timing_tests;

#[path = "../src/video/recovery_pixels.rs"]
mod recovery_pixels;

#[path = "../src/video/recovery_pressure_tests.rs"]
mod recovery_pressure_tests;

#[path = "../src/video/recovery_path_tests.rs"]
mod recovery_path_tests;
