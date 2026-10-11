//! Review sheets for Connections and the Generate chooser (PK1 / PK2).
//! `cargo test -p slate --lib review::packs -- --ignored`

use super::super::packs::{chooser_in_view, generate_chooser_open, pack_board};
use super::*;
use atlas_ai::packs::{PackHealth, OPENAI};

/// Popups fade in over a few frames; shoot once they have settled.
fn shoot(h: &mut Harness, raster: &mut FrameRaster, item: &str, state: &str) {
    for _ in 0..12 {
        capture_frame(h, raster, |_| {});
    }
    let out = capture_frame(h, raster, |_| {});
    review_shot(h, raster, out, item, state);
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn pk1_connections() {
    let mut raster = FrameRaster::new(1440, 900);
    for dark in [true, false] {
        let mut h = pack_board("pk1_connections", &["comfy", "pdfium"]);
        h.app.dark_mode = dark;
        h.app.tab_mut().chrome.advanced_open = true;
        let theme = if dark { "dark" } else { "light" };
        shoot(&mut h, &mut raster, "PK1", &format!("connections_{theme}"));
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn pk2_generate_chooser() {
    let mut raster = FrameRaster::new(1440, 900);
    for dark in [true, false] {
        let theme = if dark { "dark" } else { "light" };
        let mut h = pack_board("pk2_generate", &["comfy"]);
        h.app.dark_mode = dark;
        generate_chooser_open(&mut h, |h| {
            capture_frame(h, &mut raster, |_| {});
        });
        shoot(
            &mut h,
            &mut raster,
            "PK2",
            &format!("generate_no_key_{theme}"),
        );
        h.app.pin_pack_for_test(OPENAI, PackHealth::Ok);
        shoot(
            &mut h,
            &mut raster,
            "PK2",
            &format!("generate_with_key_{theme}"),
        );
    }
    let mut h = pack_board("pk2_generate_bare", &[]);
    generate_chooser_open(&mut h, |h| {
        capture_frame(h, &mut raster, |_| {});
    });
    shoot(
        &mut h,
        &mut raster,
        "PK2",
        "generate_nothing_installed_dark",
    );
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn pk2_program_grid_and_key_entry() {
    let mut raster = FrameRaster::new(1440, 900);
    let mut h = pack_board("pk2_grid", &["codex"]);
    chooser_in_view(&mut h);
    shoot(&mut h, &mut raster, "PK2", "grid_no_key");
    h.app.ai.packs.key_entry = true;
    shoot(&mut h, &mut raster, "PK2", "key_entry");
    h.app.ai.packs.key_entry = false;
    h.app.pin_pack_for_test(OPENAI, PackHealth::Ok);
    h.app.pin_pack_for_test("cursor", PackHealth::Ok);
    h.app.ai.packs.refresh(None);
    super::super::packs::settle(&mut h);
    shoot(&mut h, &mut raster, "PK2", "grid_with_key");
}
