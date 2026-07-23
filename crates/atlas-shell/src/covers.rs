//! Bake Cover Flow cover images for the shared home screen.
//!
//! Atlas: media mosaic under a folder, or a miniature folder-tree diagram.
//! Slate: mosaic / hero from workbook-linked media, or a quiet workbook tile.

use crate::recent::{cover_cache_path, covers_dir};
use atlas_core::types::{wants_thumb, Family};
use image::Rgba;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// Square covers — album-art aspect, matching the Cover Flow shelf.
const COVER_W: u32 = 512;
const COVER_H: u32 = 512;
const MOSAIC_N: usize = 9;
const BG: Rgba<u8> = Rgba([0x1c, 0x20, 0x26, 255]);
const CARD: Rgba<u8> = Rgba([0x2a, 0x32, 0x3c, 255]);
const CARD_BORDER: Rgba<u8> = Rgba([0x45, 0x55, 0x66, 255]);
const LINE: Rgba<u8> = Rgba([0x55, 0x66, 0x77, 255]);

/// Build (or reuse) a folder cover from media and/or a structure diagram.
pub fn bake_folder_cover(root: &Path) -> Option<PathBuf> {
    let out = cover_cache_path(root);
    if cache_hit(&out) {
        return Some(out);
    }
    let samples = sample_media(root, MOSAIC_N);
    if !samples.is_empty() {
        return bake_mosaic_cover(root, &samples);
    }
    bake_folder_structure_cover(root)
}

/// Mosaic of up to nine thumbnail-able files (Atlas folders or Slate workbooks).
pub fn bake_mosaic_cover(key_path: &Path, samples: &[PathBuf]) -> Option<PathBuf> {
    let out = cover_cache_path(key_path);
    if cache_hit(&out) {
        return Some(out);
    }
    if samples.is_empty() {
        return None;
    }
    let cell_w = COVER_W / 3;
    let cell_h = COVER_H / 3;
    let mut canvas = image::RgbaImage::from_pixel(COVER_W, COVER_H, BG);
    for (i, path) in samples.iter().take(MOSAIC_N).enumerate() {
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
    save_cover(&out, canvas)
}

/// Workbook cover: mosaic when linked images exist, otherwise a workbook tile.
pub fn bake_workbook_cover(key_path: &Path, media: &[PathBuf]) -> Option<PathBuf> {
    if !media.is_empty() {
        return bake_mosaic_cover(key_path, media);
    }
    bake_workbook_tile_cover(key_path)
}

/// Bake a cover from a single image path (legacy hero — prefer [`bake_mosaic_cover`]).
pub fn bake_image_cover(key_path: &Path, image_path: &Path) -> Option<PathBuf> {
    bake_mosaic_cover(key_path, &[image_path.to_path_buf()])
}

/// Solid accent-tinted placeholder when no media is available.
pub fn bake_placeholder_cover(key_path: &Path) -> Option<PathBuf> {
    bake_solid_cover(key_path, [0x2d, 0xd4, 0xbf])
}

/// Kick off background bakes for folders that do not have a cached PNG yet.
pub fn spawn_missing_folder_covers(folders: impl IntoIterator<Item = PathBuf>) {
    for folder in folders {
        let path = folder.clone();
        if !path.is_dir() {
            continue;
        }
        let cache = cover_cache_path(&path);
        if cache.is_file() || !mark_cover_bake_requested(&path) {
            continue;
        }
        std::thread::spawn(move || {
            let _ = bake_folder_cover(&path);
        });
    }
}

/// Kick off a workbook cover bake when the PNG is missing.
pub fn spawn_missing_workbook_cover(workbook: PathBuf, media: Vec<PathBuf>) {
    if !workbook.is_file() {
        return;
    }
    let cache = cover_cache_path(&workbook);
    if cache.is_file() || !mark_cover_bake_requested(&workbook) {
        return;
    }
    std::thread::spawn(move || {
        let _ = bake_workbook_cover(&workbook, &media);
    });
}

/// Returns `true` when this path was not already queued for a background bake.
pub fn schedule_cover_bake(path: &Path) -> bool {
    mark_cover_bake_requested(path)
}

fn mark_cover_bake_requested(path: &Path) -> bool {
    static IN_FLIGHT: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    let set = IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()));
    set.lock()
        .map(|mut g| g.insert(path.to_path_buf()))
        .unwrap_or(false)
}

fn cache_hit(out: &Path) -> bool {
    out.is_file() && std::fs::metadata(out).map(|m| m.len() > 0).unwrap_or(false)
}

fn save_cover(out: &Path, canvas: image::RgbaImage) -> Option<PathBuf> {
    let _ = std::fs::create_dir_all(covers_dir());
    canvas.save(out).ok()?;
    Some(out.to_path_buf())
}

fn bake_solid_cover(key_path: &Path, rgb: [u8; 3]) -> Option<PathBuf> {
    let out = cover_cache_path(key_path);
    if cache_hit(&out) {
        return Some(out);
    }
    let canvas =
        image::RgbaImage::from_pixel(COVER_W, COVER_H, Rgba([rgb[0], rgb[1], rgb[2], 255]));
    save_cover(&out, canvas)
}

/// Miniature horizontal tree (structure-only) for folders without media thumbs.
fn bake_folder_structure_cover(root: &Path) -> Option<PathBuf> {
    let out = cover_cache_path(root);
    if cache_hit(&out) {
        return Some(out);
    }
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Folder".into());
    let children = immediate_subdir_names(root, 6);
    let mut canvas = image::RgbaImage::from_pixel(COVER_W, COVER_H, BG);
    paint_mini_tree(&mut canvas, &root_name, &children);
    save_cover(&out, canvas)
}

fn bake_workbook_tile_cover(key_path: &Path) -> Option<PathBuf> {
    let out = cover_cache_path(key_path);
    if cache_hit(&out) {
        return Some(out);
    }
    let name = key_path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Workbook".into());
    let mut canvas = image::RgbaImage::from_pixel(COVER_W, COVER_H, BG);
    // Single centered “slide stack” tile — reads as an empty workbook.
    let card_w = 280u32;
    let card_h = 180u32;
    let x0 = (COVER_W - card_w) / 2;
    let y0 = (COVER_H - card_h) / 2 - 20;
    for (i, tint) in [
        (0i32, CARD),
        (4, CARD.gamma_multiply(0.92)),
        (8, CARD.gamma_multiply(0.85)),
    ] {
        fill_rect(
            &mut canvas,
            x0 + i as u32,
            y0 + i as u32,
            x0 + card_w + i as u32,
            y0 + card_h + i as u32,
            tint,
        );
        stroke_rect(
            &mut canvas,
            x0 + i as u32,
            y0 + i as u32,
            x0 + card_w + i as u32,
            y0 + card_h + i as u32,
            CARD_BORDER,
        );
    }
    let _ = name; // reserved for future label rendering
    save_cover(&out, canvas)
}

fn immediate_subdir_names(root: &Path, limit: usize) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            e.file_type().ok()?.is_dir().then_some(())?;
            let name = e.file_name().to_string_lossy().into_owned();
            if atlas_core::scanner::SKIP_DIRS
                .iter()
                .any(|s| name.eq_ignore_ascii_case(s))
            {
                return None;
            }
            Some(name)
        })
        .collect();
    names.sort_by(|a, b| a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()));
    names.truncate(limit);
    names
}

fn paint_mini_tree(canvas: &mut image::RgbaImage, root: &str, children: &[String]) {
    let root_w = 200u32;
    let root_h = 44u32;
    let root_x = (COVER_W - root_w) / 2;
    let root_y = 72u32;
    draw_node(canvas, root_x, root_y, root_w, root_h);

    if children.is_empty() {
        return;
    }

    let child_w = 88u32;
    let child_h = 36u32;
    let gap = 12u32;
    let row_w = children.len() as u32 * child_w + (children.len() as u32 - 1) * gap;
    let row_x0 = (COVER_W.saturating_sub(row_w)) / 2;
    let child_y = root_y + root_h + 80;

    let root_cx = root_x + root_w / 2;
    let root_by = root_y + root_h;
    let trunk_y = root_by + (child_y - root_by) / 2;
    stroke_vline(canvas, root_cx, root_by, trunk_y, LINE);
    stroke_hline(
        canvas,
        row_x0 + child_w / 2,
        row_x0 + row_w - child_w / 2,
        trunk_y,
        LINE,
    );

    for (i, _name) in children.iter().enumerate() {
        let cx = row_x0 + i as u32 * (child_w + gap) + child_w / 2;
        stroke_vline(canvas, cx, trunk_y, child_y, LINE);
        draw_node(
            canvas,
            row_x0 + i as u32 * (child_w + gap),
            child_y,
            child_w,
            child_h,
        );
    }

    let _ = root; // structure silhouette only (no font rasterizer in shell)
}

fn draw_node(canvas: &mut image::RgbaImage, x: u32, y: u32, w: u32, h: u32) {
    fill_rect(canvas, x, y, x + w, y + h, CARD);
    stroke_rect(canvas, x, y, x + w, y + h, CARD_BORDER);
}

fn fill_rect(canvas: &mut image::RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba<u8>) {
    for y in y0..y1.min(COVER_H) {
        for x in x0..x1.min(COVER_W) {
            canvas.put_pixel(x, y, color);
        }
    }
}

fn stroke_rect(canvas: &mut image::RgbaImage, x0: u32, y0: u32, x1: u32, y1: u32, color: Rgba<u8>) {
    stroke_hline(canvas, x0, x1.saturating_sub(1), y0, color);
    stroke_hline(
        canvas,
        x0,
        x1.saturating_sub(1),
        y1.saturating_sub(1),
        color,
    );
    stroke_vline(canvas, x0, y0, y1.saturating_sub(1), color);
    stroke_vline(
        canvas,
        x1.saturating_sub(1),
        y0,
        y1.saturating_sub(1),
        color,
    );
}

fn stroke_hline(canvas: &mut image::RgbaImage, x0: u32, x1: u32, y: u32, color: Rgba<u8>) {
    if y >= COVER_H {
        return;
    }
    for x in x0..=x1.min(COVER_W - 1) {
        canvas.put_pixel(x, y, color);
    }
}

fn stroke_vline(canvas: &mut image::RgbaImage, x: u32, y0: u32, y1: u32, color: Rgba<u8>) {
    if x >= COVER_W {
        return;
    }
    for y in y0..=y1.min(COVER_H - 1) {
        canvas.put_pixel(x, y, color);
    }
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
        stack.extend(dirs.into_iter().rev());
    }
    out
}

trait RgbaGamma {
    fn gamma_multiply(self, gamma: f32) -> Self;
}

impl RgbaGamma for Rgba<u8> {
    fn gamma_multiply(self, gamma: f32) -> Self {
        Rgba([
            ((self.0[0] as f32) * gamma).round().clamp(0.0, 255.0) as u8,
            ((self.0[1] as f32) * gamma).round().clamp(0.0, 255.0) as u8,
            ((self.0[2] as f32) * gamma).round().clamp(0.0, 255.0) as u8,
            self.0[3],
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_cover_writes_png() {
        let dir = std::env::temp_dir().join(format!("atlas_cover_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let sub = dir.join("child_a");
        let _ = std::fs::create_dir_all(&sub);
        let out = bake_folder_structure_cover(&dir).expect("structure cover");
        assert!(out.is_file());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(out);
    }
}
