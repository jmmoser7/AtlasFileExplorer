//! Stroke bitmap jobs asked again, and freed when a tab closes.

use super::*;

/// A line job that panics on its worker is not waited on forever: the live
/// canvas asks again, and the segment's exact stamp lands.
#[test]
fn a_lost_line_job_is_asked_again() {
    let (mut h, mut raster, _, c) = shift_chain_board("r10_lost_job");
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    h.app.brush_live.as_mut().expect("parked canvas").panic_next = true;
    let asks = h.app.brush_tiles.line_tags_issued();
    hold_shift_drag(
        &mut h,
        &mut raster,
        &[p(60.0, 140.0), p(200.0, 100.0), p(300.0, 100.0)],
    );
    let canvas = h.app.brush_live.as_ref().expect("live canvas");
    assert!(!canvas.panic_next, "no job took the panic");
    assert!(canvas.line_exact(), "the exact stamp landed");
    assert!(canvas.settled(), "a job is still awaited");
    assert!(
        h.app.brush_tiles.line_tags_issued() >= asks + 2,
        "the lost job was never asked again"
    );
    assert_eq!(h.app.brush_tiles.lines_wanted_len(), 0);
    let xf = h.app.board_xf();
    assert!(red_at(&raster, &xf, p(200.0, 100.0)), "the segment is dark");
}

/// Review r16 D3: closing a tab frees every cached bitmap of its strokes,
/// its nested boards' included, and every raster landed for it.
#[test]
fn closing_a_tab_frees_every_bitmap_of_its_strokes() {
    let (mut h, mut raster, id, _) = eraser_bar_board("eraser_close_frees");
    nested_bar_raster_landed(&mut h, &mut raster, id);
    assert!(!h.app.brush_stamps.is_empty(), "the child bar has a bitmap");
    let first = h.app.active_tab;
    h.app.new_tab();
    shot(&mut h, &mut raster, |_| {});
    h.app.force_close_tab(first);
    assert_eq!(h.app.brush_stamps.len(), 0, "the closed tab's bitmaps stay");
    let landed =
        h.app.brush_tiles.stroke_landed_len(true) + h.app.brush_tiles.stroke_landed_len(false);
    assert_eq!(landed, 0, "the closed tab's landed rasters stay");
}
