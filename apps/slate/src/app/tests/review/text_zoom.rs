//! Review sheets for zoom-stable canvas text (ledger item TX1).
//!
//! Each object is shot at zooms 0.5, 1, 2, and 4, and the four crops are
//! resampled to one size side by side in `<object>_compare.png`, so a human
//! can check that every line breaks at the same word.

use super::*;
use crate::app::tests::text::zoom_stable::{Fixture, Object};

const ZOOMS: [(f32, &str); 4] = [(0.5, "z050"), (1.0, "z100"), (2.0, "z200"), (4.0, "z400")];
const W: usize = 2000;
const H: usize = 1400;
const TILE_W: u32 = 480;
const GAP: u32 = 16;

fn big_screen(input: &mut egui::RawInput) {
    input.screen_rect = Some(ERect::from_min_size(
        Pos2::ZERO,
        EVec2::new(W as f32, H as f32),
    ));
}

fn crop(raster: &FrameRaster, rect: ERect) -> image::RgbImage {
    let x0 = rect.min.x.max(0.0) as u32;
    let y0 = rect.min.y.max(0.0) as u32;
    let x1 = (rect.max.x.min(raster.w as f32) as u32).max(x0 + 1);
    let y1 = (rect.max.y.min(raster.h as f32) as u32).max(y0 + 1);
    let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    image::RgbImage::from_fn(x1 - x0, y1 - y0, |x, y| {
        let p = raster.px[(y0 + y) as usize * raster.w + (x0 + x) as usize];
        image::Rgb([q(p[0]), q(p[1]), q(p[2])])
    })
}

fn sheet(object: Object) {
    let mut fixture = Fixture::new(object);
    let mut raster = FrameRaster::new(W, H);
    let mut tiles = Vec::new();
    for (zoom, tag) in ZOOMS {
        fixture.prepare(zoom);
        capture_frame(&mut fixture.h, &mut raster, big_screen);
        let out = capture_frame(&mut fixture.h, &mut raster, big_screen);
        let state = format!("{}_{tag}", object.name());
        review_shot(&mut fixture.h, &mut raster, out, "TX1", &state);
        tiles.push(crop(&raster, fixture.screen_rect().expand(8.0 * zoom)));
    }
    let first = &tiles[0];
    let tile_h = (TILE_W as f32 * first.height() as f32 / first.width() as f32).round() as u32;
    let mut compare = image::RgbImage::from_pixel(
        GAP + (TILE_W + GAP) * tiles.len() as u32,
        tile_h + GAP * 2,
        image::Rgb([90, 90, 90]),
    );
    for (i, tile) in tiles.iter().enumerate() {
        let tile =
            image::imageops::resize(tile, TILE_W, tile_h, image::imageops::FilterType::Triangle);
        let x = GAP + (TILE_W + GAP) * i as u32;
        image::imageops::overlay(&mut compare, &tile, x as i64, GAP as i64);
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/review/TX1");
    std::fs::create_dir_all(&dir).unwrap();
    compare
        .save(dir.join(format!("{}_compare.png", object.name())))
        .unwrap();
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn tx1_chat_card_at_four_zooms() {
    sheet(Object::Chat);
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn tx1_text_node_at_four_zooms() {
    sheet(Object::TextNode);
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn tx1_shape_text_at_four_zooms() {
    sheet(Object::ShapeText);
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn tx1_snippet_card_at_four_zooms() {
    sheet(Object::Snippet);
}
