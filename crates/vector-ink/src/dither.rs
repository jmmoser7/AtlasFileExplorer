//! Ordered dither for 8-bit output. A slow gradient (a wide soft tip, a heavy
//! blur) quantized by rounding holds each level for many pixels and reads as
//! nested rings. Adding a per-pixel threshold before `floor` spreads each step
//! into a fine pattern while keeping the average exact.

const BAYER8: [u8; 64] = [
    0, 32, 8, 40, 2, 34, 10, 42, //
    48, 16, 56, 24, 50, 18, 58, 26, //
    12, 44, 4, 36, 14, 46, 6, 38, //
    60, 28, 52, 20, 62, 30, 54, 22, //
    3, 35, 11, 43, 1, 33, 9, 41, //
    51, 19, 59, 27, 49, 17, 57, 25, //
    15, 47, 7, 39, 13, 45, 5, 37, //
    63, 31, 55, 23, 61, 29, 53, 21, //
];

/// Threshold in `(0, 1)` for pixel `(x, y)`. Callers pass coordinates on a
/// shared grid so neighbouring images continue one pattern.
#[inline]
pub(crate) fn threshold(x: i64, y: i64) -> f32 {
    let i = ((y & 7) * 8 + (x & 7)) as usize;
    (BAYER8[i] as f32 + 0.5) / 64.0
}

/// `value` in `0..=255` to a byte. Each pixel lands on one of the two levels
/// around `value`, and a neighbourhood averages to `value`.
#[inline]
pub(crate) fn quantize(value: f32, x: i64, y: i64) -> u8 {
    (value + threshold(x, y)).floor().clamp(0.0, 255.0) as u8
}
