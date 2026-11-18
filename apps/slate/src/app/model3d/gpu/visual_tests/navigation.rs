//! Real-GL camera, mesh navigation and exact refinement regressions.
use super::*;

/// A red framing member underneath a blue membrane. Compare fully clear,
/// authored partial opacity, and opaque at the same interior sample; the
/// center must retain the beam and also acquire the membrane tint.
pub(super) fn check_translucent_roof(engine: &ModelEngine) -> Result<(), String> {
    let plane = |y: f32, half_width: f32, rgb, opacity| {
        let mut material = model_preview::PreviewMaterial::from_srgb(rgb);
        material.base_color[3] = opacity;
        PreviewMesh {
            positions: vec![
                [-half_width, y, -1.0],
                [half_width, y, -1.0],
                [half_width, y, 1.0],
                [-half_width, y, 1.0],
            ],
            normals: vec![[0.0, -1.0, 0.0]; 4],
            indices: vec![0, 1, 2, 0, 2, 3],
            material: Some(material),
            ..Default::default()
        }
    };
    let cam = ModelCamera {
        yaw: -std::f32::consts::FRAC_PI_2,
        pitch: 0.0,
        distance: 4.0,
        ..Default::default()
    };
    let mut samples = Vec::new();
    for opacity in [0.0, 0.28, 1.0] {
        let scene = PreviewScene {
            format: "3dm",
            meshes: vec![
                plane(0.0, 0.35, [235, 55, 30], 1.0),
                plane(-0.2, 1.0, [30, 85, 225], opacity),
            ],
            bounds_min: [-1.0, -0.2, -1.0],
            bounds_max: [1.0, 0.0, 1.0],
            notes: vec![],
        };
        let model = engine.upload(&scene).ok_or("roof test upload failed")?;
        let image = engine
            .render(&model, &cam, 128, 128, None)
            .ok_or("roof test render failed")?;
        samples.push(image.pixels[64 * 128 + 64]);
        engine.free(model);
    }
    let (clear, membrane, opaque) = (samples[0], samples[1], samples[2]);
    if !(clear.r() > membrane.r()
        && membrane.r() > opaque.r()
        && clear.b() < membrane.b()
        && membrane.b() < opaque.b()
        && membrane.r() > membrane.b()
        && opaque.b() > opaque.r())
    {
        return Err(format!(
            "Translucent membrane obscures or loses its framing: {samples:?}"
        ));
    }
    Ok(())
}

/// Verify that a stale partial render never reaches the display and that only
/// the last completed job replaces it. Timings include software completion of
/// each submission, matching the interval for which the UI thread is occupied.
pub(super) fn check_refinement(
    engine: &ModelEngine,
    model: &GpuModel,
    cam: ModelCamera,
    size: (u32, u32, f32),
    reference: &egui::ColorImage,
) -> Result<String, String> {
    let mut target = engine
        .new_live_target()
        .ok_or("refinement target unavailable")?;
    let mut stale = cam;
    stale.yaw -= 0.07;
    if !engine.render_frame(
        &mut target,
        model,
        &stale,
        size,
        None,
        FrameQuality::Navigating,
    ) {
        return Err("draft failed".into());
    }
    let displayed = engine.read_target(&target).ok_or("draft readback failed")?;
    if !matches!(
        engine.refine_live(&mut target, model, &stale, size, None),
        LiveDraw::Pending
    ) {
        return Err("expected partial stale refinement".into());
    }
    if engine.read_target(&target).as_ref() != Some(&displayed) {
        return Err("a partial refinement replaced the displayed frame".into());
    }
    // Change pose during refinement and verify the restarted job draws the
    // same final pixels as one uninterrupted direct render.
    let mut steps = Vec::new();
    let mut worst = (0.0, ("restart", 0, 0));
    let start = Instant::now();
    loop {
        let stage = target
            .refinement
            .as_ref()
            .filter(|_| !steps.is_empty())
            .map(|job| job.step())
            .unwrap_or(("restart", 0, 0));
        let step = Instant::now();
        let result = engine.refine_live(&mut target, model, &cam, size, None);
        let ms = step.elapsed().as_secs_f64() * 1000.0;
        if worst.0 < ms {
            worst = (ms, stage);
        }
        steps.push(ms);
        match result {
            LiveDraw::Complete => break,
            LiveDraw::Pending => {}
            _ => return Err("refinement failed".into()),
        }
        if steps.len() == 1 && engine.read_target(&target).as_ref() != Some(&displayed) {
            return Err("the restarted partial frame became visible".into());
        }
        if steps.len() > 200_000 {
            return Err("refinement made no bounded progress".into());
        }
    }
    if engine.read_target(&target).as_ref() != Some(reference) {
        return Err(format!(
            "{:?}: refined display differs from direct full-quality pixels",
            cam.display
        ));
    }
    steps.sort_by(f64::total_cmp);
    let n = steps.len();
    let report = format!(
        "Exact cancellable refinement: {n} steps, total {:.1}ms, step median {:.2}ms, p95 {:.2}ms, max {:.2}ms at {:?}",
        start.elapsed().as_secs_f64()*1000.0, steps[n/2], steps[(n*95).div_ceil(100)-1],steps[n-1],worst.1
    );
    engine.free_target(target);
    Ok(report)
}

/// A second benchmark can target close architectural details at native scale.
fn benchmark_camera(model: &GpuModel) -> ModelCamera {
    let mut cam = resolve_camera(&ModelCamera::default(), model.bounds_min, model.bounds_max);
    cam.materials = true;
    if std::env::var_os("SLATE_BENCH_CATHEDRAL_DETAIL").is_some() {
        cam.target = [0.0, 0.0, 32.0];
        cam.distance = 12.0;
        cam.pitch = 0.05;
        cam.yaw = -std::f32::consts::FRAC_PI_2;
    }
    cam
}

/// Replay identical camera poses with the original per-mask draw ranges and
/// the optimized material ranges. Finish GL for timing, outside the timed
/// CPU submission interval; read back only for the settled image comparison.
pub(super) fn benchmark_linked_model(
    gl: Arc<glow::Context>,
    path: &Path,
) -> Result<String, String> {
    unsafe { while gl.get_error() != glow::NO_ERROR {} }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let scene = model_preview::load_preview(path, &bytes).map_err(|e| format!("{e:?}"))?;
    let decode = start.elapsed();
    let engine = ModelEngine::new(gl.clone()).ok_or("GL engine unavailable")?;
    check_translucent_roof(&engine)?;
    let start = Instant::now();
    let mut model = engine.upload(&scene).ok_or("linked model upload failed")?;
    unsafe { gl.finish() };
    let triangles = model.ground_offset / 12;
    let mut report = format!("device={} mesh parts={} triangles={triangles} mask ranges={} shaded ranges={} decode={decode:?} upload={:?}\n",
        unsafe { gl.get_parameter_string(glow::RENDERER) }, scene.meshes.len(), model.draws.len(), model.surface_draws.len(), start.elapsed());
    writeln!(
        report,
        "Navigation geometry: {} triangles, max world error {:.6}",
        model
            .navigation
            .as_ref()
            .map_or(triangles, |mesh| mesh.count / 3),
        model.navigation.as_ref().map_or(0.0, |mesh| mesh.error)
    )
    .unwrap();
    let mut target = engine.new_live_target().ok_or("target unavailable")?;
    if std::env::var_os("SLATE_BENCH_REFINEMENT_ONLY").is_some() {
        let mut cam = benchmark_camera(&model);
        cam.yaw += 0.26;
        let reference = engine
            .render(&model, &cam, 1200, 800, None)
            .ok_or("reference unavailable")?;
        writeln!(
            report,
            "{}",
            check_refinement(&engine, &model, cam, (1200, 800, 1.5), &reference)?
        )
        .unwrap();
        engine.free_target(target);
        engine.free(model);
        return Ok(report);
    }
    let mut batches = std::mem::replace(&mut model.surface_draws, model.draws.clone());
    let mut reference = None;
    for (name, quality) in [
        ("per-mask", FrameQuality::Final),
        ("batched", FrameQuality::Final),
        ("navigating", FrameQuality::Navigating),
        ("settled", FrameQuality::Final),
    ] {
        if name == "batched" {
            std::mem::swap(&mut batches, &mut model.surface_draws);
        }
        let mut cam = benchmark_camera(&model);
        let mut submit = Vec::new();
        let mut frame = Vec::new();
        let samples: usize = if engine.software {
            if name == "navigating" {
                10
            } else {
                2
            }
        } else {
            24
        };
        let warmups = if engine.software { 1 } else { 2 };
        let start_yaw = cam.yaw;
        if name == "navigating" {
            let camera = surface::FrameCamera::new(&model, &cam, 1.5, false);
            let depth = view_depth_range(&camera.view, model.bounds_min, model.bounds_max)
                .0
                .max(camera.near);
            let world_per_pixel = 2.0 * depth * (FOV_Y * 0.5).tan() / 800.0;
            let error_px = model
                .navigation
                .as_ref()
                .map(|mesh| mesh.error / world_per_pixel);
            writeln!(report, "Motion start camera target={:?}, distance={:.3}, yaw={:.3}, pitch={:.3}, LOD projected error={error_px:?}px (accept at <=1.5)", cam.target, cam.distance, cam.yaw, cam.pitch).unwrap();
        }
        for pos in 0..samples + warmups {
            cam.yaw = start_yaw + (27 + pos - samples - warmups) as f32 * 0.01;
            let start = Instant::now();
            if !engine.render_frame(&mut target, &model, &cam, (1200, 800, 1.5), None, quality) {
                return Err("frame failed".into());
            }
            let cpu = start.elapsed().as_secs_f64() * 1000.0;
            unsafe { gl.finish() };
            if pos >= warmups {
                submit.push(cpu);
                frame.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        submit.sort_by(f64::total_cmp);
        frame.sort_by(f64::total_cmp);
        let mean = frame.iter().sum::<f64>() / frame.len() as f64;
        let mid = samples / 2;
        let p95 = (samples * 95).div_ceil(100) - 1;
        writeln!(report, "{name}: {samples} measured at 1200x800 submit median={:.3}ms p95={:.3}ms GPU-finished mean={mean:.3}ms median={:.3}ms p95={:.3}ms", submit[mid], submit[p95], frame[mid], frame[p95]).unwrap();
        let image = engine.read_target(&target).ok_or("frame readback failed")?;
        if matches!(name, "navigating" | "settled") {
            if let Ok(prefix) = std::env::var("SLATE_BENCH_OUTPUT_PREFIX") {
                save_png(Path::new(&format!("{prefix}-{name}.png")), &image)?;
            }
        }
        if let Some(reference) = &reference {
            if quality == FrameQuality::Final && reference != &image {
                return Err(format!("{name}: settled frame differs from reference"));
            }
            if quality == FrameQuality::Navigating {
                let delta = mean_difference(reference, &image);
                writeln!(
                    report,
                    "Moving image mean RGB absolute difference from full quality: {delta:.3}/255"
                )
                .unwrap();
            }
        } else {
            reference = Some(image);
        }
        check_gl(&gl, name)?;
    }
    if engine.software {
        let mut cam = benchmark_camera(&model);
        cam.yaw += 0.26;
        writeln!(
            report,
            "{}",
            check_refinement(
                &engine,
                &model,
                cam,
                (1200, 800, 1.5),
                reference.as_ref().unwrap()
            )?
        )
        .unwrap();
    }
    engine.free_target(target);
    engine.free(model);
    Ok(report)
}
