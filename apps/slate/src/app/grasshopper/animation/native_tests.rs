use super::*;
use crate::app::tests::Harness;
use atlas_core::video::{VideoEvent, VideoPool};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

// Only inspect mailboxes created by this test process. The helper can replace
// its first mailbox with a recovery worker before issuing the first solve.
fn native_stage(workspace: &Path, prefix: &str, since: std::time::SystemTime) -> (String, bool) {
    let Some(root) = std::fs::read_dir(workspace)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with(prefix)
                && e.metadata()
                    .and_then(|m| m.created())
                    .is_ok_and(|t| t >= since)
        })
        .max_by_key(|e| e.file_name())
        .map(|e| e.path())
    else {
        return ("waiting for helper".into(), false);
    };
    let label = root.file_name().unwrap().to_string_lossy();
    let status = |name: &str| std::fs::read_to_string(root.join(name)).unwrap_or_default();
    if !root.join("ready").is_file() {
        return (
            format!(
                "{label}: {} {}",
                status("progress.txt"),
                status("error.txt")
            ),
            false,
        );
    }
    let latest = std::fs::read_dir(root.join("jobs"))
        .into_iter()
        .flatten()
        .flatten()
        .max_by_key(|e| e.file_name())
        .map(|e| e.path());
    let Some(job) = latest else {
        return (format!("{label}: ready"), false);
    };
    let issued = job.join("request.json").is_file();
    let solved = job.join("response.json").is_file();
    let preview = job.join("preview.3dm").is_file();
    (
        format!(
            "{label}/{}: request={issued} response={solved} preview={preview}",
            job.file_name().unwrap().to_string_lossy()
        ),
        issued,
    )
}

/// Exercises the same solve/render, H.264 publication, journal placement, and
/// playback-worker paths as the slider's Play button. The GHX contains a native
/// numeric slider that directly changes the size of a visible box.
#[test]
#[ignore = "requires installed Rhino 8, a native OpenGL context, and the packaged video worker"]
fn native_slider_exports_an_animated_mp4_and_reopens_its_workbook() {
    let movie = Arc::new(Mutex::new(None));
    let published = Arc::clone(&movie);
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_visible(false)
            .with_inner_size([240.0, 160.0]),
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    // The GL context is current immediately in the application creator. The
    // hidden test window doesn't need focus, desktop input, or a visible paint.
    eframe::run_native(
        "Slate slider export test",
        options,
        Box::new(move |cc| {
            let mut h = Harness::new("native-gh-slider-export");
            h.app.home_new_workspace();
            h.app.gl = Some(cc.gl.clone().expect("native GL renderer"));
            let src = Path::new(env!("CARGO_MANIFEST_DIR")).join(
                "../../design/agent-interaction-studio/native-implementation/examples/linked-grasshopper/01 Program.ghx"
            );
            let definition = h.base.join("Program.ghx");
            std::fs::copy(src, &definition).unwrap();
            let item = h.app.add_paths(&[definition])[0];
            h.app.place_items_on_board(&[item], egui::Pos2::ZERO);
            let node = *h.app.board_sel.iter().next().unwrap();
            let viewer = (h.app.tab().id, node);
            let workbook = h.base.join("Slider export.slate");
            h.app.save_doc_to(viewer.0, workbook.clone());
            let workspace = application_connector::headless_rhino_configuration().unwrap().workspace;
            let owner = format!("gh-{}-", std::process::id());
            let since = std::time::SystemTime::now();
            let started = Instant::now();
            let mut next_check = started;
            let mut phase_started = started;
            let mut last_progress = String::new();
            h.app.request_grasshopper(node, &h.ctx);
            let mut launched = false;
            let mut samples = BTreeSet::new();
            let output = loop {
                h.app.grasshopper_frame(&h.ctx);
                h.app.maintain_portal_capture(&h.ctx);
                let view = &h.app.grasshopper.views[&viewer];
                if !launched && view.ready_revision.is_none() && view.due.is_none()
                    && h.app.grasshopper.background.as_ref().is_none_or(|worker| worker.busy.is_none()) {
                    panic!("First Grasshopper preview stopped: {:?}; last progress: {last_progress}", view.status);
                }
                if !launched && view.ready_revision.is_some() {
                    let mut control = view.controls.iter().find(|c| c.name == "Base width").unwrap().clone();
                    let (min, max) = control.min.zip(control.max).unwrap();
                    control.step = Some((max - min) * 0.5);
                    h.app.start_grasshopper_animation(node, &control);
                    assert!(h.app.grasshopper_animation_active(), "slider action failed");
                    launched = true;
                }
                if let Some(run) = &h.app.grasshopper.animation {
                    if let Phase::Render { key, .. } = &run.phase {
                        samples.insert(key.clone());
                    }
                }
                let video = h.app.doc().scene.nodes.iter().find_map(|node| {
                    let NodeKind::Image(image) = &node.kind else { return None };
                    let item = h.app.doc().item(image.item)?;
                    item.path.extension()?.eq_ignore_ascii_case("mp4").then_some((node.id, item.id))
                });
                if let Some(value) = video { break value; }
                if launched && !h.app.grasshopper_animation_active() {
                    if let Some((message, _)) = h.app.toasts.last().filter(|(message, _)| {
                        message.ends_with("No video was added.")
                            || message == "Grasshopper animation stopped before the video completed."
                    }) {
                        panic!("Grasshopper export ended without publishing a video: {message}");
                    }
                }
                let now = Instant::now();
                if now >= next_check {
                    next_check = now + Duration::from_secs(1);
                    let (native, job_issued) = native_stage(&workspace, &owner, since);
                    let idle_phase = if launched { "publishing" } else { "first preview" };
                    let (sample, phase) = h.app.grasshopper.animation.as_ref().map_or((0,idle_phase), |run| {
                        (run.index + 1, match run.phase {
                            Phase::Solve { .. } => "solve", Phase::Render { .. } => "render", Phase::Saving { .. } => "saving"
                        })
                    });
                    let view = &h.app.grasshopper.views[&viewer];
                    let toast = h.app.toasts.last().map(|(text, _)| text.as_str());
                    let progress = format!("sample {sample} {phase}, generation {}, status {:?}, toast {toast:?}, {native}", view.generation, view.status);
                    if progress != last_progress {
                        eprintln!("Grasshopper export +{:.1}s: {progress}", started.elapsed().as_secs_f64());
                        phase_started = now;
                        last_progress = progress;
                    }
                    // The native worker allows 600 s for recovery startup and
                    // 180 s per solve. Give each independent stage its budget
                    // plus scheduling time, and cap the three-sample test overall.
                    let budget = if matches!(phase,"render"|"saving"|"publishing") { PUBLICATION_TIMEOUT }
                        else if job_issued { Duration::from_secs(180) } else { Duration::from_secs(600) };
                    assert!(now.duration_since(phase_started) < budget + Duration::from_secs(30)
                        && now.duration_since(started) < Duration::from_secs(30*60),
                        "Grasshopper export stalled after {:?} in its current stage: {last_progress}", now.duration_since(phase_started));
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(samples.len(), 3, "every sampled solver geometry must be distinct");
            h.app.save_doc_to(viewer.0, workbook.clone());
            let reopened = slate_doc::SlateDoc::load_from(&workbook).unwrap();
            let NodeKind::Image(card) = &reopened.scene.node(output.0).unwrap().kind else { panic!() };
            assert_eq!(card.item, output.1);
            let path = slate_doc::scene::resolve_source(Some(&workbook), &reopened.item(card.item).unwrap().path.to_string_lossy());
            assert!(std::fs::read(&path).unwrap().windows(4).any(|v| v == b"avc1"));
            eprintln!("Reopened {} with {}", workbook.display(), path.display());
            *published.lock().unwrap() = Some(path);
            struct Close;
            impl eframe::App for Close {
                fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            // Posting the close command here also covers hidden viewports that
            // don't receive another paint after creation.
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            Ok(Box::new(Close))
        }),
    ).unwrap();

    let worker = VideoPool::spawn();
    worker.request_strip(1, 1, movie.lock().unwrap().take().unwrap(), 0.0, None);
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut distinct_frames = BTreeSet::new();
    let mut duration = 0.0;
    while Instant::now() < deadline && (distinct_frames.len() < 2 || duration == 0.0) {
        match worker.try_recv() {
            Some(VideoEvent::Probed {
                duration: value, ..
            }) => duration = value,
            Some(VideoEvent::Strip { frame, .. }) => {
                distinct_frames.insert(frame.rgba);
            }
            Some(VideoEvent::Failed { .. }) => panic!("packaged decoder rejected the saved movie"),
            _ => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    assert!(
        (duration - 0.25_f32).abs() < 0.04,
        "unexpected duration {duration}"
    );
    assert!(
        distinct_frames.len() >= 2,
        "expected visibly different decoded animation frames"
    );
}
