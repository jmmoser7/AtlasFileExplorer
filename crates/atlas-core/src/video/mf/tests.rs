use crate::video::{VideoEvent, VideoPool};
use std::time::{Duration, Instant};

#[test]
fn h264_video_decodes_scrub_and_advancing_playback_frames() {
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
