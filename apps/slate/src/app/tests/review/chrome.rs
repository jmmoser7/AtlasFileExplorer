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

fn shoot(h: &mut Harness, raster: &mut FrameRaster, item: &str, state: &str, toast: Option<&str>) {
    let out = capture_frame(h, raster, |_| {
        if let Some(msg) = toast {
            h.app.toast(msg);
        }
    });
    review_shot(h, raster, out, item, state);
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn review_ch1_toasts() {
    let mut raster = FrameRaster::new(1440, 900);
    for (state, dark, msg) in [
        ("short_toast_light", false, SHORT),
        ("long_toast_light", false, LONG),
        ("short_toast_dark", true, SHORT),
        ("long_toast_dark", true, LONG),
    ] {
        let mut h = chrome_board(dark, false);
        shoot(&mut h, &mut raster, "CH1", state, Some(msg));
    }
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn review_ch2_suggestion_palette() {
    let mut raster = FrameRaster::new(1440, 900);
    let mut h = chrome_board(false, false);
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "CH2", "bottom_corner_light");
    h.app.dark_mode = true;
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "CH2", "bottom_corner_dark");
}

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn review_ch3_readout_chevron_clearance() {
    let mut raster = FrameRaster::new(1440, 900);
    for (state, hidden) in [("readouts_open", false), ("readouts_closed", true)] {
        let mut h = chrome_board(false, hidden);
        let out = capture_frame(&mut h, &mut raster, |_| {});
        review_shot(&mut h, &mut raster, out, "CH3", state);
    }
}

#[test]
fn chrome_toasts_and_suggestion_do_not_overlap_obstructions() {
    let mut h = line_board("chrome_layout");
    h.app.toast(LONG);
    let out = h.frame_output(|_| {});
    let _ = out;
    let canvas = h.app.canvas_rect;
    let ctx = &h.ctx;
    let toast =
        atlas_shell::toast::toast_stack_rect(ctx, canvas, 1440.0, &[LONG]).expect("toast laid out");
    let palette = atlas_shell::canvas_corner::bottom_tool_palette_rect(canvas);
    assert!(!toast.intersects(palette));
    let chevron = atlas_shell::canvas_corner::readout_chevron_hit_rect(canvas);
    let suggestion = atlas_shell::canvas_corner::suggestion_button_hit_rect(canvas);
    assert!(!chevron.intersects(suggestion));
}
