//! Real native frame loop, normal egui gestures, and composed-frame evidence.
//! The diagnostic marker exists only in this ignored test's owned window.
mod gestures;
mod report;
use report::process_cpu_ms;
mod image_fixture;
mod paint_witness;
use super::window_capture::{Observed, WindowCapture};
use crate::app::{bench_stress, tests::Harness, SlateApp};
use eframe::egui::{self, Pos2, Vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(serde::Serialize)]
struct Sample {
    phase: &'static str,
    input_to_capture_ms: f64,
    queue_to_hook_ms: f64,
    app_ms: f64,
    response_changed: bool,
    response_pixels_changed: bool,
    capture_clock_100ns: i64,
    request_clock_100ns: Option<i64>,
    observer_clock_100ns: Option<i64>,
    request_to_compositor_ms: Option<f64>,
    observer_after_compositor_ms: Option<f64>,
    clock_issue: Option<&'static str>,
    marker_available_clock_100ns: Option<i64>,
    input_to_marker_available_ms: Option<f64>,
    roi_readback_and_hash_ms: Option<f64>,
    marker_clock_issue: Option<&'static str>,
}

struct NativeBench {
    h: Harness,
    capture: Option<WindowCapture>,
    hwnd: usize,
    done: bool,
    driver: gestures::Driver,
    input_queued: bool,
    queue_to_hook_ms: f64,
    request_clock_100ns: Option<i64>,
    warm_until: Instant,
    last_update: Option<Instant>,
    fit: Option<slate_doc::WorldRect>,
    step: usize,
    scene: String,
    settle_until: Option<Instant>,
    wheel_settled: bool,
    sequence: u8,
    painted_sequence: u8,
    waiting: Option<(Instant, &'static str, f64, bool)>,
    before: (Vec2, f32, Vec<slate_doc::NodeId>),
    model: Option<slate_doc::NodeId>,
    model_grid: usize,
    before_model: Option<slate_doc::scene::ModelCamera>,
    samples: Vec<Sample>,
    profile_sections: bool,
    section_laps: Vec<(
        u64,
        Option<&'static str>,
        Option<f32>,
        Vec<(&'static str, f32)>,
    )>,
    idle: Option<(Instant, usize, Option<f64>)>,
    output: Arc<Mutex<Option<serde_json::Value>>>,
    failure: Option<String>,
}

impl NativeBench {
    fn new(
        cc: &eframe::CreationContext<'_>,
        count: usize,
        output: Arc<Mutex<Option<serde_json::Value>>>,
    ) -> Self {
        let evidence = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/performance-recovery/20261031");
        let assets = evidence.join("native-assets");
        let base = std::env::temp_dir().join(format!(
            "slate-latency-{}-{}",
            std::process::id(),
            crate::app::tests::now_nanos()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let scene = std::env::var("SLATE_LATENCY_SCENE").unwrap_or_else(|_| "image-board".into());
        assert!(matches!(
            scene.as_str(),
            "image-board" | "mixed" | "live-model"
        ));
        let model_grid = std::env::var("SLATE_LATENCY_MODEL_GRID")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(128usize);
        assert!(matches!(model_grid, 128 | 512));
        let (images, mut models) = bench_stress::synthetic_assets(&assets);
        if scene == "live-model" && model_grid == 512 {
            models.push(bench_stress::synthetic_grid(&assets, model_grid));
        }
        let mut app = SlateApp::with_ctx(&cc.egui_ctx, None);
        app.gl = cc.gl.clone();
        let label = std::env::var("SLATE_LATENCY_LABEL").unwrap_or_else(|_| "baseline".into());
        app.session_log = atlas_core::session_log::SessionLog::persist_at(
            "slate-probe",
            evidence.join(format!("native-log-{label}-{count}-{scene}")),
        );
        let workspace = base.join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        app.ai.config.workspace_dir = Some(workspace);
        app.agents.hold_catalog_probes_for_test();
        app.legacy_input_projection_for_test(std::env::var_os("SLATE_LATENCY_EAGER").is_some());
        crate::app::board_path::legacy_mesh_colors_for_test(
            std::env::var_os("SLATE_MESH_LEGACY_COLORS").is_some(),
        );
        app.kits = crate::app::kits::KitState::builtin_only();
        app.leave_home();
        app.ensure_work_tab();
        let images = app.add_paths(&images);
        let models = app.add_paths(&models);
        let mut h = Harness {
            ctx: cc.egui_ctx.clone(),
            app,
            base,
        };
        let mut model = None;
        let bounds = if scene == "live-model" {
            let index = if model_grid == 512 { 2 } else { 1 };
            h.app
                .place_items_on_board(&models[index..=index], Pos2::ZERO);
            let id = h.app.doc().scene.nodes.last().unwrap().id;
            h.app.unlock_model(id);
            model = Some(id);
            let r = h.app.doc().scene.node(id).unwrap().rect;
            slate_doc::WorldRect::new(r.x - r.w * 0.5, r.y - r.h * 0.5, r.w * 2.0, r.h * 2.0)
        } else if scene == "image-board" {
            image_fixture::seed(&mut h, count, &images)
        } else {
            bench_stress::seed(&mut h, count, &images, &models)
        };
        h.app.zoom_to_rect(bounds);
        let hwnd = match cc
            .window_handle()
            .expect("native probe has a window")
            .as_raw()
        {
            RawWindowHandle::Win32(handle) => handle.hwnd.get() as usize,
            _ => panic!("Windows-only native probe"),
        };
        crate::app::board::paint_probe::enable(true);
        Self {
            h,
            capture: None,
            hwnd,
            done: false,
            driver: gestures::Driver::new(cc.egui_ctx.clone()),
            input_queued: false,
            queue_to_hook_ms: 0.0,
            request_clock_100ns: None,
            warm_until: Instant::now() + Duration::from_secs(8),
            last_update: None,
            fit: Some(bounds),
            step: 0,
            scene,
            settle_until: None,
            wheel_settled: false,
            sequence: 0,
            painted_sequence: 0,
            waiting: None,
            before: (Vec2::ZERO, 0.0, Vec::new()),
            model,
            model_grid,
            before_model: None,
            samples: Vec::new(),
            profile_sections: std::env::var_os("SLATE_NATIVE_SECTION_TIMINGS").is_some(),
            section_laps: Vec::new(),
            idle: None,
            output,
            failure: None,
        }
    }

    fn selection(&self) -> Vec<slate_doc::NodeId> {
        self.h.app.board_sel.iter().copied().collect()
    }

    fn collect(&mut self) {
        let updates: Vec<Observed> = self.capture.as_ref().unwrap().received.try_iter().collect();
        for observed in updates {
            if observed.sequence == self.sequence {
                if let Some((_, phase, app_ms, response_changed)) = self.waiting.take() {
                    self.samples.push(Sample {
                        phase,
                        app_ms,
                        response_changed,
                        response_pixels_changed: observed.pixels_changed,
                        input_to_capture_ms: observed.input_to_capture_ms,
                        queue_to_hook_ms: self.queue_to_hook_ms,
                        capture_clock_100ns: observed.capture_clock_100ns,
                        request_clock_100ns: observed.request_clock_100ns,
                        observer_clock_100ns: observed.observer_clock_100ns,
                        request_to_compositor_ms: observed.request_to_compositor_ms,
                        observer_after_compositor_ms: observed.observer_after_compositor_ms,
                        clock_issue: observed.clock_issue,
                        marker_available_clock_100ns: observed.marker_available_clock_100ns,
                        input_to_marker_available_ms: observed.input_to_marker_available_ms,
                        roi_readback_and_hash_ms: observed.roi_readback_and_hash_ms,
                        marker_clock_issue: observed.marker_clock_issue,
                    });
                }
            }
        }
    }
}

impl eframe::App for NativeBench {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.capture.is_none() {
            return;
        }
        self.collect();
        if let Some(packet) = self.driver.take() {
            self.input_queued = false;
            self.queue_to_hook_ms = packet.issued.elapsed().as_secs_f64() * 1000.0;
            self.request_clock_100ns = packet.issued_clock_100ns;
            self.before_model = self
                .model
                .and_then(|id| self.h.app.model3d.live.get(&id))
                .map(|v| v.cam);
            self.before = (
                self.h.app.tab().cam.offset,
                self.h.app.tab().cam.z,
                self.selection(),
            );
            raw.modifiers = packet.modifiers;
            raw.events.extend(packet.events);
            if let Some(phase) = packet.phase {
                self.sequence += 1;
                self.waiting = Some((packet.issued, phase, 0.0, false));
            }
            return;
        }
        if self.input_queued
            || Instant::now() < self.warm_until
            || self.waiting.is_some()
            || self.idle.is_some()
            || self.failure.is_some()
        {
            return;
        }
        if self.step == 82 && !self.wheel_settled {
            self.wheel_settled = true;
            self.settle_until = Some(Instant::now() + Duration::from_millis(650));
        }
        if self.settle_until.is_some_and(|at| Instant::now() < at) {
            return;
        }
        self.settle_until = None;
        let Some(job) = gestures::input(&self.h.app, self.step, self.model.is_some()) else {
            self.idle = Some((Instant::now(), 0, process_cpu_ms()));
            return;
        };
        self.driver.schedule(job);
        self.input_queued = true;
        self.step += 1;
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.done {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        // eframe shows the window after the first paint. Observe only once
        // the owned HWND has actually painted, not during app construction.
        if self.capture.is_none() && ctx.cumulative_pass_nr() >= 2 {
            self.capture = Some(
                WindowCapture::start(self.hwnd, ctx.clone()).expect("compositor observer starts"),
            );
        }
        let started = Instant::now();
        if self.profile_sections {
            crate::app::board::brush_prof::begin();
        }
        let interval = self
            .last_update
            .replace(started)
            .map_or(0.0, |at| started.duration_since(at).as_secs_f32() * 1000.0);
        let _attach = self.h.app.session_log.attach();
        self.h
            .app
            .session_log
            .set_snapshot(atlas_core::session_log::Snapshot {
                entries: self.h.app.doc().items.len() as u32,
                nodes: self.h.app.doc().scene.nodes.len() as u32,
                selected: self.h.app.board_sel.len() as u32,
                view: "board",
                tool: "Select",
                ..Default::default()
            });
        self.h.app.model3d.register_live_textures(frame);
        crate::app::board::paint_probe::reset();
        self.h.app.update_app(ctx);
        if let Some(bounds) = self.fit.take() {
            self.h.app.zoom_to_rect(bounds);
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        if self.profile_sections {
            self.section_laps.push((
                ctx.cumulative_pass_nr(),
                self.waiting.as_ref().map(|(_, phase, _, _)| *phase),
                frame.info().cpu_usage.map(|seconds| seconds * 1000.0),
                crate::app::board::brush_prof::take(),
            ));
        }
        self.h.app.session_log.end_frame(
            started.elapsed(),
            interval,
            atlas_core::session_log::FrameWake {
                had_input: self.waiting.is_some(),
                eager: false,
            },
        );
        let changed = match self.waiting.as_ref().map(|(_, phase, _, _)| *phase) {
            Some("pan") if self.model.is_some() => {
                self.before_model
                    != self
                        .model
                        .and_then(|id| self.h.app.model3d.live.get(&id))
                        .map(|v| v.cam)
            }
            Some("pan") => self.before.0 != self.h.app.tab().cam.offset,
            Some("zoom") if self.model.is_some() => {
                self.before_model
                    != self
                        .model
                        .and_then(|id| self.h.app.model3d.live.get(&id))
                        .map(|v| v.cam)
            }
            Some("zoom") => self.before.1 != self.h.app.tab().cam.z,
            Some("selection") => self.before.2 != self.selection(),
            _ => false,
        };
        if let Some((start, _, app_ms, response_changed)) = &mut self.waiting {
            if *app_ms == 0.0 {
                *app_ms = ms;
                *response_changed = changed;
            }
            if start.elapsed() > Duration::from_secs(15) {
                self.failure = Some("composed marker did not arrive within15seconds".into());
            }
        }
        self.arm_painted_response();
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("owned latency marker"),
        ))
        .rect_filled(
            egui::Rect::from_min_size(Pos2::ZERO, Vec2::splat(32.0)),
            0.0,
            egui::Color32::from_rgb(self.painted_sequence, 53, 193),
        );
        if self.failure.is_some() {
            self.finish(ctx, 0);
            return;
        }
        if let Some((at, frames, _)) = &mut self.idle {
            *frames += 1;
            if at.elapsed() >= Duration::from_secs(3) {
                let count = *frames;
                self.finish(ctx, count);
            } else {
                ctx.request_repaint_after(Duration::from_secs(3).saturating_sub(at.elapsed()));
            }
        } else if Instant::now() < self.warm_until {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else if let Some(at) = self.settle_until {
            ctx.request_repaint_after(at.saturating_duration_since(Instant::now()));
        } else if self.input_queued {
            // The independent producer will wake the actual native event loop.
            ctx.request_repaint_after(Duration::from_secs(15));
        } else if self.waiting.is_none() {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_secs(15));
        }
    }
}

#[test]
#[ignore = "opens an owned native synthetic board and observes composed frames; no user-window input"]
fn native_canvas_input_to_composed_frame() {
    let nodes = std::env::var("SLATE_LATENCY_NODES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(335usize);
    let scene = std::env::var("SLATE_LATENCY_SCENE").unwrap_or_else(|_| "image-board".into());
    assert!(if scene == "live-model" {
        nodes == 1
    } else {
        (20..=5000).contains(&nodes)
    });
    let label = std::env::var("SLATE_LATENCY_LABEL").unwrap_or_else(|_| "baseline".into());
    assert!(label
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')));
    let output = Arc::new(Mutex::new(None));
    let sink = output.clone();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        vsync: true,
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_decorations(false)
            .with_title("Slate performance owned probe — synthetic"),
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };
    eframe::run_native(
        "Slate performance owned probe",
        options,
        Box::new(move |cc| Ok(Box::new(NativeBench::new(cc, nodes, sink)))),
    )
    .unwrap();
    let report = output
        .lock()
        .unwrap()
        .take()
        .expect("native benchmark writes its report before closing");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/performance-recovery/20261031/native-{label}-{nodes}.json"
    ));
    std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!(
        "native latency summary={} report={}",
        report["summary"],
        path.display()
    );
    assert!(report["failure"].is_null(), "{report}");
    for phase in if scene == "live-model" {
        &["pan", "zoom"][..]
    } else {
        &["pan", "zoom", "selection"][..]
    } {
        assert!(
            report["samples"]
                .as_array()
                .unwrap()
                .iter()
                .any(|sample| sample["phase"] == *phase
                    && sample["response_changed"] == true
                    && sample["response_pixels_changed"] == true),
            "{phase} needs independently changed composed canvas pixels"
        );
    }

    let samples = report["samples"].as_array().unwrap();
    assert_eq!(
        samples.len(),
        if scene == "live-model" { 80 } else { 120 },
        "each gesture frame was independently observed"
    );
    assert!(
        samples
            .iter()
            .any(|s| s["phase"] == "pan" && s["response_changed"] == true),
        "normal middle-drag input moved the camera"
    );
    assert!(
        samples
            .iter()
            .any(|s| s["phase"] == "zoom" && s["response_changed"] == true),
        "normal wheel input changed zoom"
    );
    if scene != "live-model" {
        assert!(
            samples
                .iter()
                .any(|s| s["phase"] == "selection" && s["response_changed"] == true),
            "normal clicks changed selection"
        );
    } else {
        assert_eq!(
            report["model_live_after"], true,
            "the actual live 3D texture remains on screen"
        );
    }
}
