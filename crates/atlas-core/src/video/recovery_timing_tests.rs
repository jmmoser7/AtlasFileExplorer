//! Real-worker timing cases included by the video_recovery integration target.
use super::*;

#[test]
fn nonzero_trim_and_resume_keep_the_same_calibrated_video_origin() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video-motion.mp4");
    let pool = VideoPool::spawn();
    for (token, from) in [(800, 0.5), (801, 0.625)] {
        pool.play(1, token, path.clone(), 0.5, Some(0.875), from, false);
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut first = None;
        let mut last = None;
        loop {
            match wait_event(
                &pool,
                token,
                deadline.saturating_duration_since(Instant::now()),
            ) {
                VideoEvent::Play { time, frame, .. } => {
                    check_pixels(&frame);
                    assert!(
                        (from..=0.875).contains(&time),
                        "frame outside normalized trim: {time}"
                    );
                    first.get_or_insert(time);
                    last = Some(time);
                }
                VideoEvent::Ended { .. } => break,
                VideoEvent::Failed { .. } => panic!("normalized trim failed: {:?}", pool.health()),
                _ => {}
            }
        }
        assert_eq!(
            first,
            Some(from),
            "seek must land on this fixture's exact normalized frame"
        );
        assert!(
            last.is_some_and(|time| time >= 0.75),
            "trim lost its final video frames"
        );
    }
    // Replay still starts at the trim in-point through the same origin map.
    pool.play(1, 802, path, 0.5, Some(0.875), 0.875, false);
    loop {
        match wait_event(&pool, 802, Duration::from_secs(10)) {
            VideoEvent::Play { time, .. } => {
                assert_eq!(time, 0.5);
                break;
            }
            VideoEvent::Failed { .. } | VideoEvent::Ended { .. } => panic!("trim replay failed"),
            _ => {}
        }
    }
    pool.cancel_all();
}
