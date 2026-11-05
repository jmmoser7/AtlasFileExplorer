use super::*;
use std::io::Cursor;

#[test]
fn protocol_roundtrips_jobs_and_frames_without_changing_pixels() {
    let job = Job::Play {
        id: 8,
        token: 12,
        path: "unicode-\u{03bb}.mp4".into(),
        start: 1.0,
        end: Some(4.0),
        from: f32::NAN,
        looped: true,
    };
    let mut packet = Vec::new();
    write_job(&mut packet, &job).unwrap();
    let decoded = read_job(&mut Cursor::new(packet)).unwrap();
    match decoded {
        Job::Play {
            id,
            token,
            path,
            start,
            end,
            from,
            looped,
        } => {
            assert_eq!(
                (id, token, start, end, looped),
                (8, 12, 1.0, Some(4.0), true)
            );
            assert!(from.is_nan());
            assert_eq!(path, PathBuf::from("unicode-\u{03bb}.mp4"));
        }
        _ => panic!("wrong job kind"),
    }
    let frame = VideoFrame {
        w: 3,
        h: 1,
        rgba: (0..12).collect(),
    };
    let event = VideoEvent::Strip {
        id: 8,
        token: 12,
        time: 1.25,
        index: 2,
        frame: frame.clone(),
    };
    let mut packet = Vec::new();
    write_event(&mut packet, &event).unwrap();
    match read_event(&mut Cursor::new(packet)).unwrap() {
        VideoEvent::Strip {
            id: 8,
            token: 12,
            time: 1.25,
            index: 2,
            frame: decoded,
        } => assert_eq!(decoded, frame),
        _ => panic!("wrong decoded event"),
    }
}

#[test]
fn oversized_truncated_invalid_dimensions_and_nonfinite_time_fail_closed() {
    let mut oversized = MAGIC.to_vec();
    oversized.extend_from_slice(&((MAX_PACKET + 1) as u32).to_le_bytes());
    assert!(read_event(&mut Cursor::new(oversized)).is_err());
    let event = VideoEvent::Play {
        id: 1,
        token: 1,
        time: 0.0,
        frame: VideoFrame {
            w: 1,
            h: 1,
            rgba: vec![1, 2, 3, 255],
        },
    };
    let mut packet = Vec::new();
    write_event(&mut packet, &event).unwrap();
    for len in 0..packet.len() {
        assert!(
            read_event(&mut Cursor::new(&packet[..len])).is_err(),
            "accepted prefix {len}"
        );
    }
    let mut bad = packet.clone();
    bad[8 + 17..8 + 21].copy_from_slice(&f32::NAN.to_bits().to_le_bytes());
    assert!(read_event(&mut Cursor::new(bad)).is_err());
    let mut bad = packet;
    bad[8 + 21..8 + 25].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(read_event(&mut Cursor::new(bad)).is_err());
    assert!(validate_frame(1, 1, 3, PLAY_EDGE).is_err());
    assert!(validate_frame(PLAY_EDGE + 1, 1, 4, PLAY_EDGE).is_err());
}
