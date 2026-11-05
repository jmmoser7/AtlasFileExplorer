use super::*;
fn frame(bytes: usize) -> VideoFrame {
    VideoFrame {
        w: 1,
        h: 1,
        rgba: vec![0; bytes],
    }
}
#[test]
fn stalled_big_strip_is_lossless_bounded_and_done_follows_all_indexes() {
    let events = Events::new();
    let mut seen = Vec::new();
    let mut blocked = 0;
    for index in 0..96 {
        let mut event = VideoEvent::Strip {
            id: 1,
            token: 1,
            index,
            time: 0.0,
            frame: frame(320 * 180 * 4),
        };
        loop {
            match events.enqueue_in_context(event, 0) {
                Enqueue::Accepted => break,
                Enqueue::Full(pending) => {
                    blocked += 1;
                    event = pending;
                    match events.pop().expect("full queue must contain pixels") {
                        VideoEvent::Strip { index, .. } => seen.push(index),
                        _ => panic!("Done/control overtook strip pixels"),
                    }
                }
                Enqueue::Stale => panic!("current strip rejected as stale"),
            }
        }
        assert!(events.health().queued_frame_bytes <= FRAME_BUDGET);
    }
    events.push(VideoEvent::StripDone { id: 1, token: 1 });
    let mut done = false;
    while let Some(event) = events.pop() {
        match event {
            VideoEvent::Strip { index, .. } => {
                assert!(!done);
                seen.push(index);
            }
            VideoEvent::StripDone { .. } => {
                assert_eq!(seen.len(), 96);
                done = true;
            }
            _ => panic!("unexpected strip event"),
        }
    }
    assert!(blocked > 0 && done);
    assert_eq!(seen, (0..96).collect::<Vec<_>>());
    assert_eq!(events.health().dropped_frames, 0);
    assert_eq!(events.health().queued_frame_bytes, 0);
}
#[test]
fn stalled_consumer_receives_opening_then_latest_and_terminal_failure() {
    let events = Events::new();
    for time in 0..200 {
        events.push(VideoEvent::Play {
            id: 2,
            token: 3,
            time: time as f32,
            frame: frame(4),
        });
    }
    events.failed(2, 3);
    assert!(matches!(
        events.pop(),
        Some(VideoEvent::Play { time: 0.0, .. })
    ));
    assert!(matches!(
        events.pop(),
        Some(VideoEvent::Play { time: 199.0, .. })
    ));
    assert!(matches!(
        events.pop(),
        Some(VideoEvent::Failed { id: 2, token: 3 })
    ));
    assert_eq!(events.health().failures, 1);
    assert_eq!(events.health().dropped_frames, 198);
}
#[test]
fn context_clear_rejects_late_old_frames_and_terminal_failures_atomically() {
    let events = Events::new();
    events.push(VideoEvent::Play {
        id: 1,
        token: 1,
        time: 0.0,
        frame: frame(4),
    });
    events.clear(1);
    assert!(!events.push_in_context(
        VideoEvent::Play {
            id: 1,
            token: 1,
            time: 1.0,
            frame: frame(4)
        },
        0
    ));
    assert!(!events.failed_in_context(1, 1, 0));
    assert!(events.pop().is_none());
    assert_eq!(events.health().failures, 0);
    assert_eq!(events.health().queued_frame_bytes, 0);
    assert_eq!(events.health().status, VideoStatus::Idle);
    assert!(events.push_in_context(
        VideoEvent::Play {
            id: 2,
            token: 2,
            time: 0.0,
            frame: frame(4)
        },
        1
    ));
    assert!(events.failed_in_context(2, 2, 1));
    assert!(matches!(events.pop(), Some(VideoEvent::Play { id: 2, .. })));
    assert!(matches!(
        events.pop(),
        Some(VideoEvent::Failed { id: 2, token: 2 })
    ));
    assert_eq!(events.health().failures, 1);
}
#[test]
fn opening_and_latest_survive_full_size_frame_pressure_within_eight_mib() {
    let events = Events::new();
    let size = crate::video::PLAY_EDGE as usize * crate::video::PLAY_EDGE as usize * 4;
    for time in 0..4 {
        events.push(VideoEvent::Play {
            id: 1,
            token: 1,
            time: time as f32,
            frame: frame(size),
        });
        assert!(events.health().queued_frame_bytes <= FRAME_BUDGET);
    }
    events.push(VideoEvent::Ended { id: 1, token: 1 });
    assert!(matches!(
        events.pop(),
        Some(VideoEvent::Play { time: 0.0, .. })
    ));
    assert!(matches!(
        events.pop(),
        Some(VideoEvent::Play { time: 3.0, .. })
    ));
    assert!(matches!(events.pop(), Some(VideoEvent::Ended { .. })));
    assert_eq!(events.health().queued_frame_bytes, 0);
    // Once opening has been consumed, subsequent frames coalesce normally.
    for time in 4..7 {
        events.push(VideoEvent::Play {
            id: 1,
            token: 1,
            time: time as f32,
            frame: frame(4),
        });
    }
    assert!(matches!(
        events.pop(),
        Some(VideoEvent::Play { time: 6.0, .. })
    ));
    assert!(events.pop().is_none());
}

#[test]
fn bounded_control_reserve_delivers_failure_without_evicting_strip_pixels() {
    let events = Events::new();
    for index in 0..EVENT_LIMIT {
        assert!(matches!(
            events.enqueue_in_context(
                VideoEvent::Strip {
                    id: 1,
                    token: 1,
                    index: index as u16,
                    time: 0.0,
                    frame: frame(4)
                },
                0
            ),
            Enqueue::Accepted
        ));
    }
    for token in 0..100 {
        assert!(events.failed_in_context(2, token, 0));
    }
    assert_eq!(events.health().failures, 100);
    assert_eq!(events.health().dropped_frames, 0);
    assert_eq!(
        events.queue.lock().unwrap().items.len(),
        EVENT_LIMIT + CONTROL_RESERVE
    );
    for index in 0..EVENT_LIMIT {
        assert!(
            matches!(events.pop(), Some(VideoEvent::Strip { index: got, .. }) if got as usize == index)
        );
    }
    let mut last = None;
    while let Some(event) = events.pop() {
        last = Some(event);
    }
    assert!(matches!(
        last,
        Some(VideoEvent::Failed { id: 2, token: 99 })
    ));
}

#[test]
fn owned_full_strip_becomes_stale_on_context_cancel_without_failure_or_drop() {
    let events = Events::new();
    for index in 0..EVENT_LIMIT {
        assert!(matches!(
            events.enqueue_in_context(
                VideoEvent::Strip {
                    id: 1,
                    token: 1,
                    index: index as u16,
                    time: 0.0,
                    frame: frame(4)
                },
                0
            ),
            Enqueue::Accepted
        ));
    }
    let pending = match events.enqueue_in_context(
        VideoEvent::Strip {
            id: 1,
            token: 1,
            index: 0,
            time: 0.0,
            frame: frame(4),
        },
        0,
    ) {
        Enqueue::Full(event) => event,
        _ => panic!("queue must return ownership when Full"),
    };
    events.clear(1);
    assert!(matches!(
        events.enqueue_in_context(pending, 0),
        Enqueue::Stale
    ));
    assert_eq!(events.health().failures, 0);
    assert_eq!(events.health().dropped_frames, 0);
    assert_eq!(events.health().queued_frame_bytes, 0);
}
