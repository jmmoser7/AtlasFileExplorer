use super::super::output;
use super::*;
use std::time::Duration;

#[cfg(windows)]
#[test]
#[ignore = "native media review: writes one local synthetic MP4 using the production encoder"]
fn media_refinement_video_fixture() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review/media-refinement");
    std::fs::create_dir_all(&dir).unwrap();
    let started = Instant::now();
    let mut encoder =
        VideoEncoder::start_at(dir.join("sample.mp4"), 320, 180, 30, started).unwrap();
    for index in 0..30 {
        let mut rgba = vec![255; 320 * 180 * 4];
        for (pixel, color) in rgba.chunks_exact_mut(4).enumerate() {
            color[0] = (pixel % 320) as u8;
            color[1] = (pixel / 320) as u8;
            color[2] = index * 7;
        }
        let sample = CapturedFrame {
            generation: 1,
            w: 320,
            h: 180,
            rgba,
            captured_at: started + Duration::from_millis(index as u64 * 33),
            recycle: None,
        };
        let _ = encoder.try_frame(sample);
        std::thread::sleep(Duration::from_millis(10));
    }
    encoder.finish_at(started + Duration::from_secs(1));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(result) = encoder.take_result() {
            println!("Review video: {}", result.unwrap().display());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "production video encoder completes"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn frame(encoder: &VideoEncoder, millis: u64) -> CapturedFrame {
    CapturedFrame {
        generation: 7,
        w: 2,
        h: 2,
        rgba: vec![255; 16],
        captured_at: encoder.started + Duration::from_millis(millis),
        recycle: None,
    }
}

#[test]
fn frame_queue_is_bounded_and_refuses_without_waiting() {
    let (mut encoder, worker) = VideoEncoder::channels(2, 2);
    for at in 0..QUEUE_FRAMES {
        let frame = frame(&encoder, at as u64);
        assert!(encoder.try_frame(frame));
    }
    let overflow = frame(&encoder, 100);
    assert!(!encoder.try_frame(overflow));
    assert_eq!(worker.frames.len(), QUEUE_FRAMES);
}

#[test]
fn queue_drops_do_not_compress_recording_time() {
    let origin = Instant::now();
    let mut timing = Timeline::new(origin);
    let first = timing.advance(origin + Duration::from_millis(100)).unwrap();
    assert_eq!(
        first,
        SampleTiming {
            start: 0,
            duration: 1_000_000
        }
    );
    // No samples arrive while the bounded queue drops busy work for 2 seconds.
    let after_load = timing
        .advance(origin + Duration::from_millis(2100))
        .unwrap();
    assert_eq!(
        after_load,
        SampleTiming {
            start: 1_000_000,
            duration: 20_000_000
        }
    );
    let last = timing.finish(origin + Duration::from_millis(3100));
    assert_eq!(
        last,
        SampleTiming {
            start: 21_000_000,
            duration: 10_000_000
        }
    );
    assert_eq!(
        first.duration + after_load.duration + last.duration,
        31_000_000
    );
}

#[test]
fn duplicate_or_out_of_order_frames_cannot_reverse_time() {
    let origin = Instant::now();
    let mut timing = Timeline::new(origin);
    assert!(timing.advance(origin + Duration::from_secs(1)).is_some());
    assert!(timing.advance(origin + Duration::from_secs(1)).is_none());
    assert!(timing.advance(origin).is_none());
    assert_eq!(timing.finish(origin).duration, 1);
}

#[test]
fn finish_closes_input_and_records_stop_before_queue_drain() {
    let (mut encoder, worker) = VideoEncoder::channels(2, 2);
    let captured = frame(&encoder, 0);
    assert!(encoder.try_frame(captured));
    encoder.finish();
    encoder.finish();
    assert!(worker.stop.try_recv().is_ok());
    assert!(worker.stop.try_recv().is_err());
    assert!(worker.frames.try_recv().is_ok());
    assert_eq!(
        worker.frames.try_recv().err(),
        Some(crossbeam_channel::TryRecvError::Disconnected)
    );
    let late = frame(&encoder, 10);
    assert!(!encoder.try_frame(late));
    assert!(worker.check_cancelled().is_ok());
}

#[test]
fn paused_finish_uses_projected_stop_instead_of_time_spent_choosing_a_destination() {
    let (mut encoder, worker) = VideoEncoder::channels(2, 2);
    let stopped = encoder.started + Duration::from_secs(3);
    encoder.finish_at(stopped);
    assert_eq!(worker.stop.try_recv().unwrap(), stopped);
    assert!(worker.frames.try_recv().is_err());
}

#[test]
fn resume_accepts_only_the_new_capture_span_generation() {
    let (mut encoder, worker) = VideoEncoder::channels(2, 2);
    let initial = frame(&encoder, 0);
    assert!(encoder.try_frame(initial));
    encoder.resume_generation(8);
    let stale = frame(&encoder, 30);
    assert!(!encoder.try_frame(stale));
    let mut resumed = frame(&encoder, 60);
    resumed.generation = 8;
    assert!(encoder.try_frame(resumed));
    assert_eq!(worker.frames.len(), 2);
}

#[test]
fn wrong_generation_size_and_pre_start_frames_are_rejected() {
    let (mut encoder, worker) = VideoEncoder::channels(2, 2);
    let first = frame(&encoder, 0);
    assert!(encoder.try_frame(first));
    let mut stale = frame(&encoder, 1);
    stale.generation = 8;
    assert!(!encoder.try_frame(stale));
    let mut wrong_size = frame(&encoder, 2);
    wrong_size.w = 4;
    assert!(!encoder.try_frame(wrong_size));
    let mut wrong_buffer = frame(&encoder, 3);
    wrong_buffer.rgba.pop();
    assert!(!encoder.try_frame(wrong_buffer));
    let mut old = frame(&encoder, 4);
    old.captured_at = encoder.started - Duration::from_millis(1);
    assert!(!encoder.try_frame(old));
    assert_eq!(worker.frames.len(), 1);
}

#[test]
fn refused_and_consumed_frames_return_their_owned_buffers() {
    let (mut encoder, worker) = VideoEncoder::channels(2, 2);
    let (recycle, returned) = bounded(2);
    let mut invalid = frame(&encoder, 0);
    invalid.w = 4;
    invalid.recycle = Some(recycle.clone());
    assert!(!encoder.try_frame(invalid));
    assert_eq!(returned.try_recv().unwrap().len(), 16);
    let mut accepted = frame(&encoder, 1);
    accepted.recycle = Some(recycle);
    assert!(encoder.try_frame(accepted));
    drop(worker.frames.try_recv().unwrap());
    assert_eq!(returned.try_recv().unwrap().len(), 16);
}

#[test]
fn a_full_buffer_return_queue_never_blocks_frame_drop() {
    let (encoder, _) = VideoEncoder::channels(2, 2);
    let (recycle, returned) = bounded(1);
    recycle.try_send(vec![9; 16]).unwrap();
    let mut captured = frame(&encoder, 0);
    captured.recycle = Some(recycle);
    drop(captured);
    assert_eq!(returned.len(), 1);
    assert_eq!(returned.try_recv().unwrap(), vec![9; 16]);
}

#[test]
fn encoder_drop_cancels_worker_without_joining_it() {
    let (encoder, worker) = VideoEncoder::channels(2, 2);
    drop(encoder);
    assert!(worker.check_cancelled().is_err());
    assert!(worker.frames.recv().is_err());
}

#[test]
fn worker_disconnection_is_reported_once_as_a_terminal_error() {
    let (mut encoder, _) = VideoEncoder::channels(2, 2);
    assert!(encoder.take_result().unwrap().is_err());
    assert!(encoder.take_result().is_none());
}

#[test]
fn h264_input_limit_rejects_odd_or_oversized_frames() {
    assert!(validate_format(1920, 1080, 30).is_ok());
    for (w, h, fps) in [
        (0, 100, 30),
        (1919, 1080, 30),
        (1920, 1081, 30),
        (1922, 1080, 30),
        (1920, 1080, 0),
        (1920, 1080, 31),
    ] {
        assert!(validate_format(w, h, fps).is_err());
    }
}

fn destination(name: &str) -> PathBuf {
    static NEXT_TEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "slate-capture-test-{}-{id}-{name}.mp4",
        std::process::id()
    ))
}

#[test]
fn cancelled_draft_preserves_existing_destination() {
    let destination = destination("cancelled");
    std::fs::write(&destination, b"existing recording").unwrap();
    let draft = output::Draft::reserve(destination.clone()).unwrap();
    let path = draft.path().to_path_buf();
    std::fs::write(&path, b"unfinished recording").unwrap();
    drop(draft);
    assert!(!path.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"existing recording");
    std::fs::remove_file(destination).unwrap();
}

#[test]
fn completed_draft_replaces_destination_and_removes_its_temporary_name() {
    let destination = destination("committed");
    std::fs::write(&destination, b"existing recording").unwrap();
    let draft = output::Draft::reserve(destination.clone()).unwrap();
    let path = draft.path().to_path_buf();
    std::fs::write(&path, b"finalized recording").unwrap();
    assert_eq!(draft.commit().unwrap(), destination);
    assert!(!path.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"finalized recording");
    std::fs::remove_file(destination).unwrap();
}

#[cfg(not(windows))]
#[test]
fn video_capture_reports_unavailable_without_creating_a_file() {
    let destination = destination("unavailable");
    assert!(VideoEncoder::start(destination.clone(), 64, 64, 30).is_err());
    assert!(!destination.exists());
}
