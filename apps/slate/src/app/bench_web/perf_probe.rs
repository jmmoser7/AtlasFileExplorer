//! Generated-only CPU evidence at the existing fake WebHost seam; never a browser.

use super::*;
use serde::Serialize;
use serde_json::{json, Value};
use slate_doc::scene::{Corner, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke, WorldRect};
use std::path::{Path, PathBuf};

mod reporting;
use reporting::{artifact, checkpoint, percentile, FailureReport};

const FRAMES: usize = 20;
const WARMUP: usize = 12;
const PACE: Duration = Duration::from_micros(16_667);
const DEADLINE: Duration = Duration::from_secs(110);

#[derive(Serialize)]
struct Sample {
    frame: usize,
    app_ms: f64,
    tessellation_ms: f64,
    cpu_ms: f64,
    mesh_primitives: usize,
    vertices: usize,
    indices: usize,
    texture_payload_bytes: usize,
    texture_set_requests: usize,
    texture_free_requests: usize,
    pixels_per_point: f32,
    captures: usize,
    poster_captures: usize,
    capture_requests: usize,
    host_opens: usize,
    host_closes: usize,
    admitted: usize,
    live: usize,
    stills: usize,
    camera: [f32; 3],
    focused: Option<u64>,
    target_visible: bool,
    target_live: bool,
    target_has_picture: bool,
    target_frame: [f32; 4],
    canvas_rect: [f32; 4],
    injected: Vec<String>,
    delivered: Vec<Value>,
}
fn camera(b: &Bench) -> [f32; 3] {
    let cam = b.app.tab().cam;
    [cam.offset.x, cam.offset.y, cam.z]
}
fn target_frame(b: &Bench) -> ERect {
    let node = b.app.doc().scene.node(b.ids[0]).unwrap();
    b.app.board_xf().rect_w2s(node.rect)
}
fn trace((id, event): (NodeId, WebInput)) -> Value {
    let data = match event {
        WebInput::Move { x, y, buttons } => json!({"kind":"move","x":x,"y":y,"buttons":buttons}),
        WebInput::Down { x, y, button } => json!({"kind":"down","x":x,"y":y,"button":button}),
        WebInput::Up { x, y, button } => json!({"kind":"up","x":x,"y":y,"button":button}),
        WebInput::Wheel {
            x,
            y,
            delta,
            horizontal,
        } => {
            json!({"kind":"wheel","x":x,"y":y,"delta":delta,"horizontal":horizontal})
        }
        WebInput::Leave => json!({"kind":"leave"}),
        WebInput::Key {
            key,
            pressed,
            modifiers,
        } => {
            json!({"kind":"key","key":format!("{key:?}"),"pressed":pressed,"modifiers":format!("{modifiers:?}")})
        }
        WebInput::Text(text) => json!({"kind":"text","text":text}),
    };
    json!({"portal":id.0,"event":data})
}

impl Bench {
    fn cpu_sample(&mut self, clock: Instant, frame: usize, events: Vec<egui::Event>) -> Sample {
        let before = {
            let log = self.host.0.borrow();
            (
                log.frames_taken,
                log.posters_taken,
                log.take_calls,
                log.opens,
                log.closes,
            )
        };
        let injected = events.iter().map(|event| format!("{event:?}")).collect();
        let input = egui::RawInput {
            screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1920.0, 1080.0))),
            time: Some(clock.elapsed().as_secs_f64()),
            events,
            focused: true,
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let app = &mut self.app;
        let start = Instant::now();
        let output = ctx.run(input, |ctx| app.update_app(ctx));
        let app_ms = start.elapsed().as_secs_f64() * 1000.0;
        let texture_payload_bytes = output
            .textures_delta
            .set
            .iter()
            .map(|(_, delta)| {
                let [w, h] = delta.image.size();
                w * h * 4
            })
            .sum();
        let texture_set_requests = output.textures_delta.set.len();
        let texture_free_requests = output.textures_delta.free.len();
        let tessellate = Instant::now();
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let tessellation_ms = tessellate.elapsed().as_secs_f64() * 1000.0;
        let (mut mesh_primitives, mut vertices, mut indices) = (0, 0, 0);
        for primitive in &primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive {
                mesh_primitives += 1;
                vertices += mesh.vertices.len();
                indices += mesh.indices.len();
            }
        }
        let rect = target_frame(self);
        let canvas = self.app.canvas_rect;
        let target_visible =
            self.app.canvas_rect.contains(rect.center()) && self.app.canvas_rect.intersects(rect);
        let mut log = self.host.0.borrow_mut();
        let delivered = std::mem::take(&mut log.inputs)
            .into_iter()
            .map(trace)
            .collect();
        Sample {
            frame,
            app_ms,
            tessellation_ms,
            cpu_ms: app_ms + tessellation_ms,
            mesh_primitives,
            vertices,
            indices,
            texture_payload_bytes,
            texture_set_requests,
            texture_free_requests,
            pixels_per_point: output.pixels_per_point,
            captures: log.frames_taken - before.0,
            poster_captures: log.posters_taken - before.1,
            capture_requests: log.take_calls - before.2,
            host_opens: log.opens - before.3,
            host_closes: log.closes - before.4,
            admitted: log.admitted.len(),
            live: self.app.web.live_count(),
            stills: self.stills(),
            camera: camera(self),
            focused: self.app.web.focused.map(|id| id.0),
            target_visible,
            target_live: self.app.web.is_live(self.ids[0]),
            target_has_picture: self.app.web.has_still(self.ids[0]),
            target_frame: [rect.left(), rect.top(), rect.width(), rect.height()],
            canvas_rect: [canvas.left(), canvas.top(), canvas.width(), canvas.height()],
            injected,
            delivered,
        }
    }
}
fn seed_shapes(b: &mut Bench, count: usize) {
    // Small shapes occupy the existing portal grid's gutters, never the page.
    let nodes = (0..count)
        .map(|i| {
            let column = i % (COLS - 1);
            let lane = (i / (COLS - 1)) % 3;
            let row = i / ((COLS - 1) * 3);
            let rect = WorldRect::new(
                40.0 + CARD_W + 2.0 + column as f32 * (CARD_W + GAP) + lane as f32 * 7.0,
                124.0 + row as f32 * 9.5,
                5.0,
                5.0,
            );
            b.app.doc_mut().scene.build_node(
                rect,
                NodeKind::Shape(ShapeNode {
                    shape: ShapeKind::Rect,
                    fill: Some(Rgba([50, 130, 160, 255])),
                    stroke: Stroke::none(),
                    corner: Corner::Square,
                    sides: slate_doc::scene::default_regular_sides(),
                    phase_deg: 0.0,
                    flip: false,
                    path: None,
                    text: None,
                }),
            )
        })
        .collect();
    b.app.add_nodes(nodes);
    assert_eq!(b.app.doc().scene.nodes.len(), PORTALS + count);
}
fn pace(start: Instant) {
    if let Some(rest) = PACE.checked_sub(start.elapsed()) {
        std::thread::sleep(rest);
    }
}
fn assert_delivery(sample: &Sample, name: &str) {
    let wheel = name == "focused_wheel";
    let expected: &[&str] = if wheel {
        &["wheel"]
    } else if sample.frame == 0 {
        &["move", "down"]
    } else if sample.frame + 1 == FRAMES {
        &["move", "up"]
    } else {
        &["move"]
    };
    let kinds: Vec<_> = sample
        .delivered
        .iter()
        .map(|e| e["event"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, expected.to_vec(), "page event order/count changed");
    let dx = if wheel {
        0.0
    } else {
        (sample.frame + 1) as f64 * 2.0
    };
    let dy = if wheel { 0.0 } else { 3.0 };
    for input in &sample.delivered {
        let event = &input["event"];
        for (key, size, offset, span) in [
            ("x", PAGE_W, dx, sample.target_frame[2]),
            ("y", PAGE_H, dy, sample.target_frame[3]),
        ] {
            let expected = size as f64 * 0.5 + offset * size as f64 / span as f64;
            assert!(
                (event[key].as_f64().unwrap() - expected).abs() < 0.02,
                "wrong page CSS coordinate"
            );
        }
        if wheel {
            assert_eq!(
                event["delta"],
                if sample.frame.is_multiple_of(2) {
                    -40.0
                } else {
                    40.0
                }
            );
            assert_eq!(event["horizontal"], false);
        } else if event["kind"] == "move" {
            assert_eq!(
                event["buttons"],
                if sample.frame + 1 == FRAMES { 0 } else { 1 }
            );
        } else {
            assert_eq!(event["button"], 0);
        }
    }
}
fn scenario(b: &mut Bench, clock: Instant, name: &str, path: &Path) -> Value {
    let mut warmup = Vec::with_capacity(WARMUP);
    let mut samples = Vec::with_capacity(FRAMES);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        b.app.web_blur();
        let focused = name.starts_with("focused_");
        if name == "fit_idle" {
            b.app.fit_board();
        } else {
            b.focus_card(0, 2.0);
        }
        if focused {
            b.app.web_focus(b.ids[0]);
        }
        for frame in 0..WARMUP {
            checkpoint(clock, path, name, &warmup, &[]);
            let start = Instant::now();
            let point = target_frame(b).center();
            let events = if focused {
                vec![egui::Event::PointerMoved(point)]
            } else {
                vec![egui::Event::PointerGone]
            };
            warmup.push(b.cpu_sample(clock, frame, events));
            checkpoint(clock, path, name, &warmup, &[]);
            pace(start);
        }
        let id = b.ids[0];
        if focused {
            assert_eq!(b.app.web.focused, Some(id), "page never held focus");
            assert_eq!(
                b.app.contents_focused(),
                Some(id),
                "shared portal focus disagrees"
            );
            assert!(b.app.web.is_live(id), "focused page was not admitted");
            assert!(b.host.0.borrow().admitted.contains(&id));
            assert!(
                b.app.web.has_still(id),
                "focused page has no painted picture"
            );
        }
        b.host.reset_counts();
        let before = camera(b);
        let anchor = target_frame(b).center();
        for frame in 0..FRAMES {
            checkpoint(clock, path, name, &warmup, &samples);
            let start = Instant::now();
            let mut events = Vec::new();
            match name {
                "focused_wheel" => {
                    events.push(egui::Event::PointerMoved(anchor));
                    events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: EVec2::new(0.0, if frame.is_multiple_of(2) { -40.0 } else { 40.0 }),
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                "focused_pointer" | "camera_pan" => {
                    let point = anchor + EVec2::new((frame + 1) as f32 * 2.0, 3.0);
                    events.push(egui::Event::PointerMoved(point));
                    if frame == 0 || frame + 1 == FRAMES {
                        events.push(egui::Event::PointerButton {
                            pos: point,
                            button: if focused {
                                egui::PointerButton::Primary
                            } else {
                                egui::PointerButton::Middle
                            },
                            pressed: frame == 0,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                _ => {}
            }
            samples.push(b.cpu_sample(clock, frame, events));
            let sample = samples.last().unwrap();
            assert!(sample.target_visible, "target page is offscreen in {name}");
            if focused {
                assert_eq!(sample.focused, Some(id.0), "lost page focus in {name}");
                assert!(sample.target_live && sample.target_has_picture);
                assert_eq!(sample.camera, before, "page input moved the board camera");
                assert!(sample.delivered.iter().all(|input| input["portal"] == id.0));
                if matches!(name, "focused_wheel" | "focused_pointer") {
                    assert_delivery(sample, name);
                }
            }
            checkpoint(clock, path, name, &warmup, &samples);
            pace(start);
        }
        if name == "focused_idle" {
            assert!(
                samples.iter().all(|sample| sample.delivered.is_empty()),
                "parked pointer synthesized page input"
            );
        } else if name == "camera_pan" {
            assert_ne!(
                camera(b),
                before,
                "middle drag never moved the actual board camera"
            );
            assert!(b.app.web.focused.is_none());
            assert!(
                samples.iter().all(|sample| sample.delivered.is_empty()),
                "camera gesture leaked into a page"
            );
        }
        let summary = json!({"app_ms":percentile(&samples, |s| s.app_ms),
        "tessellation_ms":percentile(&samples, |s| s.tessellation_ms),
        "cpu_ms":percentile(&samples, |s| s.cpu_ms),
        "texture_payload_bytes":samples.iter().map(|s| s.texture_payload_bytes).sum::<usize>(),
        "capture_requests":samples.iter().map(|s| s.capture_requests).sum::<usize>(),
        "captures":samples.iter().map(|s| s.captures).sum::<usize>(),
        "host_opens":samples.iter().map(|s| s.host_opens).sum::<usize>(),
        "host_closes":samples.iter().map(|s| s.host_closes).sum::<usize>(),
        "delivered_inputs":samples.iter().map(|s| s.delivered.len()).sum::<usize>()});
        json!({"scenario":name,"warmup":warmup,"samples":samples,"summary":summary})
    }));
    match outcome {
        Ok(evidence) => evidence,
        Err(error) => {
            reporting::scenario_failure(path, name, &warmup, &samples);
            std::panic::resume_unwind(error)
        }
    }
}

#[test]
#[ignore = "bounded generated fake-host CPU diagnostic; no browser or GPU"]
fn bench_web_large_canvas_cpu() {
    let clock = Instant::now();
    let label = std::env::var("SLATE_WEB_PERF_LABEL").unwrap_or_else(|_| "local".into());
    let path = artifact(&label);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("existing evidence must be retained; choose a new label");
    let mut rows = Vec::new();
    let mut report = json!({"schema":1,"label":label,"status":"running",
        "os":std::env::consts::OS,"arch":std::env::consts::ARCH,
        "build":if cfg!(debug_assertions) { "dev" } else { "release" },
        "percentiles":"nearest rank ceil(q*N)-1; CPU intervals exclude pacing",
        "limits":{"main_frames":FRAMES,"warmup_frames":WARMUP,"cooperative_deadline_seconds":110,"pace_us":16667},
        "fixture":{"portals":PORTALS,"frame_pixels":[PAGE_W,PAGE_H],"screen_points":[1920,1080],"extra_shapes":[0,1000,5000]},
        "limitations":["CPU app ctx.run plus egui tessellation only; no JS, network, provider, native WebView, GPU or delivered-display latency.",
            "Fake host allocates patterned 1280x720 ColorImages using the existing Bench owner.",
            "Texture payload bytes count 4 bytes/texel requested in egui texture deltas; not actual GPU uploads.",
            "Warmup and CPU/input/camera traces retained; wall pacing and diagnostics are excluded from app+tessellation CPU timing.",
            "20 main samples give descriptive tails, not reliable rare-event estimates.",
            "Rust deadline is checked between frames; the runner supplies the hard command cap. A hung frame cannot checkpoint its unfinished sample.",
            "Only generated gutter rectangles are added; page contents are synthetic and do not execute a dashboard."],
        "rows":[]});
    std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let _failure_report = FailureReport(path.clone());
    for extra_nodes in [0, 1000, 5000] {
        checkpoint(clock, &path, "fixture_setup", &[], &[]);
        let setup = Instant::now();
        let mut bench = Bench::new();
        assert!(
            !bench.app.web.allows_all_websites(),
            "global web permission must remain off"
        );
        assert!(bench.app.ai.config.workspace_dir.is_none());
        bench
            .app
            .set_board_tool(super::super::board::BoardTool::Select);
        seed_shapes(&mut bench, extra_nodes);
        let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
        for name in [
            "fit_idle",
            "focused_idle",
            "focused_wheel",
            "focused_pointer",
            "camera_pan",
        ] {
            report["active"] = json!({"extra_nodes":extra_nodes,"scenario":name});
            std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
            let evidence = scenario(&mut bench, clock, name, &path);
            rows.push(
                json!({"extra_nodes":extra_nodes,"total_nodes":PORTALS+extra_nodes,
                "fixture_setup_ms":setup_ms,"evidence":evidence}),
            );
            report["rows"] = json!(rows);
            report["elapsed_ms"] = json!(clock.elapsed().as_secs_f64() * 1000.0);
            std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
    }
    assert_eq!(rows.len(), 15);
    assert!(
        clock.elapsed() < Duration::from_secs(120),
        "runtime bound exceeded"
    );
    report["status"] = json!("completed");
    report["elapsed_ms"] = json!(clock.elapsed().as_secs_f64() * 1000.0);
    std::fs::write(&path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("web CPU diagnostic: {}", path.display());
}
