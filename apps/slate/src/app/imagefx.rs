//! CPU implementation of the CSS filter set for board image previews.
//!
//! `slate_doc::scene::ImageAdjust` is constrained to what CSS `filter` can
//! express; this module applies the same math to RGBA8 pixels so the egui
//! board preview matches the exported HTML artifact.

use eframe::egui::{Color32, ColorImage};
use slate_doc::scene::{ImageAdjust, Rgba};

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

fn to_u8(v: f32) -> u8 {
    (clamp01(v) * 255.0).round() as u8
}

fn build_color_matrix(adjust: &ImageAdjust) -> Mat3 {
    Mat3::hue_rotate(adjust.hue_deg)
        .mul(Mat3::sepia(adjust.sepia))
        .mul(Mat3::grayscale(adjust.grayscale))
        .mul(Mat3::saturate(adjust.saturate))
}

/// Returns an adjusted copy of `src`. Identity adjustments return a plain clone.
pub fn adjusted(src: &ColorImage, adjust: &ImageAdjust) -> ColorImage {
    if adjust.is_identity() {
        return src.clone();
    }

    let matrix = build_color_matrix(adjust);
    let scale = adjust.brightness * adjust.contrast;
    let offset = 0.5 * (1.0 - adjust.contrast);

    let overlay = adjust.overlay.map(|Rgba([r, g, b, a])| {
        let oa = a as f32 / 255.0;
        (
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            oa,
            1.0 - oa,
        )
    });

    let mut out = src.clone();
    for pix in &mut out.pixels {
        let alpha = pix.a();
        let r = pix.r() as f32 / 255.0;
        let g = pix.g() as f32 / 255.0;
        let b = pix.b() as f32 / 255.0;

        let (mut r, mut g, mut b) =
            matrix.transform(scale * r + offset, scale * g + offset, scale * b + offset);

        // CSS filters apply in list order and `css_filter()` appends
        // invert(1) last, after the hue/sat/brightness pipeline.
        if adjust.invert {
            r = 1.0 - clamp01(r);
            g = 1.0 - clamp01(g);
            b = 1.0 - clamp01(b);
        }

        if let Some((oc_r, oc_g, oc_b, oa, inv_oa)) = overlay {
            r = r * inv_oa + oc_r * oa;
            g = g * inv_oa + oc_g * oa;
            b = b * inv_oa + oc_b * oa;
        }

        *pix = Color32::from_rgba_unmultiplied(to_u8(r), to_u8(g), to_u8(b), alpha);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::ColorImage;

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
            invert: true,
            overlay: Some(Rgba([10, 20, 30, 64])),
        };
        let [_, _, _, a] = pixel(&adjusted(&src, &adjust));
        assert_eq!(a, 77);
    }

    #[test]
    fn invert_flips_channels() {
        let src = solid([40, 100, 220, 255]);
        let adjust = ImageAdjust {
            invert: true,
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
            invert: true,
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
}
