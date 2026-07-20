//! Bake Cover Flow cover images for the shared home screen.
//!
//! Atlas: mosaic of sample media under a folder.
//! Slate: single hero image (or first workbook media path).

use crate::recent::{cover_cache_path, covers_dir};
use atlas_core::types::{wants_thumb, Family};
use std::path::{Path, PathBuf};

// Square covers — album-art aspect, matching the Cover Flow shelf.
const COVER_W: u32 = 512;
const COVER_H: u32 = 512;
const MOSAIC_N: usize = 9;

/// Build (or reuse) a folder cover mosaic from up to 9 media files under `root`.
/// Returns the PNG path on success.
pub fn bake_folder_cover(root: &Path) -> Option<PathBuf> {
    let out = cover_cache_path(root);
    if out.is_file() {
        if let Ok(meta) = std::fs::metadata(&out) {
            if meta.len() > 0 {
                return Some(out);
            }
        }
    }
    let samples = sample_media(root, MOSAIC_N);
    if samples.is_empty() {
        return bake_solid_cover(root, [0x1c, 0x20, 0x26]);
    }
    let cell_w = COVER_W / 3;
    let cell_h = COVER_H / 3;
    let mut canvas =
        image::RgbaImage::from_pixel(COVER_W, COVER_H, image::Rgba([0x1c, 0x20, 0x26, 255]));
    for (i, path) in samples.iter().enumerate() {
        let Ok(src) = image::open(path) else {
            continue;
        };
        let img = image::imageops::thumbnail(&src.to_rgba8(), cell_w, cell_h);
        let col = (i % 3) as u32;
        let row = (i / 3) as u32;
        image::imageops::overlay(
            &mut canvas,
            &img,
            (col * cell_w) as i64,
            (row * cell_h) as i64,
        );
    }
    let _ = std::fs::create_dir_all(covers_dir());
    canvas.save(&out).ok()?;
    Some(out)
}

/// Bake a cover from a single image path (Slate workbook hero).
pub fn bake_image_cover(key_path: &Path, image_path: &Path) -> Option<PathBuf> {
    let out = cover_cache_path(key_path);
    if out.is_file() {
        return Some(out);
    }
    let img = image::open(image_path).ok()?.to_rgba8();
    let (nw, nh) = fit_size(img.width(), img.height(), COVER_W, COVER_H);
    let resized = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    let mut canvas =
        image::RgbaImage::from_pixel(COVER_W, COVER_H, image::Rgba([0x1c, 0x20, 0x26, 255]));
    let x = (COVER_W.saturating_sub(resized.width())) / 2;
    let y = (COVER_H.saturating_sub(resized.height())) / 2;
    image::imageops::overlay(&mut canvas, &resized, x as i64, y as i64);
    let _ = std::fs::create_dir_all(covers_dir());
    canvas.save(&out).ok()?;
    Some(out)
}

/// Solid accent-tinted placeholder when no media is available.
pub fn bake_placeholder_cover(key_path: &Path) -> Option<PathBuf> {
    bake_solid_cover(key_path, [0x2d, 0xd4, 0xbf])
}

fn fit_size(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (max_w, max_h);
    }
    let scale = (max_w as f32 / w as f32).min(max_h as f32 / h as f32);
    (
        ((w as f32) * scale).round().max(1.0) as u32,
        ((h as f32) * scale).round().max(1.0) as u32,
    )
}

fn bake_solid_cover(key_path: &Path, rgb: [u8; 3]) -> Option<PathBuf> {
    let out = cover_cache_path(key_path);
    if out.is_file() {
        return Some(out);
    }
    let canvas =
        image::RgbaImage::from_pixel(COVER_W, COVER_H, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
    let _ = std::fs::create_dir_all(covers_dir());
    canvas.save(&out).ok()?;
    Some(out)
}

fn sample_media(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        if out.len() >= limit || visited > 4000 {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut dirs = Vec::new();
        for entry in rd.flatten() {
            visited += 1;
            let path = entry.path();
            let Ok(ft) = entry.file_type() else {
                continue;
            };
            if ft.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if atlas_core::scanner::SKIP_DIRS
                    .iter()
                    .any(|s| name.eq_ignore_ascii_case(s))
                {
                    continue;
                }
                dirs.push(path);
            } else if ft.is_file() {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                let family = Family::from_ext(&ext);
                if wants_thumb(family)
                    && matches!(family, Family::Image | Family::Video | Family::Design)
                {
                    out.push(path);
                    if out.len() >= limit {
                        break;
                    }
                }
            }
        }
        // Prefer breadth: enqueue subdirs after files at this level.
        stack.extend(dirs.into_iter().rev());
    }
    out
}
