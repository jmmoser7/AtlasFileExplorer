//! Test capture helpers; mesh rasterization has one production owner.
use super::*;
pub(crate) use atlas_shell::frame_raster::FrameRaster;

/// Run one frame with `prepare`, feeding its textures into `raster`.
pub(crate) fn capture_frame(
    h: &mut Harness,
    raster: &mut FrameRaster,
    prepare: impl FnOnce(&mut egui::RawInput),
) -> egui::FullOutput {
    let mut input = egui::RawInput {
        screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
        ..Default::default()
    };
    prepare(&mut input);
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let out = ctx.run(input, |c| app.update_app(c));
    raster.apply(&out.textures_delta);
    out
}

pub(crate) fn snapshot(
    h: &mut Harness,
    raster: &mut FrameRaster,
    out: egui::FullOutput,
    name: &str,
) {
    rasterize(h, raster, out);
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/brush-validate/frames");
    std::fs::create_dir_all(&dir).unwrap();
    raster.save(&dir.join(format!("{name}.png")));
}

/// Save a look-and-feel review frame to `target/review/<item>/<state>.png`,
/// where `item` is a request id from `docs/requests/` (e.g. `CT1`).
pub(crate) fn review_shot(
    h: &mut Harness,
    raster: &mut FrameRaster,
    out: egui::FullOutput,
    item: &str,
    state: &str,
) {
    raster.sync_fonts(&h.ctx);
    rasterize(h, raster, out);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review")
        .join(item);
    std::fs::create_dir_all(&dir).unwrap();
    raster.save(&dir.join(format!("{state}.png")));
}

/// [`review_shot`] cropped to `area` and magnified, for close-ups of one object.
pub(crate) fn review_shot_crop(
    h: &mut Harness,
    raster: &mut FrameRaster,
    out: egui::FullOutput,
    item: &str,
    state: &str,
    area: ERect,
    magnify: u32,
) {
    raster.sync_fonts(&h.ctx);
    rasterize(h, raster, out);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review")
        .join(item);
    std::fs::create_dir_all(&dir).unwrap();
    raster.save_crop(&dir.join(format!("{state}.png")), area, magnify);
}

/// Draw `out` into `raster` over black.
pub(crate) fn rasterize(h: &mut Harness, raster: &mut FrameRaster, out: egui::FullOutput) {
    // Tiled strokes render on worker threads; let them land first.
    let mut out = out;
    for _ in 0..200 {
        if !h.app.brush_tiles_enabled || h.app.brush_tiles.last.pending_jobs == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mods = h.ctx.input(|i| i.modifiers);
        out = capture_frame(h, raster, |i| i.modifiers = mods);
    }
    let prims = h.ctx.tessellate(out.shapes, out.pixels_per_point);
    for p in raster.px.iter_mut() {
        *p = [0.0, 0.0, 0.0, 1.0];
    }
    raster.draw(&prims);
}

// ---------- vertex picks on closed forms (P1.shape.vertex-style) ----------
