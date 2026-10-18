//! Opt-in production-GL images, parity checks and steady-target performance evidence.

use super::*;
use std::fmt::Write;
use std::sync::Mutex;

const WIDTH: u32 = 900;
const HEIGHT: u32 = 650;
type Outcome = Arc<Mutex<Option<Result<String, String>>>>;

struct QualityApp;

impl eframe::App for QualityApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

mod fixture;

fn check_gl(gl: &glow::Context, stage: &str) -> Result<(), String> {
    let error = unsafe { gl.get_error() };
    if error == glow::NO_ERROR {
        Ok(())
    } else {
        Err(format!("OpenGL error 0x{error:04x} after {stage}"))
    }
}

fn counts(engine: &ModelEngine) -> (u64, u64) {
    (
        engine.gl_creates.load(Ordering::Relaxed),
        engine.readbacks.load(Ordering::Relaxed),
    )
}

#[derive(Default)]
struct PixelStats {
    white: usize,
    midtone: usize,
    dark: usize,
    chromatic: usize,
    nonopaque: usize,
}

fn pixel_stats(image: &egui::ColorImage) -> PixelStats {
    let mut result = PixelStats::default();
    for pixel in &image.pixels {
        let rgb = [pixel.r(), pixel.g(), pixel.b()];
        let min = *rgb.iter().min().unwrap();
        let max = *rgb.iter().max().unwrap();
        let mean = rgb.into_iter().map(u16::from).sum::<u16>() / 3;
        result.white += usize::from(mean > 235);
        result.midtone += usize::from((160..230).contains(&mean));
        result.dark += usize::from(mean < 96);
        result.chromatic += usize::from(max.saturating_sub(min) > 20);
        result.nonopaque += usize::from(pixel.a() != 255);
    }
    result
}

fn mean_difference(a: &egui::ColorImage, b: &egui::ColorImage) -> f64 {
    let total: u64 = a
        .pixels
        .iter()
        .zip(&b.pixels)
        .map(|(a, b)| {
            (0..3)
                .map(|channel| u64::from(a[channel].abs_diff(b[channel])))
                .sum::<u64>()
        })
        .sum();
    total as f64 / (a.pixels.len() * 3) as f64
}

fn save_png(path: &Path, image: &egui::ColorImage) -> Result<(), String> {
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
    image::save_buffer(
        path,
        &rgba,
        image.width() as u32,
        image.height() as u32,
        image::ColorType::Rgba8,
    )
    .map_err(|error| error.to_string())
}

fn verify_targets(engine: &ModelEngine, target: &LiveTarget) -> Result<(), String> {
    let gl = &engine.gl;
    unsafe {
        for (name, fbo) in [
            ("MSAA", target.msaa_fbo),
            ("resolve", target.resolve_fbo),
            ("ambient occlusion", target.ao_fbo),
            ("compose", target.compose_fbo),
            ("output", target.out_fbo),
        ] {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
            if status != glow::FRAMEBUFFER_COMPLETE {
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                return Err(format!("{name} framebuffer incomplete: 0x{status:04x}"));
            }
        }
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
    }
    Ok(())
}

fn render_cases(
    engine: &ModelEngine,
    model: &GpuModel,
    directory: &Path,
) -> Result<String, String> {
    let mut target = engine
        .new_live_target()
        .ok_or("Could not allocate viewport target")?;
    let size = (WIDTH, HEIGHT, WIDTH as f32 / HEIGHT as f32);
    let cases = [
        ("arctic", ModelDisplay::Arctic, true),
        ("hidden-line-white", ModelDisplay::HiddenLine, false),
        ("hidden-line-materials", ModelDisplay::HiddenLine, true),
        ("shaded-white", ModelDisplay::Shaded, false),
        ("shaded-materials", ModelDisplay::Shaded, true),
    ];
    let mut report = String::new();
    let mut failures = Vec::new();
    let mut images = Vec::new();
    for (name, display, materials) in cases {
        let camera = ModelCamera {
            yaw: -0.95,
            pitch: 0.44,
            display,
            materials,
            ..ModelCamera::default()
        };
        if !engine.render_live(&mut target, model, &camera, size, None) {
            return Err(format!("{name}: live renderer rejected the framebuffer"));
        }
        verify_targets(engine, &target)?;
        check_gl(&engine.gl, name)?;
        unsafe {
            engine.gl.finish();
        }
        let before = counts(engine);
        let mut times = Vec::new();
        for _ in 0..12 {
            let start = Instant::now();
            if !engine.render_live(&mut target, model, &camera, size, None) {
                return Err(format!("{name}: retained target failed"));
            }
            unsafe {
                engine.gl.finish();
            }
            times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        if counts(engine) != before {
            failures.push(format!(
                "{name}: steady live rendering allocated GL objects or read pixels"
            ));
        }
        check_gl(&engine.gl, &format!("{name} steady renders"))?;
        let live = engine
            .read_target(&target)
            .ok_or("Could not read live target")?;
        let poster = engine
            .render(model, &camera, WIDTH, HEIGHT, None)
            .ok_or("Could not render frozen poster")?;
        if live.pixels != poster.pixels {
            failures.push(format!(
                "{name}: live/poster difference {:.4} per RGB channel",
                mean_difference(&live, &poster)
            ));
        }
        save_png(&directory.join(format!("{name}.png")), &live)?;
        let stats = pixel_stats(&live);
        if display == ModelDisplay::HiddenLine && !materials {
            let coverage = post_line_coverage(&live, model, &camera);
            writeln!(report, "visible vertical silhouette coverage={coverage:.3}").unwrap();
            if coverage < 0.9 {
                failures.push("Vertical silhouette strokes must stay continuous".into());
            }
        }
        if stats.nonopaque != 0 {
            failures.push(format!(
                "{name}: {} output pixels unexpectedly carry transparency",
                stats.nonopaque
            ));
        }
        if display == ModelDisplay::Arctic
            && (!white_roofs(&live, model, &camera)
                || stats.midtone < 2_000
                || stats.chromatic > 200)
        {
            failures.push("Arctic needs neutral white surfaces and visible shaded recesses".into());
        }
        if display == ModelDisplay::HiddenLine
            && !materials
            && (stats.dark < 300 || stats.white < 50_000 || stats.chromatic > 200)
        {
            failures.push("Hidden line needs neutral fills and visible dark feature lines".into());
        }
        times.sort_by(f64::total_cmp);
        writeln!(report, "{name}: white={} midtone={} dark={} chromatic={} nonopaque={} live/poster RGB difference={:.4}; GPU-finished median={:.3}ms p95={:.3}ms; live creates/readbacks=0",
            stats.white, stats.midtone, stats.dark, stats.chromatic,
            stats.nonopaque,
            mean_difference(&live, &poster), times[times.len()/2], times[times.len()-1]).unwrap();
        images.push(live);
        check_gl(&engine.gl, &format!("{name} poster/readback"))?;
    }
    for (off, on) in [(1, 2), (3, 4)] {
        let delta = mean_difference(&images[off], &images[on]);
        writeln!(
            report,
            "materials toggle pair {off}/{on}: RGB difference={delta:.3}"
        )
        .unwrap();
        if delta < 1.0 {
            failures.push("Materials toggle did not change imported surface appearance".into());
        }
    }
    large_arctic(engine, model, &mut target, directory, &mut report)?;
    depth_zoom_cases(engine, model, directory, &mut report)?;
    engine.free_target(target);
    std::fs::write(directory.join("report.txt"), &report).map_err(|e| e.to_string())?;
    if failures.is_empty() {
        Ok(report)
    } else {
        Err(format!("{}\n{report}", failures.join("\n")))
    }
}

fn depth_zoom_cases(
    engine: &ModelEngine,
    model: &GpuModel,
    directory: &Path,
    report: &mut String,
) -> Result<(), String> {
    let (center, radius) = bounds_sphere(model.bounds_min, model.bounds_max);
    let mut previous = None;
    for (name, distance) in [("depth-inside-bounds", 0.6), ("depth-close", 0.3)] {
        let camera = ModelCamera {
            target: center,
            distance: radius * distance,
            yaw: -0.95,
            pitch: 0.44,
            display: ModelDisplay::Depth,
            ..ModelCamera::default()
        };
        let image = engine
            .render(model, &camera, WIDTH, HEIGHT, None)
            .ok_or_else(|| format!("{name}: depth render failed"))?;
        let visible = image.pixels.iter().filter(|pixel| pixel.r() > 64).count();
        if visible < 500 {
            return Err(format!(
                "{name}: depth collapsed to black ({visible} visible pixels)"
            ));
        }
        if previous
            .as_ref()
            .is_some_and(|previous: &egui::ColorImage| previous.pixels == image.pixels)
        {
            return Err("Depth display did not change when the camera zoomed".into());
        }
        save_png(&directory.join(format!("{name}.png")), &image)?;
        writeln!(
            report,
            "{name}: {visible} depth pixels remain above quarter brightness"
        )
        .unwrap();
        previous = Some(image);
    }
    check_gl(&engine.gl, "depth zoom cases")
}

fn project_point(frame: &surface::FrameCamera, p: [f32; 3]) -> [f32; 2] {
    let v: [f32; 4] = std::array::from_fn(|i| {
        frame.mvp[i] * p[0] + frame.mvp[4 + i] * p[1] + frame.mvp[8 + i] * p[2] + frame.mvp[12 + i]
    });
    [
        (v[0] / v[3] * 0.5 + 0.5) * WIDTH as f32,
        (0.5 - v[1] / v[3] * 0.5) * HEIGHT as f32,
    ]
}

fn white_roofs(image: &egui::ColorImage, model: &GpuModel, camera: &ModelCamera) -> bool {
    let frame = surface::FrameCamera::new(model, camera, WIDTH as f32 / HEIGHT as f32, true);
    // Sample the model, so changing the ground tone cannot satisfy this check.
    fixture::WHITE_ROOFS.into_iter().all(|p| {
        let [x, y] = project_point(&frame, p).map(|v| v.round() as usize);
        let pixel = image.pixels[y * WIDTH as usize + x];
        pixel.r() > 235 && pixel.g() > 235 && pixel.b() > 235
    })
}

fn post_line_coverage(image: &egui::ColorImage, model: &GpuModel, camera: &ModelCamera) -> f32 {
    let frame = surface::FrameCamera::new(model, camera, WIDTH as f32 / HEIGHT as f32, false);
    let [a, b] = fixture::VISIBLE_POST.map(|p| project_point(&frame, p));
    let covered = (0..50)
        .filter(|i| {
            let t = *i as f32 / 49.0;
            let x = (a[0] + (b[0] - a[0]) * t).round() as i32;
            let y = (a[1] + (b[1] - a[1]) * t).round() as i32;
            (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let (x, y) = (
                        (x + dx).clamp(0, WIDTH as i32 - 1),
                        (y + dy).clamp(0, HEIGHT as i32 - 1),
                    );
                    image.pixels[y as usize * WIDTH as usize + x as usize].r() < 190
                })
            })
        })
        .count();
    covered as f32 / 50.0
}

fn large_arctic(
    engine: &ModelEngine,
    model: &GpuModel,
    target: &mut LiveTarget,
    directory: &Path,
    report: &mut String,
) -> Result<(), String> {
    let camera = ModelCamera {
        yaw: -0.95,
        pitch: 0.44,
        display: ModelDisplay::Arctic,
        ..Default::default()
    };
    let size = (1920, 1080, 1920.0 / 1080.0);
    if !engine.render_live(target, model, &camera, size, None) {
        return Err("Full-HD Arctic target failed".into());
    }
    unsafe {
        engine.gl.finish();
    }
    let before = counts(engine);
    let mut times = Vec::new();
    for _ in 0..12 {
        let start = Instant::now();
        if !engine.render_live(target, model, &camera, size, None) {
            return Err("Full-HD Arctic render failed".into());
        }
        unsafe {
            engine.gl.finish();
        }
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    if counts(engine) != before {
        return Err("Full-HD retained render allocated or read pixels".into());
    }
    times.sort_by(f64::total_cmp);
    let live = engine
        .read_target(target)
        .ok_or("Full-HD Arctic readback failed")?;
    let poster = engine
        .render(model, &camera, size.0, size.1, None)
        .ok_or("Full-HD poster failed")?;
    if live.pixels != poster.pixels {
        return Err("Full-HD live/poster mismatch".into());
    }
    save_png(&directory.join("arctic-full-hd.png"), &live)?;
    writeln!(report,"Arctic 1920x1080: median={:.3}ms p95={:.3}ms; live/poster exact; retained creates/readbacks=0", times[6],times[11]).unwrap();
    check_gl(&engine.gl, "full-HD Arctic")
}

fn run_checks(gl: Arc<glow::Context>, directory: &Path) -> Result<String, String> {
    let scene = fixture::write_scene(directory)?;
    let triangles: usize = scene.meshes.iter().map(|mesh| mesh.indices.len() / 3).sum();
    unsafe { while gl.get_error() != glow::NO_ERROR {} }
    let engine =
        ModelEngine::new(gl.clone()).ok_or("Production model shaders did not compile/link")?;
    check_gl(&gl, "shader construction")?;
    let admission_start = Instant::now();
    let model = engine
        .upload(&scene)
        .ok_or("Could not upload architectural scene/shadow map")?;
    let submission_ms = admission_start.elapsed().as_secs_f64() * 1000.0;
    unsafe {
        gl.finish();
    }
    let admission_ms = admission_start.elapsed().as_secs_f64() * 1000.0;
    if model.textures.len() != 1 {
        return Err("Repeated material images must share one GPU texture".into());
    }
    check_gl(&gl, "model and shadow upload")?;
    let device = unsafe {
        format!(
            "{} / {} / {}",
            gl.get_parameter_string(glow::VENDOR),
            gl.get_parameter_string(glow::RENDERER),
            gl.get_parameter_string(glow::VERSION)
        )
    };
    println!("Model render quality: {WIDTH}x{HEIGHT}, {triangles} triangles; {device}");
    std::fs::write(
        directory.join("device.txt"),
        format!(
            "{device}\ncold GPU admission submit={submission_ms:.3}ms, finished={admission_ms:.3}ms\nshared material GPU textures={}\n",
            model.textures.len()
        ),
    )
    .map_err(|e| e.to_string())?;
    let outcome = render_cases(&engine, &model, directory);
    engine.free(model);
    check_gl(&gl, "cleanup")?;
    outcome
}

#[test]
#[ignore = "opens a real OpenGL window; writes target/render-quality images and GPU evidence"]
fn model_render_quality_real_gl() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("target/render-quality");
    let outcome: Outcome = Arc::new(Mutex::new(None));
    let result = outcome.clone();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        vsync: false,
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 650.0])
            .with_title("Slate model render quality"),
        event_loop_builder: Some(Box::new(|builder| {
            #[cfg(windows)]
            {
                use winit::platform::windows::EventLoopBuilderExtWindows;
                builder.with_any_thread(true);
            }
            #[cfg(not(windows))]
            let _ = builder;
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Slate model render quality",
        options,
        Box::new(move |cc| {
            *result.lock().unwrap() = Some(
                cc.gl
                    .clone()
                    .ok_or_else(|| "eframe did not provide its Glow context".into())
                    .and_then(|gl| run_checks(gl, &directory)),
            );
            Ok(Box::new(QualityApp))
        }),
    )
    .expect("real-GL application must start and return");
    let outcome = outcome
        .lock()
        .unwrap()
        .take()
        .expect("GL checks must execute");
    println!(
        "{}",
        outcome
            .as_ref()
            .map_or_else(|error| error.as_str(), |report| report.as_str())
    );
    assert!(outcome.is_ok(), "{}", outcome.unwrap_err());
}
