//! Bounded actual-pixel failure receipts for the real adaptive corpus test.
use super::VideoFrame;

pub(super) fn assert_solid(frame: &VideoFrame, red: bool) {
    let mut min = [255u8; 4];
    let mut max = [0u8; 4];
    let mut bad = 0;
    let mut bad_rows = 0;
    let mut examples = Vec::new();
    for (y, row) in frame.rgba.chunks_exact(frame.w as usize * 4).enumerate() {
        let mut row_bad = false;
        for (x, pixel) in row.chunks_exact(4).enumerate() {
            for ((low, high), value) in min.iter_mut().zip(max.iter_mut()).zip(pixel.iter()) {
                *low = (*low).min(*value);
                *high = (*high).max(*value);
            }
            let valid = if red {
                pixel[0] > 200 && pixel[1] < 20 && pixel[2] < 20
            } else {
                pixel[1] > 100 && pixel[0] < 20 && pixel[2] < 20
            };
            if !valid {
                bad += 1;
                row_bad = true;
                if examples.len() < 8 {
                    examples.push((x, y, [pixel[0], pixel[1], pixel[2], pixel[3]]));
                }
            }
        }
        bad_rows += usize::from(row_bad);
    }
    assert_eq!(bad, 0, "adaptive{}x{} expected{}: RGBAmin={min:?} max={max:?}, bad_rows={bad_rows}, examples={examples:?}", frame.w, frame.h, if red {"red"} else {"green"});
}

/// Verified WPT MP4 SHA256:
///7fa7d6c9754d3314811911e553b8dc75e0d4d50cc3a4731497e493be44554636.
/// Independent PyAV16.0.1/libavcodec62.11.100 confirms one encoded near-black
/// frame at15728/16000s; receipt:target/video-studio/adaptive-pyav-reference.json.
pub(super) fn assert_reference_transition(frame: &VideoFrame) {
    assert_eq!((frame.w, frame.h), (200, 150));
    let mut histogram = [0usize; 3];
    for pixel in frame.rgba.chunks_exact(4) {
        assert!(
            pixel[0] <= 2 && pixel[0] == pixel[1] && pixel[1] == pixel[2] && pixel[3] == 255,
            "encodedtransitionframe hasunexpectedpixel{pixel:?}"
        );
        histogram[pixel[0] as usize] += 1;
    }
    assert_eq!(
        histogram,
        [25368, 1708, 2924],
        "encodedtransitionpixelhistogram differsfromindependentreference"
    );
}
