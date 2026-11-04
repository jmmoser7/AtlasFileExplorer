//! Test capture helpers; mesh rasterization has one production owner.
use super::*;
pub(crate) use atlas_shell::frame_raster::FrameRaster;

/// Child-process opt-in for disposable CPU review evidence. Legacy review paths
/// retain their existing behavior; owned outputs must never replace evidence.
pub(crate) fn review_output_path(item: &str, name: &str) -> PathBuf {
    let root = std::env::var_os("SLATE_REVIEW_OUTPUT_ROOT").map(PathBuf::from);
    review_output_path_at(root.as_deref(), item, name).expect("reserve review output")
}

fn review_output_path_at(
    owned_root: Option<&std::path::Path>,
    item: &str,
    name: &str,
) -> std::io::Result<PathBuf> {
    use std::path::Component;
    let root = if let Some(root) = owned_root {
        if !root.is_absolute()
            || [item, name].iter().any(|value| {
                let mut parts = std::path::Path::new(value).components();
                !matches!(parts.next(), Some(Component::Normal(_))) || parts.next().is_some()
            })
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "owned review root must be absolute and output names must be single components",
            ));
        }
        root.to_path_buf()
    } else {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/review")
    };
    let directory = root.join(item);
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(name);
    if owned_root.is_some() {
        // Existing encoders accept paths. Reserve ownership atomically before
        // those encoders replace only this newly created empty file.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
    }
    Ok(path)
}

#[test]
fn owned_review_output_rejects_collisions_and_invalid_paths() {
    let root = std::env::temp_dir().join(format!("slate-review-output-{}", now_nanos()));
    let path = review_output_path_at(Some(&root), "area", "state.png").unwrap();
    std::fs::write(&path, b"retained evidence").unwrap();
    assert_eq!(
        review_output_path_at(Some(&root), "area", "state.png")
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"retained evidence");
    for (directory, item, name) in [
        (std::path::Path::new("relative"), "area", "state.png"),
        (root.as_path(), "..", "state.png"),
        (root.as_path(), "area", "../state.png"),
    ] {
        assert_eq!(
            review_output_path_at(Some(directory), item, name)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}

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
    raster.save(&review_output_path(item, &format!("{state}.png")));
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
    raster.save_crop(
        &review_output_path(item, &format!("{state}.png")),
        area,
        magnify,
    );
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
