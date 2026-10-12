//! Renderer-free CSS image filter math shared by board and raster exports.

use crate::scene::{ImageAdjust, Rgba};

/// 3×3 row-major color matrix (W3C Filter Effects).
#[derive(Clone, Copy)]
struct Mat3([f32; 9]);

impl Mat3 {
    fn saturate(s: f32) -> Self {
        Self([
            0.213 + 0.787 * s,
            0.715 - 0.715 * s,
            0.072 - 0.072 * s,
            0.213 - 0.213 * s,
            0.715 + 0.285 * s,
            0.072 - 0.072 * s,
            0.213 - 0.213 * s,
            0.715 - 0.715 * s,
            0.072 + 0.928 * s,
        ])
    }

    fn grayscale(g: f32) -> Self {
        let t = 1.0 - g;
        Self([
            0.2126 + 0.7874 * t,
            0.7152 - 0.7152 * t,
            0.0722 - 0.0722 * t,
            0.2126 - 0.2126 * t,
            0.7152 + 0.2848 * t,
            0.0722 - 0.0722 * t,
            0.2126 - 0.2126 * t,
            0.7152 - 0.7152 * t,
            0.0722 + 0.9278 * t,
        ])
    }

    fn sepia(p: f32) -> Self {
        let t = 1.0 - p;
        Self([
            0.393 + 0.607 * t,
            0.769 - 0.769 * t,
            0.189 - 0.189 * t,
            0.349 - 0.349 * t,
            0.686 + 0.314 * t,
            0.168 - 0.168 * t,
            0.272 - 0.272 * t,
            0.534 - 0.534 * t,
            0.131 + 0.869 * t,
        ])
    }

    fn hue_rotate(deg: f32) -> Self {
        let rad = deg.to_radians();
        let (c, s) = (rad.cos(), rad.sin());
        Self([
            0.213 + c * 0.787 - s * 0.213,
            0.715 - c * 0.715 - s * 0.715,
            0.072 - c * 0.072 + s * 0.928,
            0.213 - c * 0.213 + s * 0.143,
            0.715 + c * 0.285 + s * 0.140,
            0.072 - c * 0.072 - s * 0.283,
            0.213 - c * 0.213 - s * 0.787,
            0.715 - c * 0.715 + s * 0.715,
            0.072 + c * 0.928 + s * 0.072,
        ])
    }

    fn mul(self, rhs: Self) -> Self {
        let a = &self.0;
        let b = &rhs.0;
        Self([
            a[0] * b[0] + a[1] * b[3] + a[2] * b[6],
            a[0] * b[1] + a[1] * b[4] + a[2] * b[7],
            a[0] * b[2] + a[1] * b[5] + a[2] * b[8],
            a[3] * b[0] + a[4] * b[3] + a[5] * b[6],
            a[3] * b[1] + a[4] * b[4] + a[5] * b[7],
            a[3] * b[2] + a[4] * b[5] + a[5] * b[8],
            a[6] * b[0] + a[7] * b[3] + a[8] * b[6],
            a[6] * b[1] + a[7] * b[4] + a[8] * b[7],
            a[6] * b[2] + a[7] * b[5] + a[8] * b[8],
        ])
    }

    fn transform(self, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let m = &self.0;
        (
            m[0] * r + m[1] * g + m[2] * b,
            m[3] * r + m[4] * g + m[5] * b,
            m[6] * r + m[7] * g + m[8] * b,
        )
    }
}

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn lerp_channel(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn to_u8(v: f32) -> u8 {
    (clamp01(v) * 255.0).round() as u8
}

fn build_color_matrix(adjust: &ImageAdjust) -> Mat3 {
    Mat3::hue_rotate(adjust.hue_deg)
        .mul(Mat3::sepia(adjust.sepia))
        .mul(Mat3::grayscale(adjust.grayscale))
        .mul(Mat3::saturate(adjust.saturate))
}

/// W3C color-matrix coefficients (row-major 3×3) shared by CPU and GPU filters.
pub fn color_matrix_coefficients(adjust: &ImageAdjust) -> [f32; 9] {
    build_color_matrix(adjust).0
}

/// Uniforms for the model viewport GPU filter pass (`brightness * contrast`, offset).
pub fn filter_tone(adjust: &ImageAdjust) -> (f32, f32) {
    (
        adjust.brightness * adjust.contrast,
        0.5 * (1.0 - adjust.contrast),
    )
}

/// Overlay + invert terms for the GPU filter pass.
pub fn filter_shader_overlay(adjust: &ImageAdjust) -> ([f32; 4], f32) {
    let overlay = adjust.overlay.map(|Rgba([r, g, b, a])| {
        [
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        ]
    });
    (overlay.unwrap_or([0.0; 4]), adjust.invert.clamp(0.0, 1.0))
}

/// SVG color matrix for the same alpha-preserving tint used by CPU pixels.
pub fn overlay_color_matrix(adjust: &ImageAdjust) -> Option<[f32; 20]> {
    let (overlay, _) = filter_shader_overlay(adjust);
    let [r, g, b, a] = overlay;
    (a > 0.0).then_some([
        1.0 - a,
        0.0,
        0.0,
        0.0,
        r * a,
        0.0,
        1.0 - a,
        0.0,
        0.0,
        g * a,
        0.0,
        0.0,
        1.0 - a,
        0.0,
        b * a,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ])
}

/// Prepared CSS pixel transform. Input and output use straight-alpha sRGBA8.
/// Construct once per image; the same coefficients serve the GPU adapter.
pub struct PixelFilter {
    matrix: Mat3,
    scale: f32,
    offset: f32,
    overlay: [f32; 4],
    invert: f32,
}

impl PixelFilter {
    pub fn new(adjust: &ImageAdjust) -> Self {
        let (scale, offset) = filter_tone(adjust);
        let (overlay, invert) = filter_shader_overlay(adjust);
        Self {
            matrix: build_color_matrix(adjust),
            scale,
            offset,
            overlay,
            invert,
        }
    }

    pub fn apply(&self, pixel: [u8; 4]) -> [u8; 4] {
        let r = clamp01(self.scale * (pixel[0] as f32 / 255.0) + self.offset);
        let g = clamp01(self.scale * (pixel[1] as f32 / 255.0) + self.offset);
        let b = clamp01(self.scale * (pixel[2] as f32 / 255.0) + self.offset);
        let (r, g, b) = self.matrix.transform(r, g, b);
        let (mut r, mut g, mut b) = (clamp01(r), clamp01(g), clamp01(b));
        if self.invert != 0.0 {
            r = clamp01(lerp_channel(r, 1.0 - r, self.invert));
            g = clamp01(lerp_channel(g, 1.0 - g, self.invert));
            b = clamp01(lerp_channel(b, 1.0 - b, self.invert));
        }
        let overlay = self.overlay;
        if overlay[3] > 0.0 {
            let inv_oa = 1.0 - overlay[3];
            r = clamp01(r * inv_oa + overlay[0] * overlay[3]);
            g = clamp01(g * inv_oa + overlay[1] * overlay[3]);
            b = clamp01(b * inv_oa + overlay[2] * overlay[3]);
        }
        [to_u8(r), to_u8(g), to_u8(b), pixel[3]]
    }
}

/// Apply the board's CSS filter set without changing pixel alpha.
pub fn adjust_rgba8(pixels: &mut [u8], adjust: &ImageAdjust) {
    if adjust.is_identity() {
        return;
    }
    let filter = PixelFilter::new(adjust);
    for pixel in pixels.chunks_exact_mut(4) {
        let out = filter.apply([pixel[0], pixel[1], pixel[2], pixel[3]]);
        pixel.copy_from_slice(&out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_pixels_filter_straight_rgb_and_preserve_alpha() {
        let mut pixels = [40, 80, 120, 128, 40, 80, 120, 0];
        adjust_rgba8(
            &mut pixels,
            &ImageAdjust {
                brightness: 2.0,
                ..Default::default()
            },
        );
        assert_eq!(pixels, [80, 160, 240, 128, 80, 160, 240, 0]);
    }

    #[test]
    fn overlay_applies_once_after_brightness() {
        let filter = PixelFilter::new(&ImageAdjust {
            brightness: 2.0,
            overlay: Some(Rgba([0, 0, 0, 128])),
            ..Default::default()
        });
        assert_eq!(filter.apply([200, 200, 200, 77]), [127, 127, 127, 77]);
    }
}
