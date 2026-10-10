//! Bitmap composition for copied board pictures (OS clipboard CF_DIBV5 / PNG).

use image::RgbaImage;
use slate_doc::scene::{Node, NodeKind};
use slate_doc::SlateDoc;
use std::path::{Path, PathBuf};

/// A copied bitmap is scaled down past this many pixels or this side length.
const MAX_COPY_PIXELS: f32 = 40.0 * 1024.0 * 1024.0;
const MAX_COPY_SIDE: f32 = 16384.0;

/// The copied pictures composed as they sit on the board: each one as shown
/// (`image_composite::composite_rgba`), scaled so the sharpest keeps its own
/// resolution, turned by its rotation, faded by its opacity. Transparent
/// between pictures. Cloud placeholders are skipped, never downloaded.
pub(crate) fn render_pictures(
    doc: &SlateDoc,
    workbook: Option<&Path>,
    pictures: &[Node],
) -> Option<RgbaImage> {
    let shown: Vec<(&Node, RgbaImage)> = pictures
        .iter()
        .filter_map(|node| {
            let NodeKind::Image(img) = &node.kind else {
                return None;
            };
            let source = super::super::image_composite::item_file(doc, workbook, img.item)?;
            if atlas_core::cloud::is_dehydrated(&source) {
                return None;
            }
            let pixels =
                super::super::image_composite::composite_rgba(doc, node, img, &source, true)?;
            Some((node, pixels))
        })
        .collect();
    let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
    let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut scale: f32 = 0.0;
    for (node, pixels) in &shown {
        for (x, y) in node.rect.corners_rotated(node.rotation_deg) {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        scale = scale
            .max(pixels.width() as f32 / node.rect.w.max(1e-3))
            .max(pixels.height() as f32 / node.rect.h.max(1e-3));
    }
    if shown.is_empty() || scale <= 0.0 {
        return None;
    }
    let (bw, bh) = ((max_x - min_x).max(1e-3), (max_y - min_y).max(1e-3));
    scale = scale
        .min((MAX_COPY_PIXELS / (bw * bh)).sqrt())
        .min(MAX_COPY_SIDE / bw.max(bh));
    let w = (bw * scale).round().max(1.0) as u32;
    let h = (bh * scale).round().max(1.0) as u32;
    let mut canvas = RgbaImage::new(w, h);
    for (node, pixels) in &shown {
        let tw = (node.rect.w * scale).round().max(1.0) as u32;
        let th = (node.rect.h * scale).round().max(1.0) as u32;
        let resized;
        let fitted = if pixels.dimensions() == (tw, th) {
            pixels
        } else {
            resized = image::imageops::resize(pixels, tw, th, image::imageops::Triangle);
            &resized
        };
        let (cx, cy) = node.rect.center();
        let center = ((cx - min_x) * scale, (cy - min_y) * scale);
        draw_turned(&mut canvas, fitted, center, node.rotation_deg, node.opacity);
    }
    Some(canvas)
}

/// Source-over `src` onto `canvas`, centered at `center` and turned
/// clockwise by `rotation_deg` (y down), bilinear-sampled.
fn draw_turned(
    canvas: &mut RgbaImage,
    src: &RgbaImage,
    center: (f32, f32),
    rotation_deg: f32,
    opacity: f32,
) {
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    let (hw, hh) = (sw * 0.5, sh * 0.5);
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    let ex = hw * cos.abs() + hh * sin.abs();
    let ey = hw * sin.abs() + hh * cos.abs();
    let x0 = (center.0 - ex).floor().max(0.0) as u32;
    let y0 = (center.1 - ey).floor().max(0.0) as u32;
    let x1 = ((center.0 + ex).ceil().max(0.0) as u32).min(canvas.width());
    let y1 = ((center.1 + ey).ceil().max(0.0) as u32).min(canvas.height());
    let opacity = opacity.clamp(0.0, 1.0);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - center.0;
            let dy = y as f32 + 0.5 - center.1;
            let sx = dx * cos + dy * sin + hw - 0.5;
            let sy = -dx * sin + dy * cos + hh - 0.5;
            if sx < -0.5 || sy < -0.5 || sx > sw - 0.5 || sy > sh - 0.5 {
                continue;
            }
            let [r, g, b, a] = sample_bilinear(src, sx, sy);
            let a = a * opacity;
            if a <= 0.0 {
                continue;
            }
            let dst = canvas.get_pixel_mut(x, y);
            let da = dst.0[3] as f32 / 255.0;
            let out_a = a + da * (1.0 - a);
            for (c, s) in dst.0[..3].iter_mut().zip([r, g, b]) {
                let d = *c as f32;
                *c = ((s * a + d * da * (1.0 - a)) / out_a)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            dst.0[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Straight-alpha color (0–255) and alpha (0–1) at a fractional pixel,
/// weighting color by alpha so transparent texels do not darken edges.
fn sample_bilinear(src: &RgbaImage, x: f32, y: f32) -> [f32; 4] {
    let max_x = src.width() as f32 - 1.0;
    let max_y = src.height() as f32 - 1.0;
    let (x, y) = (x.clamp(0.0, max_x), y.clamp(0.0, max_y));
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (x0, y0) = (x0 as u32, y0 as u32);
    let x1 = (x0 + 1).min(src.width() - 1);
    let y1 = (y0 + 1).min(src.height() - 1);
    let mut acc = [0.0f32; 4];
    for (px, py, w) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x1, y0, fx * (1.0 - fy)),
        (x0, y1, (1.0 - fx) * fy),
        (x1, y1, fx * fy),
    ] {
        if w <= 0.0 {
            continue;
        }
        let p = src.get_pixel(px, py).0;
        let a = p[3] as f32 / 255.0 * w;
        acc[0] += p[0] as f32 * a;
        acc[1] += p[1] as f32 * a;
        acc[2] += p[2] as f32 * a;
        acc[3] += a;
    }
    if acc[3] <= 0.0 {
        return [0.0; 4];
    }
    [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]]
}

/// A CF_DIBV5 payload: BITMAPV5HEADER, 32-bit BGRA with an alpha mask, sRGB,
/// rows bottom-up (Word rejects a negative height).
pub(crate) fn encode_dibv5(img: &RgbaImage) -> Vec<u8> {
    const HEADER: u32 = 124;
    const BI_BITFIELDS: u32 = 3;
    const LCS_SRGB: u32 = 0x7352_4742;
    const LCS_GM_IMAGES: u32 = 4;
    let (w, h) = img.dimensions();
    let pixel_bytes = w * h * 4;
    let mut out = Vec::with_capacity((HEADER + pixel_bytes) as usize);
    out.extend_from_slice(&HEADER.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(h as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&BI_BITFIELDS.to_le_bytes());
    out.extend_from_slice(&pixel_bytes.to_le_bytes());
    out.extend_from_slice(&[0u8; 16]); // resolution, palette counts
    for mask in [0x00ff_0000u32, 0x0000_ff00, 0x0000_00ff, 0xff00_0000] {
        out.extend_from_slice(&mask.to_le_bytes());
    }
    out.extend_from_slice(&LCS_SRGB.to_le_bytes());
    out.extend_from_slice(&[0u8; 48]); // endpoints, gamma
    out.extend_from_slice(&LCS_GM_IMAGES.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]); // profile data, profile size, reserved
    for row in img.rows().rev() {
        for p in row {
            out.extend_from_slice(&[p.0[2], p.0[1], p.0[0], p.0[3]]);
        }
    }
    out
}

/// The linked file behind a copied node, when that node is a picture that
/// goes on the clipboard as a bitmap.
pub(crate) fn copy_picture_path(
    doc: &SlateDoc,
    workbook: Option<&Path>,
    node: &Node,
) -> Option<PathBuf> {
    let NodeKind::Image(img) = &node.kind else {
        return None;
    };
    let path = super::super::image_composite::item_file(doc, workbook, img.item)?;
    (slate_doc::media_kind(&path) == slate_doc::MediaKind::Image).then_some(path)
}
