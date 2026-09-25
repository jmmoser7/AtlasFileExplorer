//! 3D viewport frame cost on a real GL context: an eframe window (vsync off,
//! or on with `BENCH_VSYNC=1`, as the app runs) drives the real `update_app`
//! with one placed model and synthetic input.
//!
//! ```powershell
//! cargo test -p slate --release --lib bench_model3d -- --ignored --nocapture
//! ```
//!
//! Per scenario: median and p95 of app time (`update_app`, the number the
//! session log records), median frame interval with repaint forced every
//! frame, and — for the idle scenarios — the frames the app asks for on its
//! own over two seconds (a repaint loop shows up here, not in app time).
//! Offscreen model renders and pixel readbacks per frame come from
//! `ModelEngine`'s counters.

use super::board_properties::Panel;
use super::SlateApp;
use eframe::egui;
use slate_doc::NodeId;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const WARM: usize = 40;
const MEASURED: usize = 150;
const NATURAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    FrozenIdle,
    FrozenSelected,
    StripOpen,
    DialogPending,
    /// The toolbar's 3D button: the real Explorer dialog, owned by the window.
    DialogReal,
    LiveIdle,
    LiveOrbit,
    /// One click on empty canvas while live and selected: the press frame's
    /// cost and whether that single click froze and deselected.
    ClickOff,
    /// Screenshot the live frame, freeze, screenshot the poster: the live
    /// GPU texture must match the poster (orientation and color space).
    LiveMatchesPoster,
}

const SCENARIOS: [Scenario; 9] = [
    Scenario::FrozenIdle,
    Scenario::FrozenSelected,
    Scenario::StripOpen,
    Scenario::DialogPending,
    Scenario::DialogReal,
    Scenario::LiveIdle,
    Scenario::LiveOrbit,
    Scenario::ClickOff,
    Scenario::LiveMatchesPoster,
];

impl Scenario {
    fn label(self) -> &'static str {
        match self {
            Scenario::FrozenIdle => "frozen, unselected",
            Scenario::FrozenSelected => "frozen + selected",
            Scenario::StripOpen => "frozen + display strip open",
            Scenario::DialogPending => "frozen + file dialog pending",
            Scenario::DialogReal => "frozen + real Explorer dialog",
            Scenario::LiveIdle => "live, pointer still",
            Scenario::LiveOrbit => "live, orbiting",
            Scenario::ClickOff => "live + selected, one click off",
            Scenario::LiveMatchesPoster => "live frame vs frozen poster",
        }
    }

    fn has_natural_window(self) -> bool {
        self != Scenario::LiveOrbit
    }
}

enum Stage {
    Warm(usize),
    Measure(usize),
    Natural { started: Instant, frames: usize },
}

struct Bench {
    app: SlateApp,
    model: NodeId,
    scenario: usize,
    stage: Stage,
    app_ms: Vec<f32>,
    interval_ms: Vec<f32>,
    last_update: Option<Instant>,
    gl_at_measure: (u64, u64, u64),
    orbit_frame: usize,
    last_pointer: Option<egui::Pos2>,
    click: ClickScript,
    dialog: Option<crossbeam_channel::Sender<super::PickerMsg>>,
    report: Arc<Mutex<Vec<String>>>,
    shot: ShotCheck,
}

/// Frames to let a state settle (live slot drawn and registered, or the
/// poster uploaded) before a screenshot.
const SETTLE: usize = 12;

#[derive(Default)]
struct ShotCheck {
    phase: usize,
    frames: usize,
    live: Option<Arc<egui::ColorImage>>,
}

/// Mean RGB of each quadrant of `rect` (points) in a window screenshot.
fn quadrant_means(img: &egui::ColorImage, rect: egui::Rect, ppp: f32) -> [[f32; 3]; 4] {
    let r = rect.shrink(8.0);
    let mid = r.center();
    let quads = [
        egui::Rect::from_min_max(r.min, mid),
        egui::Rect::from_min_max(egui::pos2(mid.x, r.min.y), egui::pos2(r.max.x, mid.y)),
        egui::Rect::from_min_max(egui::pos2(r.min.x, mid.y), egui::pos2(mid.x, r.max.y)),
        egui::Rect::from_min_max(mid, r.max),
    ];
    quads.map(|q| {
        let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
        let x0 = (q.min.x * ppp) as usize;
        let x1 = ((q.max.x * ppp) as usize).min(img.size[0]);
        let y0 = (q.min.y * ppp) as usize;
        let y1 = ((q.max.y * ppp) as usize).min(img.size[1]);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = img.pixels[y * img.size[0] + x];
                sum[0] += p.r() as f32;
                sum[1] += p.g() as f32;
                sum[2] += p.b() as f32;
                n += 1.0;
            }
        }
        sum.map(|s| s / n.max(1.0))
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Release the orbit left over from the previous scenario.
    UpAtModel,
    /// Freeze and deselect, the state a person double-clicks into.
    Freeze,
    DownAtModel,
    UpAtModel2,
    DownAtModel2,
    UpAtModel3,
    CheckLive,
    DownOff,
    UpOff,
    Report,
}

/// The click-off script at human timing: (step, wait before it in ms).
const CLICK_SCRIPT: [(Step, u64); 10] = [
    (Step::UpAtModel, 0),
    (Step::Freeze, 100),
    (Step::DownAtModel, 400),
    (Step::UpAtModel2, 60),
    (Step::DownAtModel2, 90),
    (Step::UpAtModel3, 60),
    (Step::CheckLive, 800),
    (Step::DownOff, 0),
    (Step::UpOff, 90),
    (Step::Report, 600),
];

#[derive(Default)]
struct ClickScript {
    step: usize,
    since: Option<Instant>,
    fired: bool,
    live_after_double_click: bool,
    press_ms: f32,
}

fn percentile(samples: &[f32], p: f32) -> f32 {
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((s.len() - 1) as f32 * p).round() as usize]
}

/// A UV sphere of `rings * segments * 2` triangles, the size of a modest
/// Rhino massing model.
fn write_sphere_obj(path: &Path, rings: usize, segments: usize) {
    use std::fmt::Write;
    let mut obj = String::new();
    for r in 0..=rings {
        let phi = std::f32::consts::PI * r as f32 / rings as f32;
        for s in 0..segments {
            let theta = std::f32::consts::TAU * s as f32 / segments as f32;
            let _ = writeln!(
                obj,
                "v {} {} {}",
                phi.sin() * theta.cos(),
                phi.sin() * theta.sin(),
                phi.cos()
            );
        }
    }
    for r in 0..rings {
        for s in 0..segments {
            let a = r * segments + s + 1;
            let b = r * segments + (s + 1) % segments + 1;
            let c = a + segments;
            let d = b + segments;
            let _ = writeln!(obj, "f {a} {b} {d}");
            let _ = writeln!(obj, "f {a} {d} {c}");
        }
    }
    std::fs::write(path, obj).unwrap();
}

impl Bench {
    fn new(
        cc: &eframe::CreationContext<'_>,
        model: PathBuf,
        report: Arc<Mutex<Vec<String>>>,
    ) -> Self {
        let mut app = SlateApp::with_ctx(&cc.egui_ctx, None);
        app.gl = cc.gl.clone();
        app.kits = super::kits::KitState::builtin_only();
        app.leave_home();
        app.ensure_work_tab();
        let items = app.add_paths(&[model]);
        app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        app.place_items_on_board(&items, egui::Pos2::ZERO);
        let id = app.doc().scene.nodes.last().unwrap().id;
        let rect = app.doc().scene.node(id).unwrap().rect;
        // Room around the node so "empty canvas" exists on screen.
        app.zoom_to_rect(slate_doc::scene::WorldRect::new(
            rect.x - rect.w * 0.5,
            rect.y - rect.h * 0.5,
            rect.w * 2.0,
            rect.h * 2.0,
        ));
        Bench {
            app,
            model: id,
            scenario: 0,
            stage: Stage::Warm(0),
            app_ms: Vec::new(),
            interval_ms: Vec::new(),
            last_update: None,
            gl_at_measure: (0, 0, 0),
            orbit_frame: 0,
            last_pointer: None,
            click: ClickScript::default(),
            dialog: None,
            report,
            shot: ShotCheck::default(),
        }
    }

    /// Settle live, screenshot, freeze, settle, screenshot, compare.
    fn shot_step(&mut self, ctx: &egui::Context, scenario: Scenario) {
        let screenshot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        let shot = &mut self.shot;
        shot.frames += 1;
        match shot.phase {
            0 => {
                self.app.unlock_model(self.model);
                shot.phase = 1;
                shot.frames = 0;
            }
            1 | 3 if shot.frames == SETTLE => {
                if shot.phase == 1 {
                    assert!(
                        self.app.model3d.live_texture_id(self.model).is_some(),
                        "the live slot, not the poster, is on screen"
                    );
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                shot.phase += 1;
            }
            2 => {
                if let Some(image) = screenshot {
                    shot.live = Some(image);
                    self.app.lock_model(self.model);
                    shot.phase = 3;
                    shot.frames = 0;
                }
            }
            4 => {
                if let Some(poster) = screenshot {
                    let rect = self.app.doc().scene.node(self.model).unwrap().rect;
                    let srect = self.app.board_xf().rect_w2s(rect);
                    let ppp = ctx.pixels_per_point();
                    let live = quadrant_means(shot.live.as_ref().unwrap(), srect, ppp);
                    let frozen = quadrant_means(&poster, srect, ppp);
                    let diff = live
                        .iter()
                        .zip(&frozen)
                        .flat_map(|(a, b)| (0..3).map(move |c| (a[c] - b[c]).abs()))
                        .fold(0.0f32, f32::max);
                    let lum = |q: &[f32; 3]| q.iter().sum::<f32>() / 3.0;
                    let spread = frozen.iter().map(lum).fold(0.0f32, f32::max)
                        - frozen.iter().map(lum).fold(255.0f32, f32::min);
                    let line = format!(
                        "{:30} quadrant max diff {:5.1} / 255 | poster quadrant spread {:5.1} | live {:?} | poster {:?}",
                        scenario.label(),
                        diff,
                        spread,
                        live.map(|q| q.map(|c| c.round() as u8)),
                        frozen.map(|q| q.map(|c| c.round() as u8)),
                    );
                    println!("{line}");
                    self.report.lock().unwrap().push(line);
                    assert!(diff < 12.0, "the live frame matches the poster");
                    self.scenario += 1;
                }
            }
            _ => {}
        }
    }

    fn current(&self) -> Option<Scenario> {
        SCENARIOS.get(self.scenario).copied()
    }

    fn model_center(&self) -> egui::Pos2 {
        let rect = self.app.doc().scene.node(self.model).unwrap().rect;
        self.app.board_xf().rect_w2s(rect).center()
    }

    fn empty_canvas(&self) -> egui::Pos2 {
        let rect = self.app.doc().scene.node(self.model).unwrap().rect;
        let srect = self.app.board_xf().rect_w2s(rect);
        egui::pos2(srect.right() + 60.0, srect.center().y)
    }

    fn enter(&mut self, scenario: Scenario) {
        match scenario {
            Scenario::FrozenIdle => self.app.board_sel.clear(),
            Scenario::FrozenSelected => {
                self.app.board_sel = std::iter::once(self.model).collect();
            }
            Scenario::StripOpen => self.app.shape_properties.panel = Some(Panel::ModelDisplay),
            Scenario::DialogPending => {
                self.app.shape_properties.panel = None;
                let (tx, rx) = crossbeam_channel::unbounded();
                assert!(self.app.picker.adopt(rx), "no dialog was open");
                self.dialog = Some(tx);
            }
            Scenario::DialogReal => {
                assert!(!self.app.dialogs.any_open());
                self.app
                    .add_media_dialog(slate_doc::media::MediaGroup::Model);
            }
            Scenario::LiveIdle => {
                self.app.unlock_model(self.model);
                assert!(self.app.model3d.live.contains_key(&self.model));
            }
            Scenario::LiveOrbit => self.orbit_frame = 0,
            Scenario::ClickOff | Scenario::LiveMatchesPoster => {}
        }
    }

    fn inject(&mut self, raw: &mut egui::RawInput) {
        let Some(scenario) = self.current() else {
            return;
        };
        if scenario == Scenario::ClickOff {
            let Some((step, wait)) = CLICK_SCRIPT.get(self.click.step).copied() else {
                return;
            };
            let due = self
                .click
                .since
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(wait));
            let button = match step {
                Step::UpAtModel | Step::UpAtModel2 | Step::UpAtModel3 => {
                    Some((self.model_center(), false))
                }
                Step::DownAtModel | Step::DownAtModel2 => Some((self.model_center(), true)),
                Step::DownOff => Some((self.empty_canvas(), true)),
                Step::UpOff => Some((self.empty_canvas(), false)),
                _ => None,
            };
            if let (true, Some((p, pressed))) = (due, button) {
                self.move_pointer(raw, p);
                raw.events.push(egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
                self.click.fired = true;
            }
            return;
        }
        let pointer = match scenario {
            Scenario::LiveIdle => self.model_center(),
            Scenario::LiveOrbit => {
                let c = self.model_center();
                let i = self.orbit_frame;
                self.orbit_frame += 1;
                if i == 0 {
                    raw.events.push(egui::Event::PointerMoved(c));
                    raw.events.push(egui::Event::PointerButton {
                        pos: c,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    });
                    return;
                }
                // Sweep back and forth so the camera keeps changing.
                let phase = (i % 80) as f32 / 80.0 * std::f32::consts::TAU;
                c + egui::vec2(phase.sin() * 120.0, phase.cos() * 30.0)
            }
            _ => self.empty_canvas(),
        };
        self.move_pointer(raw, pointer);
        self.app.dialogs.gate_input(raw);
    }

    /// Only real movement is input; a repeated position would itself keep
    /// egui repainting and hide the app's own repaint requests.
    fn move_pointer(&mut self, raw: &mut egui::RawInput, p: egui::Pos2) {
        if self.last_pointer != Some(p) {
            raw.events.push(egui::Event::PointerMoved(p));
            self.last_pointer = Some(p);
        }
    }

    fn click_step(&mut self, scenario: Scenario, app_ms: f32) {
        let Some((step, wait)) = CLICK_SCRIPT.get(self.click.step).copied() else {
            return;
        };
        let due = self
            .click
            .since
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(wait));
        let done = match step {
            Step::Freeze if due => {
                self.app.lock_model(self.model);
                self.app.board_sel.clear();
                true
            }
            Step::CheckLive if due => {
                self.click.live_after_double_click =
                    self.app.model3d.live.contains_key(&self.model);
                true
            }
            Step::Report if due => {
                let line = format!(
                    "{:30} live after double-click: {} | press frame {:7.2} ms | frozen after one click: {} | deselected after one click: {}",
                    scenario.label(),
                    self.click.live_after_double_click,
                    self.click.press_ms,
                    !self.app.model3d.live.contains_key(&self.model),
                    self.app.board_sel.is_empty(),
                );
                println!("{line}");
                self.report.lock().unwrap().push(line);
                self.scenario += 1;
                true
            }
            Step::Freeze | Step::CheckLive | Step::Report => false,
            _ => std::mem::take(&mut self.click.fired),
        };
        if std::env::var_os("BENCH_TRACE").is_some() && done {
            println!(
                "  click step {} live={} sel={} drag={} app {:.2} ms",
                self.click.step,
                self.app.model3d.live.contains_key(&self.model),
                self.app.board_sel.len(),
                self.app.board_drag.is_some(),
                app_ms,
            );
        }
        if done {
            if step == Step::DownOff {
                self.click.press_ms = app_ms;
            }
            self.click.step += 1;
            self.click.since = Some(Instant::now());
        }
    }

    fn finish(&mut self, scenario: Scenario, natural: Option<usize>) {
        let (passes, reads, creates) = self.app.model3d.gl_counts();
        let frames = self.app_ms.len().max(1) as f32;
        let line = format!(
            "{:30} app median {:6.2} ms  p95 {:6.2} ms  max {:6.2} ms | interval median {:6.2} ms | renders/frame {:5.2} readbacks/frame {:5.2} gl-creates/frame {:5.2} | self-requested frames in {} s: {}",
            scenario.label(),
            percentile(&self.app_ms, 0.5),
            percentile(&self.app_ms, 0.95),
            percentile(&self.app_ms, 1.0),
            percentile(&self.interval_ms, 0.5),
            (passes - self.gl_at_measure.0) as f32 / frames,
            (reads - self.gl_at_measure.1) as f32 / frames,
            (creates - self.gl_at_measure.2) as f32 / frames,
            NATURAL.as_secs(),
            natural.map_or("n/a".to_string(), |n| n.to_string()),
        );
        println!("{line}");
        self.report.lock().unwrap().push(line);
        // The fake dialog's slot frees on the next poll.
        self.dialog = None;
        self.app_ms.clear();
        self.interval_ms.clear();
    }
}

impl eframe::App for Bench {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.inject(raw);
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app
            .dialogs
            .set_owner(atlas_shell::file_picker::DialogOwner::from_window(frame));
        self.app.model3d.register_live_textures(frame);
        let Some(scenario) = self.current() else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        };
        let dialog_scenario = matches!(scenario, Scenario::DialogPending | Scenario::DialogReal);
        if !dialog_scenario && self.app.dialogs.any_open() {
            // Wait out the previous scenario's dialog; its gate eats input.
            self.app.dialogs.close_all();
            self.app.update_app(ctx);
            self.last_update = None;
            ctx.request_repaint();
            return;
        }
        let now = Instant::now();
        let interval = self.last_update.map(|t| now.duration_since(t));
        self.last_update = Some(now);
        let t = Instant::now();
        self.app.update_app(ctx);
        let app_ms = t.elapsed().as_secs_f32() * 1000.0;

        if scenario == Scenario::ClickOff {
            self.click_step(scenario, app_ms);
            ctx.request_repaint();
            return;
        }
        if scenario == Scenario::LiveMatchesPoster {
            self.shot_step(ctx, scenario);
            ctx.request_repaint();
            return;
        }

        if matches!(self.stage, Stage::Warm(0)) {
            self.enter(scenario);
        }
        match &mut self.stage {
            Stage::Warm(n) => {
                *n += 1;
                if *n >= WARM {
                    self.gl_at_measure = self.app.model3d.gl_counts();
                    self.stage = Stage::Measure(0);
                }
                ctx.request_repaint();
            }
            Stage::Measure(n) => {
                self.app_ms.push(app_ms);
                if let Some(i) = interval {
                    self.interval_ms.push(i.as_secs_f32() * 1000.0);
                }
                *n += 1;
                if *n >= MEASURED {
                    if scenario.has_natural_window() {
                        self.stage = Stage::Natural {
                            started: Instant::now(),
                            frames: 0,
                        };
                        // The one wake the bench asks for; every other frame
                        // in the window is the app's own request.
                        ctx.request_repaint_after(NATURAL);
                    } else {
                        self.finish(scenario, None);
                        self.scenario += 1;
                        self.stage = Stage::Warm(0);
                        ctx.request_repaint();
                    }
                } else {
                    ctx.request_repaint();
                }
            }
            Stage::Natural { started, frames } => {
                if started.elapsed() >= NATURAL {
                    let frames = *frames;
                    self.finish(scenario, Some(frames));
                    self.scenario += 1;
                    self.stage = Stage::Warm(0);
                    ctx.request_repaint();
                } else {
                    *frames += 1;
                    if std::env::var_os("BENCH_TRACE").is_some() && *frames % 200 == 1 {
                        println!("  repaint causes: {:?}", ctx.repaint_causes());
                    }
                }
            }
        }
    }
}

#[test]
#[ignore]
fn bench_model3d() {
    std::env::set_var("ATLAS_SESSION_LOG", "0");
    let dir = std::env::temp_dir().join(format!("slate_bench_model3d_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("sphere.obj");
    write_sphere_obj(&model, 140, 140);
    let report = Arc::new(Mutex::new(Vec::new()));
    let sink = report.clone();
    let vsync = std::env::var_os("BENCH_VSYNC").is_some();
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        vsync,
        run_and_return: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_title("Slate 3D bench"),
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
    let profile = if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    };
    let vsync = if vsync { "on" } else { "off" };
    println!("profile={profile} model=39200 triangles window=1440x900 vsync={vsync}");
    eframe::run_native(
        "Slate 3D bench",
        options,
        Box::new(move |cc| Ok(Box::new(Bench::new(cc, model, sink)))),
    )
    .unwrap();
    let lines = report.lock().unwrap().len();
    assert_eq!(lines, SCENARIOS.len(), "every scenario reported");
    let _ = std::fs::remove_dir_all(&dir);
}
