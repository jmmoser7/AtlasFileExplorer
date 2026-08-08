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
    let mut canvas = image::RgbaImage::from_pixel(COVER_W, COVER_H, BG);
    for (i, path) in samples.iter().take(MOSAIC_N).enumerate() {
        let Ok(src) = image::open(path) else {
            continue;
        };
        let (x0, x1) = cell_bounds((i % 3) as u32, 3, COVER_W);
        let (y0, y1) = cell_bounds((i / 3) as u32, 3, COVER_H);
        let img = fill_cell(&src.to_rgba8(), x1 - x0, y1 - y0);
        image::imageops::overlay(&mut canvas, &img, x0 as i64, y0 as i64);
    }
    save_cover(&out, canvas)
}

/// Half-open pixel bounds of cell `i` of `n` across `total` pixels.
///
/// Integer division alone (`COVER_W / 3`) loses the remainder, which left a
/// two-pixel background gutter down the right edge and along the bottom of every
/// mosaic. Deriving each edge from the total instead makes the cells tile it
/// exactly, at the cost of some cells being a pixel wider than others.
fn cell_bounds(i: u32, n: u32, total: u32) -> (u32, u32) {
    (i * total / n, (i + 1) * total / n)
}

/// Largest centered crop of a `sw × sh` source that has the aspect of a
/// `tw × th` cell, as `(x, y, w, h)`.
fn cover_crop(sw: u32, sh: u32, tw: u32, th: u32) -> (u32, u32, u32, u32) {
    if sw == 0 || sh == 0 || tw == 0 || th == 0 {
        return (0, 0, sw, sh);
    }
    // Compare aspects as cross-multiplied integers to avoid float wobble.
    let (mut w, mut h) = (sw, sh);
    if sw as u64 * th as u64 > tw as u64 * sh as u64 {
        // Source is wider than the cell: keep full height, trim the sides.
        w = ((sh as u64 * tw as u64) / th as u64).max(1) as u32;
    } else {
        h = ((sw as u64 * th as u64) / tw as u64).max(1) as u32;
    }
    let w = w.min(sw);
    let h = h.min(sh);
    ((sw - w) / 2, (sh - h) / 2, w, h)
}

/// Scale a source to exactly fill a mosaic cell, cropping rather than squashing.
///
/// `imageops::thumbnail` resizes to precisely the size asked for and does *not*
/// preserve aspect (that is `DynamicImage::thumbnail`), so handing it a 3:2 photo
/// for a square cell flattened every landscape shot and stretched every portrait
/// one — nine of those in a grid is what made the shelf covers look lumpy. Crop
/// to the cell's shape first; filling is right for a mosaic tile, where
/// letterboxing would only trade distortion for gaps.
fn fill_cell(src: &image::RgbaImage, cell_w: u32, cell_h: u32) -> image::RgbaImage {
    if cell_w == 0 || cell_h == 0 {
        return image::RgbaImage::new(cell_w.max(1), cell_h.max(1));
    }
    let (x, y, w, h) = cover_crop(src.width(), src.height(), cell_w, cell_h);
    if w == 0 || h == 0 {
        return image::RgbaImage::from_pixel(cell_w, cell_h, BG);
    }
    let cropped = image::imageops::crop_imm(src, x, y, w, h).to_image();
    image::imageops::thumbnail(&cropped, cell_w, cell_h)
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
    // First bake request of the run is also when last run's stale covers go.
    let set = IN_FLIGHT.get_or_init(|| {
        crate::recent::prune_stale_covers();
        Mutex::new(HashSet::new())
    });
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
    let skip = atlas_core::skiplist::effective();
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            e.file_type().ok()?.is_dir().then_some(())?;
            let name = e.file_name().to_string_lossy().into_owned();
            if skip.skips(&name) {
                return None;
            }
            Some(name)
        })
        .collect();
    names.sort_by_key(|a| a.to_ascii_lowercase());
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
    let mut dirs_listed = 0usize;
    let skip = atlas_core::skiplist::effective();
    // A cover bake must never compete with discovery on a high-latency share.
    // Listing one directory on `\\ngrimshaw…\Resources` costs several seconds;
    // the old DFS visited up to 4000 entries and could pin the SMB link for
    // minutes after the user merely opened the folder (which is what made Atlas
    // look frozen even before they drilled into anything). One listing is
    // enough to decide mosaic vs structure tile.
    let network = atlas_core::thumbs::is_network_path(root);
    let max_visited = if network { 256 } else { 4000 };
    let max_dirs = if network { 1 } else { usize::MAX };
    while let Some(dir) = stack.pop() {
        if out.len() >= limit || visited > max_visited || dirs_listed >= max_dirs {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        dirs_listed += 1;
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
                if skip.skips(&name) {
                    continue;
                }
                if !network {
                    dirs.push(path);
                }
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
    fn cells_tile_the_cover_with_no_gutter() {
        for total in [512_u32, 510, 511, 100] {
            let mut edge = 0;
            for i in 0..3 {
                let (x0, x1) = cell_bounds(i, 3, total);
                assert_eq!(x0, edge, "cell {i} does not start where the last ended");
                assert!(x1 > x0, "cell {i} of {total} is empty");
                edge = x1;
            }
            assert_eq!(edge, total, "cells left {} px of gutter", total - edge);
        }
    }

    /// A tile fills its cell by cropping, never by squashing — the whole point of
    /// the fix, since `imageops::thumbnail` will happily resize to any aspect.
    #[test]
    fn a_tile_crops_to_the_cell_instead_of_squashing() {
        // Wide source into a square cell: keep the height, trim the sides.
        assert_eq!(cover_crop(300, 100, 90, 90), (100, 0, 100, 100));
        // Tall source into a square cell: keep the width, trim top and bottom.
        assert_eq!(cover_crop(100, 300, 90, 90), (0, 100, 100, 100));
        // Already the right shape: take all of it.
        assert_eq!(cover_crop(640, 640, 170, 170), (0, 0, 640, 640));
        // Aspect is what is matched, not size.
        assert_eq!(cover_crop(400, 100, 200, 100), (100, 0, 200, 100));

        // Whatever the source shape, the crop carries the cell's aspect and stays
        // inside the source — that is what keeps the picture undistorted.
        for (sw, sh) in [
            (4000_u32, 3000_u32),
            (3, 4000),
            (1, 1),
            (1920, 1080),
            (7, 5),
        ] {
            let (x, y, w, h) = cover_crop(sw, sh, 170, 170);
            assert!(
                x + w <= sw && y + h <= sh,
                "crop {sw}x{sh} escaped the source"
            );
            assert!(w > 0 && h > 0, "crop {sw}x{sh} is empty");
            let off = (w as i64 - h as i64).abs();
            assert!(
                off <= 1,
                "crop {w}x{h} from {sw}x{sh} is not square for a square cell"
            );
        }
    }

    #[test]
    fn degenerate_sources_do_not_panic() {
        assert_eq!(cover_crop(0, 0, 170, 170), (0, 0, 0, 0));
        assert_eq!(cover_crop(100, 100, 0, 0), (0, 0, 100, 100));
        let src = image::RgbaImage::from_pixel(1, 1, Rgba([9, 9, 9, 255]));
        let tile = fill_cell(&src, 170, 170);
        assert_eq!((tile.width(), tile.height()), (170, 170));
    }

    /// The tile keeps the middle of the picture, at the right scale: a source
    /// whose center third is a solid block must come out solid, which the old
    /// squash-to-fit could never guarantee.
    #[test]
    fn a_tile_keeps_the_center_of_the_picture() {
        let (red, blue) = (Rgba([255, 0, 0, 255]), Rgba([0, 0, 255, 255]));
        let mut src = image::RgbaImage::from_pixel(300, 100, red);
        for y in 0..100 {
            for x in 100..200 {
                src.put_pixel(x, y, blue);
            }
        }
        let tile = fill_cell(&src, 50, 50);
        assert_eq!((tile.width(), tile.height()), (50, 50));
        for (x, y, px) in tile.enumerate_pixels() {
            assert_eq!(
                px.0, blue.0,
                "tile pixel ({x}, {y}) came from outside the centered square crop"
            );
        }
    }

    #[test]
    fn a_recipe_bump_retires_the_previous_generation() {
        use crate::recent::{cover_cache_path, COVER_RECIPE_VERSION};
        let name = cover_cache_path(Path::new("C:/some/folder"));
        let name = name.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with(&format!("v{COVER_RECIPE_VERSION}-")),
            "cover filename {name} does not carry the recipe version, so a bump \
             could never reach a machine that already has covers"
        );
    }

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
