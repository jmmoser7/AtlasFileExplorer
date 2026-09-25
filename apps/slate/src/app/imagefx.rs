//! CPU implementation of the CSS filter set for board image previews.
//!
//! `slate_doc::scene::ImageAdjust` is constrained to what CSS `filter` can
//! express; this module applies the same math to RGBA8 pixels so the egui
//! board preview matches the exported HTML artifact.

use eframe::egui::{Color32, ColorImage};
use slate_doc::scene::{Crop, ImageAdjust, Rgba};
use std::path::{Path, PathBuf};

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

/// OpenGL column-major upload of the W3C row-major color matrix.
pub fn gl_color_mat3(adjust: &ImageAdjust) -> [f32; 9] {
    let m = color_matrix_coefficients(adjust);
    [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]
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

/// Returns an adjusted copy of `src`. Identity adjustments return a plain clone.
pub fn adjusted(src: &ColorImage, adjust: &ImageAdjust) -> ColorImage {
    if adjust.is_identity() {
        return src.clone();
    }

    let matrix = build_color_matrix(adjust);
    let (scale, offset) = filter_tone(adjust);
    let (overlay, invert) = filter_shader_overlay(adjust);
    let inv_oa = 1.0 - overlay[3];

    let mut out = src.clone();
    for pix in &mut out.pixels {
        let alpha = pix.a();
        let r = clamp01(scale * (pix.r() as f32 / 255.0) + offset);
        let g = clamp01(scale * (pix.g() as f32 / 255.0) + offset);
        let b = clamp01(scale * (pix.b() as f32 / 255.0) + offset);

        let (r, g, b) = matrix.transform(r, g, b);
        let (mut r, mut g, mut b) = (clamp01(r), clamp01(g), clamp01(b));

        // CSS filters apply in list order and `css_filter()` appends
        // invert(1) last, after the hue/sat/brightness pipeline.
        if invert != 0.0 {
            r = clamp01(lerp_channel(r, 1.0 - r, invert));
            g = clamp01(lerp_channel(g, 1.0 - g, invert));
            b = clamp01(lerp_channel(b, 1.0 - b, invert));
        }

        if overlay[3] > 0.0 {
            r = clamp01(r * inv_oa + overlay[0] * overlay[3]);
            g = clamp01(g * inv_oa + overlay[1] * overlay[3]);
            b = clamp01(b * inv_oa + overlay[2] * overlay[3]);
        }

        *pix = Color32::from_rgba_unmultiplied(to_u8(r), to_u8(g), to_u8(b), alpha);
    }

    out
}

/// Longest edge of a filter-radio thumbnail. Small on purpose: the swatch is
/// a preview of the photo, not a second full-resolution decode.
pub const SWATCH_EDGE: usize = 32;

/// Center-cropped square. Nearest sampling keeps the radio a low-resolution
/// view of the source.
/// PNG of the visible crop window. The hidden part of `path` is not in the file.
/// A full crop, a cloud placeholder, or a file that will not decode returns nothing.
pub(crate) fn visible_crop_file(path: &Path, crop: Crop) -> Option<PathBuf> {
    if crop.is_full() || atlas_core::cloud::is_dehydrated(path) {
        return None;
    }
    let img = image::open(path).ok()?;
    let c = crop.clamped();
    let w = img.width().max(1);
    let h = img.height().max(1);
    let x = ((c.x * w as f32).round() as u32).min(w - 1);
    let y = ((c.y * h as f32).round() as u32).min(h - 1);
    let cw = ((c.w * w as f32).round() as u32).clamp(1, w - x);
    let ch = ((c.h * h as f32).round() as u32).clamp(1, h - y);
    let cropped = img.crop_imm(x, y, cw, ch);
    let dir = std::env::temp_dir().join("slate-crop");
    std::fs::create_dir_all(&dir).ok()?;
    let key = format!(
        "{:016x}-{:.4}-{:.4}-{:.4}-{:.4}.png",
        path_key(path),
        c.x,
        c.y,
        c.w,
        c.h
    );
    let out = dir.join(key);
    cropped.save(&out).ok()?;
    Some(out)
}

fn path_key(path: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}

pub fn square_swatch(src: &ColorImage, edge: usize) -> ColorImage {
    let edge = edge.max(1);
    let (w, h) = (src.size[0], src.size[1]);
    let mut out = ColorImage::new([edge, edge], Color32::TRANSPARENT);
    if w == 0 || h == 0 {
        return out;
    }
    let side = w.min(h);
    let x0 = (w - side) / 2;
    let y0 = (h - side) / 2;
    for y in 0..edge {
        let sy = y0 + y * side / edge;
        for x in 0..edge {
            let sx = x0 + x * side / edge;
            out.pixels[y * edge + x] = src.pixels[sy * w + sx];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::ColorImage;
    use image::GenericImageView;

    fn solid(color: [u8; 4]) -> ColorImage {
        let px = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
        ColorImage {
            size: [1, 1],
            pixels: vec![px],
        }
    }

    fn pixel(img: &ColorImage) -> [u8; 4] {
        let p = img.pixels[0];
        [p.r(), p.g(), p.b(), p.a()]
    }

    fn approx_eq(a: u8, b: u8, tol: u8) {
        let d = a.abs_diff(b);
        assert!(d <= tol, "expected {a} ≈ {b} (±{tol}), diff {d}");
    }

    #[test]
    fn identity_preserves_pixels() {
        let src = solid([40, 80, 120, 200]);
        let out = adjusted(&src, &ImageAdjust::default());
        assert_eq!(pixel(&src), pixel(&out));
    }

    #[test]
    fn brightness_doubles_gray() {
        let src = solid([100, 100, 100, 255]);
        let adjust = ImageAdjust {
            brightness: 2.0,
            ..ImageAdjust::default()
        };
        let out = adjusted(&src, &adjust);
        let [r, g, b, a] = pixel(&out);
        approx_eq(r, 200, 1);
        approx_eq(g, 200, 1);
        approx_eq(b, 200, 1);
        assert_eq!(a, 255);
    }

    #[test]
    fn brightness_clamps_before_overlay() {
        let src = solid([200, 200, 200, 255]);
        let adjust = ImageAdjust {
            brightness: 1.5,
            overlay: Some(Rgba([0, 0, 0, 128])),
            ..ImageAdjust::default()
        };
        let [r, g, b, _] = pixel(&adjusted(&src, &adjust));
        // CSS brightness clips 300 to 255 before the 50% black overlay.
        approx_eq(r, 127, 1);
        approx_eq(g, 127, 1);
        approx_eq(b, 127, 1);
    }

    #[test]
    fn gl_matrix_upload_matches_cpu_transform() {
        let color = [0.17, 0.43, 0.81];
        let cases = [
            ImageAdjust {
                grayscale: 1.0,
                ..ImageAdjust::default()
            },
            ImageAdjust {
                sepia: 1.0,
                ..ImageAdjust::default()
            },
            ImageAdjust {
                hue_deg: 73.0,
                ..ImageAdjust::default()
            },
            ImageAdjust {
                saturate: 1.8,
                ..ImageAdjust::default()
            },
        ];
        for adjust in cases {
            let cpu = build_color_matrix(&adjust).transform(color[0], color[1], color[2]);
            let gl = gl_color_mat3(&adjust);
            let gpu = (
                gl[0] * color[0] + gl[3] * color[1] + gl[6] * color[2],
                gl[1] * color[0] + gl[4] * color[1] + gl[7] * color[2],
                gl[2] * color[0] + gl[5] * color[1] + gl[8] * color[2],
            );
            assert!((cpu.0 - gpu.0).abs() < 1e-6);
            assert!((cpu.1 - gpu.1).abs() < 1e-6);
            assert!((cpu.2 - gpu.2).abs() < 1e-6);
        }
    }

    #[test]
    fn grayscale_makes_channels_equal() {
        let colors = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [64, 128, 192, 255],
        ];
        let adjust = ImageAdjust {
            grayscale: 1.0,
            ..ImageAdjust::default()
        };
        for color in colors {
            let src = solid(color);
            let [r, g, b, _] = pixel(&adjusted(&src, &adjust));
            approx_eq(r, g, 1);
            approx_eq(g, b, 1);
        }
    }

    #[test]
    fn contrast_preserves_mid_gray() {
        for k in [0.5_f32, 1.0, 2.0, 3.5] {
            let src = solid([128, 128, 128, 255]);
            let adjust = ImageAdjust {
                contrast: k,
                ..ImageAdjust::default()
            };
            let [r, g, b, _] = pixel(&adjusted(&src, &adjust));
            approx_eq(r, 128, 1);
            approx_eq(g, 128, 1);
            approx_eq(b, 128, 1);
        }
    }

    #[test]
    fn hue_rotate_360_is_near_identity() {
        let src = solid([200, 50, 100, 255]);
        let adjust = ImageAdjust {
            hue_deg: 360.0,
            ..ImageAdjust::default()
        };
        let [sr, sg, sb, _] = pixel(&src);
        let [r, g, b, _] = pixel(&adjusted(&src, &adjust));
        approx_eq(r, sr, 2);
        approx_eq(g, sg, 2);
        approx_eq(b, sb, 2);
    }

    #[test]
    fn overlay_full_and_half_alpha() {
        let src = solid([0, 255, 0, 255]);

        let full = ImageAdjust {
            overlay: Some(Rgba([255, 0, 0, 255])),
            ..ImageAdjust::default()
        };
        let [r, g, b, a] = pixel(&adjusted(&src, &full));
        approx_eq(r, 255, 2);
        approx_eq(g, 0, 2);
        approx_eq(b, 0, 2);
        assert_eq!(a, 255);

        let half = ImageAdjust {
            overlay: Some(Rgba([255, 0, 0, 128])),
            ..ImageAdjust::default()
        };
        let [r, g, b, a] = pixel(&adjusted(&src, &half));
        approx_eq(r, 128, 2);
        approx_eq(g, 128, 2);
        approx_eq(b, 0, 2);
        assert_eq!(a, 255);
    }

    #[test]
    fn alpha_preserved_through_filters() {
        let src = solid([100, 150, 200, 77]);
        let adjust = ImageAdjust {
            brightness: 1.5,
            contrast: 1.2,
            saturate: 0.8,
            grayscale: 0.3,
            sepia: 0.4,
            hue_deg: 45.0,
            invert: 1.0,
            overlay: Some(Rgba([10, 20, 30, 64])),
        };
        let [_, _, _, a] = pixel(&adjusted(&src, &adjust));
        assert_eq!(a, 77);
    }

    #[test]
    fn invert_amount_blends_towards_full_invert() {
        let src = solid([40, 100, 220, 255]);
        let half = ImageAdjust {
            invert: 0.5,
            ..ImageAdjust::default()
        };
        let [r, g, b, _] = pixel(&adjusted(&src, &half));
        approx_eq(r, 128, 1);
        approx_eq(g, 128, 1);
        approx_eq(b, 128, 1);
    }

    #[test]
    fn invert_flips_channels() {
        let src = solid([40, 100, 220, 255]);
        let adjust = ImageAdjust {
            invert: 1.0,
            ..ImageAdjust::default()
        };
        let [r, g, b, a] = pixel(&adjusted(&src, &adjust));
        approx_eq(r, 215, 1);
        approx_eq(g, 155, 1);
        approx_eq(b, 35, 1);
        assert_eq!(a, 255);
    }

    #[test]
    fn invert_applies_after_brightness() {
        // brightness(2) then invert: 100/255 → 200/255 → 55/255. The
        // reverse order would give 255 − 100 = 155 → 255 (clipped), so this
        // pins the CSS list order (invert appended last).
        let src = solid([100, 100, 100, 255]);
        let adjust = ImageAdjust {
            brightness: 2.0,
            invert: 1.0,
            ..ImageAdjust::default()
        };
        let [r, g, b, _] = pixel(&adjusted(&src, &adjust));
        approx_eq(r, 55, 1);
        approx_eq(g, 55, 1);
        approx_eq(b, 55, 1);
    }

    #[test]
    fn sepia_on_white_matches_matrix() {
        let src = solid([255, 255, 255, 255]);
        let adjust = ImageAdjust {
            sepia: 1.0,
            ..ImageAdjust::default()
        };
        let m = Mat3::sepia(1.0);
        let (er, eg, eb) = m.transform(1.0, 1.0, 1.0);
        let [r, g, b, _] = pixel(&adjusted(&src, &adjust));
        approx_eq(r, to_u8(er), 2);
        approx_eq(g, to_u8(eg), 2);
        approx_eq(b, to_u8(eb), 2);
    }

    #[test]
    fn square_swatch_is_a_center_crop_and_takes_a_filter() {
        let mut src = ColorImage::new([8, 4], Color32::from_rgb(20, 40, 200));
        src.pixels[0] = Color32::from_rgb(255, 0, 0);
        let swatch = square_swatch(&src, SWATCH_EDGE);
        assert_eq!(swatch.size, [SWATCH_EDGE, SWATCH_EDGE]);
        assert_eq!(swatch.pixels[0], Color32::from_rgb(20, 40, 200));
        let mono = adjusted(&swatch, &slate_doc::scene::PhotoFilter::Mono.at(1.0));
        let p = mono.pixels[0];
        assert!((p.r() as i16 - p.g() as i16).abs() <= 2);
        assert!((p.g() as i16 - p.b() as i16).abs() <= 2);
    }

    #[test]
    fn visible_crop_file_keeps_only_the_window() {
        let dir = std::env::temp_dir().join(format!("slate-crop-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("full.png");
        let mut img = image::RgbaImage::new(4, 2);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = image::Rgba([x as u8, y as u8, 0, 255]);
        }
        img.save(&src).unwrap();
        let crop = Crop {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        let out = visible_crop_file(&src, crop).unwrap();
        let cropped = image::open(&out).unwrap();
        assert_eq!(cropped.width(), 2);
        assert_eq!(cropped.height(), 2);
        let px = cropped.get_pixel(0, 0);
        assert_eq!(px.0[0], 2);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&out);
    }
}
