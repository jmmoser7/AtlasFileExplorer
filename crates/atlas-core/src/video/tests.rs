use super::*;

#[test]
fn pan_covers_the_trim_window() {
    let (start, end) = playback_window(3.0, Some(11.0), 20.0);
    assert_eq!(time_at(0.0, start, end), 3.0);
    assert_eq!(time_at(1.0, start, end), 11.0);
    assert_eq!(time_at(0.5, start, end), 7.0);
    assert_eq!(fraction_at(7.0, start, end), 0.5);
    let (start, end) = playback_window(0.0, None, 8.0);
    assert_eq!((start, end), (0.0, 8.0));
    let (start, end) = playback_window(30.0, Some(10.0), 20.0);
    assert_eq!(start, end);
}

#[test]
fn scrub_u_follows_the_node_width_and_rotation() {
    assert_eq!(scrub_u(0.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0), Some(0.0));
    assert_eq!(scrub_u(100.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0), Some(1.0));
    assert_eq!(scrub_u(50.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0), Some(0.5));
    assert!(scrub_u(-1.0, 20.0, 0.0, 0.0, 100.0, 40.0, 0.0).is_none());
    // 180° swaps the left and right edges.
    assert_eq!(scrub_u(0.0, 20.0, 0.0, 0.0, 100.0, 40.0, 180.0), Some(1.0));
    assert_eq!(
        scrub_u(100.0, 20.0, 0.0, 0.0, 100.0, 40.0, 180.0),
        Some(0.0)
    );
}

#[test]
fn timecode_and_sample_grid() {
    assert_eq!(format_timecode(0.0), "0:00.0");
    assert_eq!(format_timecode(65.24), "1:05.2");
    assert_eq!(format_timecode(3600.0), "1:00:00.0");
    let times = sample_times(0.0, 10.0, 96);
    assert_eq!(times.len(), 96);
    assert_eq!(times[0], 0.0);
    assert!(times[95] < 10.0);
    assert!(times[95] > 9.0);
}

#[test]
fn bgra_scale_swaps_channels_and_honors_bottom_up() {
    let px = |b, g, r| [b, g, r, 0];
    let mut top_down = Vec::new();
    top_down.extend_from_slice(&px(10, 20, 30));
    top_down.extend_from_slice(&px(10, 20, 30));
    top_down.extend_from_slice(&px(10, 20, 30));
    top_down.extend_from_slice(&px(10, 20, 30));
    let frame = bgra_scaled(&top_down, 2, 2, 8, 2).unwrap();
    assert_eq!(&frame.rgba[0..4], &[30, 20, 10, 255]);

    let mut bottom_up = Vec::new();
    bottom_up.extend_from_slice(&px(1, 2, 3));
    bottom_up.extend_from_slice(&px(1, 2, 3));
    bottom_up.extend_from_slice(&px(4, 5, 6));
    bottom_up.extend_from_slice(&px(4, 5, 6));
    let frame = bgra_scaled(&bottom_up, 2, 2, -8, 2).unwrap();
    assert_eq!(&frame.rgba[0..4], &[6, 5, 4, 255]);
}

#[test]
fn replay_normalizes_end_and_nonfinite_resume_positions() {
    assert_eq!(playback_origin(10.0, 3.0, 10.0), 3.0);
    assert_eq!(playback_origin(99.0, 3.0, 10.0), 3.0);
    assert_eq!(playback_origin(f32::NAN, 3.0, 10.0), 3.0);
    assert_eq!(playback_origin(6.0, 3.0, 10.0), 6.0);
    assert_eq!(playback_origin(-8.0, 3.0, 10.0), 3.0);
}

#[test]
fn padded_odd_and_negative_rows_preserve_every_channel_without_reading_padding() {
    for w in [1u32, 3, 7] {
        for h in [1u32, 3, 5] {
            for padding in [0usize, 4, 20] {
                let pitch = w as usize * 4 + padding;
                let mut data = vec![199u8; pitch * h as usize];
                let mut expected = Vec::new();
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let pixel = [x as u8 + 1, y as u8 + 7, (x + y) as u8 + 21, 0];
                        data[y * pitch + x * 4..y * pitch + x * 4 + 4].copy_from_slice(&pixel);
                        expected.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
                    }
                }
                let frame = bgra_scaled(&data, w, h, pitch as i32, 100).unwrap();
                assert_eq!(frame.rgba, expected);
                let reversed: Vec<u8> = data.chunks_exact(pitch).rev().flatten().copied().collect();
                assert_eq!(
                    bgra_scaled(&reversed, w, h, -(pitch as i32), 100)
                        .unwrap()
                        .rgba,
                    expected
                );
                let needed = pitch * (h as usize - 1) + w as usize * 4;
                assert!(bgra_scaled(&data[..needed - 1], w, h, pitch as i32, 100).is_none());
                assert_eq!(
                    bgra_scaled(&data[..needed], w, h, pitch as i32, 100)
                        .unwrap()
                        .rgba,
                    expected
                );
            }
        }
    }
}

#[test]
fn invalid_stride_and_locked_scanline_bounds_fail_closed() {
    use super::frame::{bgra_scaled_rows, validate_rows};
    assert!(bgra_scaled(&[0; 12], 3, 1, 8, 3).is_none());
    assert!(bgra_scaled(&[0; 12], 3, 1, 0, 3).is_none());
    assert!(bgra_scaled(&[0; 12], u32::MAX, u32::MAX, i32::MIN, 3).is_none());
    assert!(validate_rows(24, 0, 2, 3, -8, true).is_none());
    assert!(validate_rows(24, 16, 2, 3, -8, true).is_some());
    assert!(validate_rows(24, 17, 2, 3, -8, true).is_none());
    let pixels = [1, 2, 3, 0, 9, 10, 11, 0];
    assert_eq!(
        bgra_scaled_rows(&pixels, 1, 2, -4, 4, 2).unwrap().rgba,
        [11, 10, 9, 255, 3, 2, 1, 255]
    );
}

#[test]
fn declared_stride_allows_unused_coded_rows_without_guessing_pitch() {
    use super::frame::linear_stride;
    let (w, h, pitch, coded_rows) = (400u32, 300u32, 1600i32, 304usize);
    let mut source = vec![0u8; pitch as usize * coded_rows];
    for pixel in source[..pitch as usize * h as usize].as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&[7, 31, 93, 0]);
    }
    source[pitch as usize * h as usize..].fill(199);
    assert_eq!(source.len(), 486400);
    assert_eq!(linear_stride(source.len(), w, h, Some(pitch)), Some(pitch));
    assert_eq!(
        linear_stride(source.len(), w, h, None),
        None,
        "extra allocation cannot invent a pitch"
    );
    assert!(linear_stride(pitch as usize * h as usize - 1, w, h, Some(pitch)).is_none());
    let scaled = bgra_scaled(&source, w, h, pitch, 320).unwrap();
    assert_eq!((scaled.w, scaled.h), (320, 240));
    assert!(scaled
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|pixel| *pixel == [93, 31, 7, 255]));
}

#[test]
fn display_aperture_ignores_coded_padding_and_honors_negative_pitch_and_offsets() {
    use super::frame::{scaled_region, FrameArea};
    let coded = (8u32, 6u32);
    let pitch = coded.0 as usize * 4;
    let area = FrameArea {
        x: 2,
        y: 1,
        w: 3,
        h: 3,
    };
    let mut data = vec![199u8; pitch * coded.1 as usize];
    for y in area.y..area.y + area.h {
        for x in area.x..area.x + area.w {
            let at = y as usize * pitch + x as usize * 4;
            data[at..at + 4].copy_from_slice(&[7, 31, 93, 0]);
        }
    }
    let frame = scaled_region(&data, coded, pitch as i32, 0, area, 100).unwrap();
    assert_eq!((frame.w, frame.h), (3, 3));
    assert!(frame
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|pixel| *pixel == [93, 31, 7, 255]));
    let reversed: Vec<u8> = data.chunks_exact(pitch).rev().flatten().copied().collect();
    let origin = (coded.1 as usize - 1) * pitch;
    assert_eq!(
        scaled_region(&reversed, coded, -(pitch as i32), origin, area, 100).unwrap(),
        frame
    );
    assert!(scaled_region(
        &data,
        coded,
        pitch as i32,
        0,
        FrameArea { x: 7, ..area },
        100
    )
    .is_none());
    assert!(scaled_region(
        &data,
        coded,
        pitch as i32,
        0,
        FrameArea { w: 0, ..area },
        100
    )
    .is_none());
    assert!(scaled_region(&data[..data.len() - 1], coded, pitch as i32, 0, area, 100).is_none());
}
