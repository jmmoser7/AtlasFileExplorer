//! Review sheets for toasts and the suggestion box (ledger items CH1–CH3).

use super::*;

const SHORT: &str = "Saved.";
const LONG: &str = "Save the workbook first — assets travel beside the .slate file and cannot be collected until you choose a path.";

fn chrome_board(dark: bool, readouts_hidden: bool) -> Harness {
    let mut h = line_board("chrome_review");
    h.app.dark_mode = dark;
    h.app.tab_mut().chrome.canvas_fullscreen = readouts_hidden;
    h
}

/// Settle two frames (fonts, panel layout), then shoot the third.
fn shoot(h: &mut Harness, raster: &mut FrameRaster, item: &str, state: &str) {
    capture_frame(h, raster, |_| {});
    capture_frame(h, raster, |_| {});
    let out = capture_frame(h, raster, |_| {});
    review_shot(h, raster, out, item, state);
}

/// 2× nearest-neighbour crop of the canvas's lower-left corner, written next
/// to the full frame as `<state>_corner.png`.
fn save_corner(h: &Harness, raster: &FrameRaster, item: &str, state: &str) {
    let canvas = h.app.canvas_rect;
    let (w, ht) = (240usize, 90usize);
    let x0 = canvas.left().max(0.0) as usize;
    let y0 = (canvas.bottom() as usize).saturating_sub(ht - 10);
    let scale = 2;
    let mut img = image::RgbImage::new((w * scale) as u32, (ht * scale) as u32);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let sx = (x0 + x as usize / scale).min(raster.w - 1);
        let sy = (y0 + y as usize / scale).min(raster.h - 1);
        let p = raster.px[sy * raster.w + sx];
        let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        *px = image::Rgb([q(p[0]), q(p[1]), q(p[2])]);
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review")
        .join(item);
    img.save(dir.join(format!("{state}_corner.png"))).unwrap();
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn review_ch1_toasts() {
    let mut raster = FrameRaster::new(1440, 900);
    for (state, dark, msgs) in [
        ("short_toast_light", false, &[SHORT][..]),
        ("long_toast_light", false, &[LONG][..]),
        ("short_toast_dark", true, &[SHORT][..]),
        ("long_toast_dark", true, &[LONG][..]),
        ("stacked_toasts_dark", true, &[LONG, SHORT][..]),
    ] {
        let mut h = chrome_board(dark, false);
        for msg in msgs {
            h.app.toast(*msg);
        }
        shoot(&mut h, &mut raster, "CH1", state);
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn review_ch2_suggestion_palette() {
    let mut raster = FrameRaster::new(1440, 900);
    for (state, dark) in [("bottom_edge_light", false), ("bottom_edge_dark", true)] {
        let mut h = chrome_board(dark, false);
        shoot(&mut h, &mut raster, "CH2", state);
        save_corner(&h, &raster, "CH2", state);
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn review_ch3_readout_chevron_clearance() {
    let mut raster = FrameRaster::new(1440, 900);
    for (state, hidden) in [("readouts_open", false), ("readouts_closed", true)] {
        let mut h = chrome_board(false, hidden);
        shoot(&mut h, &mut raster, "CH3", state);
        save_corner(&h, &raster, "CH3", state);
    }
}

#[test]
fn chrome_toasts_and_suggestion_do_not_overlap_obstructions() {
    for readouts_hidden in [false, true] {
        let mut h = chrome_board(false, readouts_hidden);
        h.app.toast(LONG);
        h.app.toast(SHORT);
        h.frame_output(|_| {});
        let canvas = h.app.canvas_rect;
        let toast = atlas_shell::toast::toast_stack_rect(&h.ctx, canvas, 1440.0, &[LONG, SHORT])
            .expect("toast laid out");
        let palette = atlas_shell::canvas_corner::bottom_tool_palette_rect(canvas);
        assert!(
            toast.bottom() < palette.top(),
            "{toast:?} above {palette:?}"
        );
        let chevron = atlas_shell::canvas_corner::readout_chevron_hit_rect(canvas);
        let suggestion = atlas_shell::canvas_corner::suggestion_button_hit_rect(canvas);
        assert!(!chevron.intersects(suggestion));
        assert!(canvas.contains_rect(chevron) && canvas.contains_rect(suggestion));
    }
}
