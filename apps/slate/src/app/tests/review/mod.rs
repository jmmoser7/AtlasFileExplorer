//! Look-and-feel review sheets: `#[ignore]`d tests that write PNGs to
//! `target/review/<item>/<state>.png` for a human to look at before merge.
//! Item ids come from `docs/requests/`. Run one area with
//! `cargo test -p slate --lib review::chat_train -- --ignored`.

use super::*;

mod board;
mod chat_train;
mod chrome;
mod segment;
mod text_zoom;

#[test]
#[ignore = "review sheet: writes PNGs to target/review/"]
fn review_sheet_smoke() {
    let mut h = line_board("review_smoke");
    let mut raster = FrameRaster::new(1440, 900);
    capture_frame(&mut h, &mut raster, |_| {});
    let out = capture_frame(&mut h, &mut raster, |_| {});
    review_shot(&mut h, &mut raster, out, "smoke", "empty_board");
}
