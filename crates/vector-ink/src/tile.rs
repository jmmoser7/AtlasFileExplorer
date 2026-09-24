//! World-aligned brush tiles.
//!
//! A committed stamp is max-composited inside itself, then source-over onto
//! whatever is already under it. Tiles cut that same image on a shared pixel
//! grid. Because each pixel is independent, stamping a stroke into a tile and
//! into a full canvas that shares `origin` and `pixel` writes the same bytes.

use crate::stamp::{apply_erase, stamp_polyline, StampImage, TipPoint};

/// Side length of a board tile, in pixels. Tests pass a smaller size.
pub const TILE_PX: u32 = 512;

/// One committed stroke, already in world space: ink contours, then erase
/// passes oldest-first (the same order as [`apply_erase`]).
#[derive(Clone, Debug)]
pub struct StrokeInk {
    pub contours: Vec<Vec<TipPoint>>,
    pub erase: Vec<Vec<TipPoint>>,
}

/// Inclusive-exclusive world box `[min_x, min_y, max_x, max_y]`.
pub fn ink_bounds(stroke: &StrokeInk) -> Option<[f32; 4]> {
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    let mut any = false;
    for p in stroke.contours.iter().chain(stroke.erase.iter()).flatten() {
        if !p.pos[0].is_finite() || !p.pos[1].is_finite() {
            continue;
        }
        let r = p.tip.diameter.max(0.0) * 0.5;
        any = true;
        b[0] = b[0].min(p.pos[0] - r);
        b[1] = b[1].min(p.pos[1] - r);
        b[2] = b[2].max(p.pos[0] + r);
        b[3] = b[3].max(p.pos[1] + r);
    }
    any.then_some(b)
}

/// Tile column/row containing `world` when tiles are `tile_px` pixels of `pixel`
/// world units and the grid is anchored at world origin.
pub fn tile_index(world: f32, pixel: f32, tile_px: u32) -> i32 {
    let span = tile_px as f32 * pixel;
    if span <= 0.0 {
        return 0;
    }
    (world / span).floor() as i32
}

pub fn tile_origin(tx: i32, ty: i32, pixel: f32, tile_px: u32) -> [f32; 2] {
    let span = tile_px as f32 * pixel;
    [tx as f32 * span, ty as f32 * span]
}

/// Source-over `src` onto `dst` inside `region` (`[x0, y0, x1, y1)`). Both
/// buffers are straight RGBA of the same width. A zero source alpha leaves
/// the destination untouched, matching a transparent stamp pixel.
pub fn source_over_region(dst: &mut [u8], src: &[u8], width: u32, region: [u32; 4]) {
    let height = (dst.len() / 4) as u32 / width.max(1);
    let [x0, y0, x1, y1] = region;
    let x1 = x1.min(width);
    let y1 = y1.min(height);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * width + x) * 4) as usize;
            let sa = src[i + 3];
            if sa == 0 {
                continue;
            }
            if sa == 255 || dst[i + 3] == 0 {
                dst[i..i + 4].copy_from_slice(&src[i..i + 4]);
                continue;
            }
            let da = dst[i + 3] as u32;
            let inv = 255 - sa as u32;
            let out_a = sa as u32 + (da * inv + 127) / 255;
            if out_a == 0 {
                dst[i..i + 4].fill(0);
                continue;
            }
            for c in 0..3 {
                let num =
                    src[i + c] as u32 * sa as u32 + (dst[i + c] as u32 * da * inv + 127) / 255;
                dst[i + c] = ((num + out_a / 2) / out_a).min(255) as u8;
            }
            dst[i + 3] = out_a.min(255) as u8;
        }
    }
}

/// Max-composite `stroke` into a cleared layer, erase it, then source-over
/// onto `dst`. `layer` must match `dst`'s dimensions; it is left cleared in
/// the stroke's pixel box.
pub fn composite_stroke(dst: &mut StampImage, layer: &mut StampImage, stroke: &StrokeInk) {
    debug_assert_eq!(dst.width, layer.width);
    debug_assert_eq!(dst.height, layer.height);
    debug_assert_eq!(dst.origin, layer.origin);
    debug_assert_eq!(dst.pixel, layer.pixel);
    let Some(bounds) = ink_bounds(stroke) else {
        return;
    };
    let Some(region) = pixel_box(dst, bounds) else {
        return;
    };
    clear_region(&mut layer.rgba, layer.width, region);
    for contour in &stroke.contours {
        stamp_polyline(layer, contour);
    }
    if !stroke.erase.is_empty() {
        apply_erase(layer, &stroke.erase);
    }
    source_over_region(&mut dst.rgba, &layer.rgba, dst.width, region);
    clear_region(&mut layer.rgba, layer.width, region);
}

/// Composite `strokes` in order onto `dst` (which the caller zeroes).
pub fn composite_strokes(dst: &mut StampImage, strokes: &[StrokeInk]) {
    let mut layer = StampImage {
        width: dst.width,
        height: dst.height,
        origin: dst.origin,
        pixel: dst.pixel,
        rgba: vec![0u8; dst.rgba.len()],
    };
    for stroke in strokes {
        composite_stroke(dst, &mut layer, stroke);
    }
}

/// Same composite as [`composite_strokes`], but each stroke is stamped into
/// the tiles it touches and the tiles are copied back. `dst.origin` is the
/// world position of pixel `(0, 0)` and must sit on the tile grid (tile
/// `(0, 0)`'s origin) so tile pixels and canvas pixels are the same samples.
pub fn composite_strokes_tiled(dst: &mut StampImage, strokes: &[StrokeInk], tile_px: u32) {
    if tile_px == 0 || dst.width == 0 || dst.height == 0 {
        return;
    }
    let mut tiles: Vec<TileBuf> = Vec::new();
    for stroke in strokes {
        let Some(bounds) = ink_bounds(stroke) else {
            continue;
        };
        let tx0 = tile_index(bounds[0], dst.pixel, tile_px);
        let ty0 = tile_index(bounds[1], dst.pixel, tile_px);
        let tx1 = tile_index(bounds[2], dst.pixel, tile_px);
        let ty1 = tile_index(bounds[3], dst.pixel, tile_px);
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let tile = tile_mut(&mut tiles, dst, tile_px, tx, ty);
                composite_stroke(&mut tile.img, &mut tile.layer, stroke);
            }
        }
    }
    for tile in &tiles {
        blit_tile(dst, &tile.img, tile.ox, tile.oy);
    }
}

struct TileBuf {
    img: StampImage,
    layer: StampImage,
    ox: u32,
    oy: u32,
}

fn tile_mut<'a>(
    tiles: &'a mut Vec<TileBuf>,
    dst: &StampImage,
    tile_px: u32,
    tx: i32,
    ty: i32,
) -> &'a mut TileBuf {
    if let Some(i) = tiles.iter().position(|t| {
        let origin = tile_origin(tx, ty, dst.pixel, tile_px);
        t.img.origin == origin
    }) {
        return &mut tiles[i];
    }
    let origin = tile_origin(tx, ty, dst.pixel, tile_px);
    let ox = ((origin[0] - dst.origin[0]) / dst.pixel).round().max(0.0) as u32;
    let oy = ((origin[1] - dst.origin[1]) / dst.pixel).round().max(0.0) as u32;
    let w = tile_px.min(dst.width.saturating_sub(ox)).max(1);
    let h = tile_px.min(dst.height.saturating_sub(oy)).max(1);
    let pixels = (w as usize) * (h as usize) * 4;
    let img = StampImage {
        width: w,
        height: h,
        origin,
        pixel: dst.pixel,
        rgba: vec![0u8; pixels],
    };
    let layer = StampImage {
        width: w,
        height: h,
        origin,
        pixel: dst.pixel,
        rgba: vec![0u8; pixels],
    };
    tiles.push(TileBuf { img, layer, ox, oy });
    tiles.last_mut().expect("just pushed")
}

fn blit_tile(dst: &mut StampImage, src: &StampImage, ox: u32, oy: u32) {
    let stride = dst.width as usize * 4;
    let src_stride = src.width as usize * 4;
    for row in 0..src.height {
        let y = oy + row;
        if y >= dst.height {
            break;
        }
        let copy = src_stride.min(stride.saturating_sub(ox as usize * 4));
        let d0 = y as usize * stride + ox as usize * 4;
        let s0 = row as usize * src_stride;
        dst.rgba[d0..d0 + copy].copy_from_slice(&src.rgba[s0..s0 + copy]);
    }
}

fn pixel_box(img: &StampImage, bounds: [f32; 4]) -> Option<[u32; 4]> {
    let x0 = ((bounds[0] - img.origin[0]) / img.pixel).floor().max(0.0) as u32;
    let y0 = ((bounds[1] - img.origin[1]) / img.pixel).floor().max(0.0) as u32;
    let x1 = ((bounds[2] - img.origin[0]) / img.pixel).ceil() as i32;
    let y1 = ((bounds[3] - img.origin[1]) / img.pixel).ceil() as i32;
    let x0 = x0.saturating_sub(1).min(img.width);
    let y0 = y0.saturating_sub(1).min(img.height);
    let x1 = (x1 + 1).clamp(0, img.width as i32) as u32;
    let y1 = (y1 + 1).clamp(0, img.height as i32) as u32;
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
}

fn clear_region(rgba: &mut [u8], width: u32, region: [u32; 4]) {
    let [x0, y0, x1, y1] = region;
    let stride = width as usize * 4;
    for y in y0..y1 {
        let row = y as usize * stride;
        rgba[row + x0 as usize * 4..row + x1 as usize * 4].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StampStyle;

    fn tip(x: f32, y: f32, diameter: f32, softness: f32, rgba: [u8; 4]) -> TipPoint {
        TipPoint {
            pos: [x, y],
            tip: StampStyle {
                diameter,
                softness,
                rgba,
            },
        }
    }

    fn canvas() -> StampImage {
        StampImage {
            width: 96,
            height: 64,
            origin: [0.0, 0.0],
            pixel: 1.0,
            rgba: vec![0u8; 96 * 64 * 4],
        }
    }

    fn strokes() -> Vec<StrokeInk> {
        vec![
            StrokeInk {
                contours: vec![vec![
                    tip(4.0, 20.0, 14.0, 0.65, [220, 40, 40, 180]),
                    tip(50.0, 18.0, 18.0, 0.2, [220, 40, 40, 180]),
                    tip(88.0, 30.0, 10.0, 0.9, [20, 40, 200, 140]),
                    tip(40.0, 28.0, 16.0, 0.4, [220, 40, 40, 180]),
                ]],
                erase: vec![vec![
                    tip(30.0, 20.0, 12.0, 0.3, [0, 0, 0, 200]),
                    tip(46.0, 22.0, 8.0, 0.0, [0, 0, 0, 255]),
                ]],
            },
            StrokeInk {
                contours: vec![vec![
                    tip(10.0, 40.0, 22.0, 1.0, [20, 180, 80, 160]),
                    tip(70.0, 48.0, 12.0, 0.5, [240, 220, 40, 220]),
                ]],
                erase: Vec::new(),
            },
            StrokeInk {
                contours: vec![vec![tip(48.0, 24.0, 20.0, 0.15, [255, 255, 255, 90])]],
                erase: Vec::new(),
            },
        ]
    }

    fn max_diff(a: &[u8], b: &[u8]) -> u8 {
        a.iter()
            .zip(b)
            .map(|(p, q)| p.abs_diff(*q))
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn tiled_composite_matches_per_stroke_composite() {
        let strokes = strokes();
        let mut direct = canvas();
        composite_strokes(&mut direct, &strokes);
        let mut tiled = canvas();
        composite_strokes_tiled(&mut tiled, &strokes, 32);
        let diff = max_diff(&direct.rgba, &tiled.rgba);
        assert_eq!(diff, 0, "tile grid drifted by {diff}");
        assert!(
            direct.rgba.iter().any(|b| *b != 0),
            "fixture painted nothing"
        );
    }

    #[test]
    fn appending_a_stroke_onto_tiles_matches_a_full_composite() {
        let strokes = strokes();
        let mut full = canvas();
        composite_strokes(&mut full, &strokes);
        // Prefix tiles, then source-over the newest stroke into those pixels.
        let mut prefix = canvas();
        composite_strokes_tiled(&mut prefix, &strokes[..2], 32);
        let mut layer = StampImage {
            rgba: vec![0u8; prefix.rgba.len()],
            ..prefix.clone()
        };
        composite_stroke(&mut prefix, &mut layer, &strokes[2]);
        assert_eq!(max_diff(&full.rgba, &prefix.rgba), 0);
    }
}
