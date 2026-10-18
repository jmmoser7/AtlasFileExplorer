//! Production-painter visual fixture, separate from interaction acceptance.
//! `cargo run --release -p atlas-shell --example agent_presence -- --capture <prefix>`
//! captures both themes and two book-page phases, then closes the window.
//! `--bench` instead prints headless CPU evidence; it opens no native window.
//! `--conversation` opens the actual production conversation components with a
//! clearly labeled local fixture backend for interactive native UI acceptance.
#[path = "agent_presence/conversation.rs"]
mod conversation;
use atlas_shell::{
    agent_presence::{participant_name, AvatarVisual, PresencePainter},
    theme::Palette,
};
use eframe::egui;
use serde_json::Value;
use std::time::Instant;

const BENCH_PEERS: usize = 20;
const BENCH_WARMUP: usize = 90;
const BENCH_FRAMES: usize = 240;

struct BenchPeer {
    id: String,
    name: String,
    expression: String,
    prop: String,
    hat: String,
}

#[derive(Default)]
struct BenchTimes {
    avatar: Vec<f64>,
    names: Vec<f64>,
    paint: Vec<f64>,
    context: Vec<f64>,
    tessellation: Vec<f64>,
}

fn milliseconds(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn distribution(samples: &[f64]) -> Value {
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    serde_json::json!({
        "median_ms": (ordered[ordered.len() / 2 - 1] + ordered[ordered.len() / 2]) / 2.0,
        "p95_ms": ordered[(ordered.len() * 95).div_ceil(100) - 1],
        "max_ms": ordered[ordered.len() - 1],
    })
}

fn benchmark_case(peers: &[BenchPeer], rig: &Value, changing: bool) -> Value {
    let ctx = egui::Context::default();
    ctx.set_pixels_per_point(1.0);
    let palette = Palette::for_mode(false);
    ctx.set_visuals(palette.visuals());
    let mut presence = PresencePainter::default();
    let expressions = rig["expressions"].as_array().unwrap();
    let directions = rig["orientations"].as_array().unwrap();
    let mut times = BenchTimes::default();
    let mut last_primitives = 0;
    let mut last_vertices = 0;
    for frame in 0..BENCH_WARMUP + BENCH_FRAMES {
        let now = frame as f64 / 60.0;
        // Motion and evidence classification are outside the measured paint path.
        let poses: Vec<_> = peers
            .iter()
            .enumerate()
            .map(|(index, peer)| {
                let phase = now as f32 + index as f32 * 0.3;
                let center = egui::pos2(
                    90.0 + (index % 5) as f32 * 230.0 + phase.sin() * 24.0,
                    80.0 + (index / 5) as f32 * 155.0 + phase.cos() * 15.0,
                );
                let expression = if changing {
                    expressions[(index + frame / 15) % expressions.len()]["id"]
                        .as_str()
                        .unwrap()
                } else {
                    &peer.expression
                };
                let orientation = directions
                    [(index + if changing { frame / 23 } else { 0 }) % directions.len()]["id"]
                    .as_str()
                    .unwrap();
                (center, expression, orientation)
            })
            .collect();
        let mut avatar_ms = 0.0;
        let mut names_ms = 0.0;
        let mut paint_ms = 0.0;
        let context_start = Instant::now();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 720.0),
                )),
                time: Some(now),
                ..Default::default()
            },
            |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Middle,
                    egui::Id::new("presence-benchmark"),
                ));
                let paint_start = Instant::now();
                let avatar_start = Instant::now();
                for (peer, &(center, expression, orientation)) in peers.iter().zip(&poses) {
                    presence.paint_avatar(
                        &painter,
                        AvatarVisual {
                            id: &peer.id,
                            center,
                            expression,
                            orientation,
                            hat: &peer.hat,
                            prop: &peer.prop,
                            opacity: 1.0,
                        },
                        now,
                        false,
                    );
                }
                avatar_ms += milliseconds(avatar_start);
                let names_start = Instant::now();
                for (peer, &(center, _, _)) in peers.iter().zip(&poses) {
                    participant_name(&painter, center, &peer.name, &palette);
                }
                names_ms += milliseconds(names_start);
                paint_ms += milliseconds(paint_start);
            },
        );
        let context_ms = milliseconds(context_start);
        let tessellation_start = Instant::now();
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let tessellation_ms = milliseconds(tessellation_start);
        if frame >= BENCH_WARMUP {
            times.avatar.push(avatar_ms);
            times.names.push(names_ms);
            times.paint.push(paint_ms);
            times.context.push((context_ms - paint_ms).max(0.0));
            times.tessellation.push(tessellation_ms);
        }
        last_primitives = primitives.len();
        last_vertices = primitives
            .iter()
            .map(|primitive| match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                egui::epaint::Primitive::Callback(_) => 0,
            })
            .sum::<usize>();
        // Drop frame mesh references before updating the next cached face.
    }
    let paint = distribution(&times.paint);
    serde_json::json!({
        "case": if changing { "moving_with_expression_and_look_transitions" } else { "cached_moving_with_animated_props" },
        "paint_cpu": paint,
        "avatars_hats_props_cpu": distribution(&times.avatar),
        "name_labels_cpu": distribution(&times.names),
        "egui_context_overhead_cpu": distribution(&times.context),
        "egui_tessellation_cpu": distribution(&times.tessellation),
        "within_0_5_ms_p95_paint_budget": paint["p95_ms"].as_f64().unwrap() <= 0.5,
        "last_frame_primitives": last_primitives,
        "last_frame_vertices": last_vertices,
    })
}

fn benchmark() {
    let fixture = Preview::new(None);
    let art: Value = serde_json::from_str(include_str!("../assets/agent-hats.json")).unwrap();
    let hats: Vec<_> = art["hats"].as_object().unwrap().keys().collect();
    let work = [
        "reading",
        "writing",
        "organizing",
        "wiring",
        "chat",
        "creating-image",
        "reasoning",
        "searching",
        "presentations",
        "working",
    ];
    let records = fixture.activities["activities"].as_array().unwrap();
    let peers: Vec<_> = (0..BENCH_PEERS)
        .map(|index| {
            let record = records
                .iter()
                .find(|r| r["id"] == work[index % work.len()])
                .unwrap();
            BenchPeer {
                id: format!("benchmark-peer-{index}"),
                name: format!("Collaborator {}", index + 1),
                expression: record["expression"].as_str().unwrap().into(),
                prop: record["prop"].as_str().unwrap().into(),
                hat: hats[index % hats.len()].clone(),
            }
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "artifact": "headless production PresencePainter CPU evaluation; not computer-use acceptance",
        "peers": BENCH_PEERS,
        "head_points": art["headPoints"],
        "pixels_per_point": 1.0,
        "warmup_frames_per_case": BENCH_WARMUP,
        "measured_frames_per_case": BENCH_FRAMES,
        "simulated_frames_per_second": 60,
        "build_debug_assertions": cfg!(debug_assertions),
        "gpu_measured": false,
        "paint_scope": "production avatars, animated props, hats and name labels; excludes egui context setup/end, scene classification, tessellation and GPU",
        "budget_source": "docs/collab.md: twenty-peer presence paint under 0.5 ms; names dropped first",
        "cases": [benchmark_case(&peers, &fixture.rig, false), benchmark_case(&peers, &fixture.rig, true)],
    })).unwrap());
}

struct Preview {
    painter: PresencePainter,
    rig: Value,
    activities: Value,
    capture: Option<String>,
    state: usize,
    frames: usize,
}
impl Preview {
    fn new(capture: Option<String>) -> Self {
        Self {
            painter: PresencePainter::default(),
            rig: serde_json::from_str(include_str!("../assets/agent-avatar.json")).unwrap(),
            activities: serde_json::from_str(include_str!(
                "../../atlas-agent/assets/agent-activities.json"
            ))
            .unwrap(),
            capture,
            state: 0,
            frames: 0,
        }
    }
}
impl eframe::App for Preview {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dark = self.state >= 2;
        let phase = if self.capture.is_some() {
            if self.state % 2 == 0 {
                0.0
            } else {
                0.7
            }
        } else {
            ctx.input(|i| i.time)
        };
        let palette = Palette::for_mode(dark);
        ctx.set_visuals(palette.visuals());
        egui::CentralPanel::default().frame(egui::Frame::new().fill(palette.bg).inner_margin(24.0)).show(ctx,|ui|{
            ui.heading("Slate collaborators · native renderer");
            ui.label(egui::RichText::new("Approved face geometry at 32 screen points. The book turns; the matte head and eyes keep their original style.").small().color(palette.sub));
            ui.add_space(16.0);let origin=ui.cursor().min;let painter=ui.painter();
            let expressions=self.rig["expressions"].as_array().unwrap();let directions=self.rig["orientations"].as_array().unwrap();
            for(col,direction)in directions.iter().enumerate(){
                painter.text(origin+egui::vec2(122.0+col as f32*66.0,0.0),egui::Align2::CENTER_TOP,direction["label"].as_str().unwrap(),egui::FontId::proportional(10.0),palette.sub);
            }
            for(row,expression)in expressions.iter().enumerate(){
                painter.text(origin+egui::vec2(0.0,38.0+row as f32*54.0),egui::Align2::LEFT_CENTER,expression["label"].as_str().unwrap(),egui::FontId::proportional(11.0),palette.sub);
                for(col,direction)in directions.iter().enumerate(){
                    let id=format!("{}-{}",expression["id"],direction["id"]);
                    self.painter.paint_avatar(painter,AvatarVisual{id:&id,center:origin+egui::vec2(122.0+col as f32*66.0,38.0+row as f32*54.0),expression:expression["id"].as_str().unwrap(),orientation:direction["id"].as_str().unwrap(),hat:"none",prop:"none",opacity:1.0},phase,false);
                }
            }
            let x=origin.x+760.0;painter.text(egui::pos2(x,origin.y),egui::Align2::LEFT_TOP,"Small tools, alive",egui::FontId::proportional(15.0),palette.ink);
            let rows=["reading","writing","searching","creating-image","wiring","chat","reasoning","success"];
            for(row,activity)in rows.iter().enumerate(){
                let record=self.activities["activities"].as_array().unwrap().iter().find(|v|v["id"]==*activity).unwrap();
                let center=egui::pos2(x+20.0,origin.y+43.0+row as f32*54.0);
                self.painter.paint_avatar(painter,AvatarVisual{id:activity,center,expression:record["expression"].as_str().unwrap(),orientation:"down-right",hat:if row==0{"cap"}else if row==1{"beanie"}else{"none"},prop:record["prop"].as_str().unwrap(),opacity:1.0},phase,false);
                painter.text(center+egui::vec2(40.0,-3.0),egui::Align2::LEFT_CENTER,record["label"].as_str().unwrap(),egui::FontId::proportional(11.0),palette.sub);
            }
            painter.text(origin+egui::vec2(0.0,540.0),egui::Align2::LEFT_TOP,"Renderer fixture · shared atlas-shell painter · 4× MSAA · no provider connected",egui::FontId::proportional(10.0),palette.sub);
        });
        if let Some(prefix) = &self.capture {
            self.frames += 1;
            if self.frames == 8 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            let screenshot = ctx.input(|i| {
                i.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = screenshot {
                let pixels = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
                    .unwrap()
                    .save(format!(
                        "{prefix}-{}-page{}.png",
                        if dark { "dark" } else { "light" },
                        self.state % 2
                    ))
                    .unwrap();
                println!(
                    "Native capture: {}x{} pixels, {} pixels per point, theme {}, page {}",
                    image.width(),
                    image.height(),
                    ctx.pixels_per_point(),
                    if dark { "dark" } else { "light" },
                    self.state % 2
                );
                self.state += 1;
                self.frames = 0;
                if self.state == 4 {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--bench") {
        benchmark();
        return Ok(());
    }
    let capture = args
        .iter()
        .position(|a| a == "--capture")
        .and_then(|index| args.get(index + 1))
        .cloned();
    let conversation = args.iter().any(|arg| arg == "--conversation");
    eframe::run_native(
        "Slate collaborator renderer",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size(if conversation {
                [1280.0, 720.0]
            } else {
                [1120.0, 700.0]
            }),
            multisampling: atlas_shell::NATIVE_MSAA_SAMPLES,
            ..Default::default()
        },
        Box::new(move |_| {
            let app: Box<dyn eframe::App> = if conversation {
                Box::new(conversation::Preview::new(capture))
            } else {
                Box::new(Preview::new(capture))
            };
            Ok(app)
        }),
    )
}
