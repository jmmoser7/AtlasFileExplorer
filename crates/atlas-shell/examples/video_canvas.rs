//! Standalone phase 03 host for the production video decoder and policy.
use atlas_shell::{
    canvas_scale, canvas_text,
    file_picker::{DialogGate, DialogOwner, FilePicker, PickRequest},
    theme::Palette,
    video_controls,
};
use eframe::egui::{self, pos2, vec2, Align2, Rect, Stroke, StrokeKind};
use std::path::PathBuf;
#[path = "video_canvas/media.rs"]
mod media;
#[path = "video_canvas/stress.rs"]
mod stress;

struct Studio {
    media: media::Media,
    origin: egui::Pos2,
    zoom: f32,
    fit: bool,
    dark: bool,
    diagnostics: bool,
    stress: Option<stress::Run>,
    report: String,
    gate: DialogGate,
    picker: FilePicker<Option<Vec<PathBuf>>>,
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../atlas-core/tests/fixtures/video-motion.mp4")
}

impl Studio {
    fn new(mut paths: Vec<PathBuf>) -> Self {
        if paths.is_empty() {
            paths = vec![fixture(), fixture(), fixture()];
        }
        let gate = DialogGate::default();
        let picker = gate.picker();
        Self {
            media: media::Media::new(paths),
            origin: egui::Pos2::ZERO,
            zoom: 1.0,
            fit: true,
            dark: false,
            diagnostics: false,
            stress: None,
            report: String::new(),
            gate,
            picker,
        }
    }

    fn canvas(&mut self, ui: &mut egui::Ui, theme: Palette) {
        let bounds = ui.max_rect();
        if self.fit {
            let height = ((self.media.clips.len().saturating_sub(1)) as f32 * 290.0).max(660.0);
            self.zoom = ((bounds.width() - 80.0) / 1120.0)
                .min((bounds.height() - 80.0) / height)
                .max(0.1);
            self.origin = bounds.center() - vec2(560.0, height * 0.5) * self.zoom;
            self.fit = false;
        }
        let pan = ui.input(|i| {
            i.pointer.middle_down()
                || i.pointer.secondary_down()
                || (i.key_down(egui::Key::Space) && i.pointer.primary_down())
        });
        if let Some(p) = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| bounds.contains(*p))
        {
            if pan {
                self.origin += ui.input(|i| i.pointer.delta());
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            let horizontal = ui.input(|i| i.modifiers.shift && !i.modifiers.ctrl);
            if horizontal {
                self.origin.x += scroll;
            } else if scroll != 0.0 {
                let profile = atlas_core::display::SLATE_BOARD;
                let next = profile.clamp(self.zoom * profile.wheel_factor(scroll));
                self.origin = p + (self.origin - p) * next / self.zoom;
                self.zoom = next;
            }
        }
        let z = self.zoom;
        for i in 0..self.media.clips.len() {
            let (x, y, w, h) = if i == 0 {
                (0.0, 0.0, 720.0, 405.0)
            } else {
                (760.0, ((i - 1) as f32) * 290.0, 360.0, 202.5)
            };
            let rect = Rect::from_min_size(self.origin + vec2(x, y) * z, vec2(w, h) * z);
            let clip = &self.media.clips[i];
            ui.painter().rect_filled(rect, 4, theme.thumb_bg);
            if let Some(tex) = &clip.texture {
                let aspect = tex.size_vec2();
                let size = aspect * (rect.width() / aspect.x).min(rect.height() / aspect.y);
                ui.painter().image(
                    tex.id(),
                    Rect::from_center_size(rect.center(), size),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                canvas_text::text(
                    ui.painter(),
                    rect.center(),
                    Align2::CENTER_CENTER,
                    clip.state,
                    canvas_scale::font(14.0, z),
                    theme.sub,
                );
            }
            ui.painter().rect_stroke(
                rect,
                4,
                Stroke::new(1.0_f32, theme.border),
                StrokeKind::Outside,
            );
            let name = clip.path.file_name().unwrap_or_default().to_string_lossy();
            canvas_text::text(
                ui.painter(),
                rect.left_top() - vec2(0.0, 9.0 * z),
                Align2::LEFT_BOTTOM,
                format!("{} · {}", i + 1, name),
                canvas_scale::font(15.0, z),
                theme.ink,
            );
            let mut time = clip.time;
            let duration = clip.duration;
            let mut looped = clip.looped;
            let playing = self.media.playing == Some(i);
            video_controls::paint(ui.painter(), rect, z, playing, theme);
            let mut toggle = false;
            let mut replay = false;
            let controls = Rect::from_min_size(
                rect.left_bottom() + vec2(0.0, 8.0 * z),
                vec2(w * z, 72.0 * z),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(controls), |ui| {
                ui.style_mut().override_font_id = Some(canvas_scale::font(13.0, z));
                ui.horizontal(|ui| {
                    replay = ui.button("Replay").clicked();
                    ui.checkbox(&mut looped, "Loop");
                    ui.label(atlas_core::video::format_timecode(time));
                });
                if ui
                    .add(
                        egui::Slider::new(&mut time, 0.0..=duration.max(0.001))
                            .show_value(false)
                            .text("Scrub"),
                    )
                    .changed()
                {
                    self.media.scrub(i, time, ui.ctx());
                }
                ui.colored_label(theme.sub, self.media.clips[i].state);
            });
            let loop_changed = self.media.clips[i].looped != looped;
            self.media.clips[i].looped = looped;
            if !pan
                && ui
                    .interact(rect, ui.id().with(("video", i)), egui::Sense::click())
                    .on_hover_text(if playing {
                        "Pause video"
                    } else {
                        "Play video · retry if unavailable"
                    })
                    .clicked()
            {
                toggle = true;
            }
            let control = ui
                .interact(
                    video_controls::rect(rect, z),
                    ui.id().with(("video-control", i)),
                    egui::Sense::click(),
                )
                .on_hover_text(if playing {
                    "Pause video"
                } else {
                    "Play video · retry if unavailable"
                });
            control.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    true,
                    if playing { "Pause video" } else { "Play video" },
                )
            });
            if !pan && control.clicked() {
                toggle = true;
            }
            if toggle {
                if playing {
                    self.media.pause();
                } else {
                    self.media.play(i, false);
                }
            }
            if replay {
                self.media.play(i, true);
            }
            if loop_changed && playing {
                self.media.play(i, false);
            }
        }
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.gate.set_owner(DialogOwner::from_window(frame));
        if let Some(Some(paths)) = self.picker.poll(ctx) {
            for path in paths {
                self.media.add(path);
            }
        }
        self.media.pump(ctx);
        if let Some(run) = &mut self.stress {
            if let Some(report) = run.tick(&mut self.media, ctx) {
                self.report = report;
                self.stress = None;
            }
        }
        let drops = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect::<Vec<_>>()
        });
        if !drops.is_empty() {
            self.gate.close_all();
        }
        for path in drops {
            self.media.add(path);
        }
        let theme = Palette::for_mode(self.dark);
        ctx.set_visuals(theme.visuals());
        egui::TopBottomPanel::top("studio-chrome").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Videos · 03 Rust Canvas");
                if ui.button("Open video…").clicked() {
                    self.picker.open(PickRequest::files().filter("Video", &["mp4","mov","mkv","webm","avi","wmv"]),|paths|paths);
                }
                if ui.button("Clear").clicked() { self.stress = None; self.media.clear(); }
                if ui.button("Fit").clicked() {
                    self.fit = true;
                }
                ui.checkbox(&mut self.dark, "Dark");
                ui.checkbox(&mut self.diagnostics, "Decoder / stress");
            });
            if self.diagnostics {
                let health = self.media.pool.health();
                ui.label(format!("Decoder {:?} · worker {:?} · restarts {} · failures {} · queued {} KiB · dropped {}",
                    health.status,health.worker_pid,health.restarts,health.failures,health.queued_frame_bytes/1024,health.dropped_frames));
                ui.label(format!("Retained decoded samples {:.1} / 40 MiB · texture RGBA estimate {:.1} MiB · {} positions cached",
                    self.media.cache_bytes() as f64/1048576.0,self.media.texture_bytes() as f64/1048576.0,
                    self.media.clips.iter().map(|c|c.slots.iter().flatten().count()).sum::<usize>()));
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Decoded playback frames: {} · failures: {} · stale events rejected: {}",
                        self.media.clips.iter().map(|c| c.frames).sum::<u64>(),
                        self.media.failures,
                        self.media.stale
                    ));
                    if ui
                        .add_enabled(
                            self.stress.is_none(),
                            egui::Button::new("Run real decode stress"),
                        )
                        .clicked()
                    {
                        self.stress = Some(stress::Run::new(&mut self.media));
                    }
                    if self.stress.is_some() && ui.button("Stop stress").clicked() {
                        self.stress = None;
                        self.media.pause();
                    }
                });
                ui.label(if self.stress.is_some() {
                    "Stress running · actual decoder play/pause/replay/switch/failure recovery"
                } else {
                    &self.report
                });
            }
            if !self.media.notice.is_empty() { ui.label(&self.media.notice); }
        });
        egui::TopBottomPanel::bottom("navigation-help").show(ctx,|ui| {
            ui.label("Click video or Play: play / pause · drag Scrub: decoded samples · right / middle / Space-drag: pan · wheel: zoom · drop local videos");
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(theme.bg))
            .show(ctx, |ui| self.canvas(ui, theme));
        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }

    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        self.gate.gate_input(raw);
    }
}

fn main() -> eframe::Result {
    let mut paths = vec![];
    let mut auto_stress = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--stress" {
            auto_stress = true;
        } else if arg == "--corpus" {
            if let Some(directory) = args.next() {
                if let Ok(entries) = std::fs::read_dir(directory) {
                    let mut clips = entries
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| {
                            path.extension().is_some_and(|ext| {
                                matches!(
                                    ext.to_str(),
                                    Some("mp4" | "webm" | "mov" | "mkv" | "avi" | "wmv")
                                )
                            })
                        })
                        .collect::<Vec<_>>();
                    clips.sort();
                    paths.extend(clips);
                }
            }
        } else {
            paths.push(PathBuf::from(arg));
        }
    }
    eframe::run_native(
        "Slate Studio · Videos",
        eframe::NativeOptions {
            multisampling: atlas_shell::NATIVE_MSAA_SAMPLES,
            viewport: egui::ViewportBuilder::default()
                .with_title("Slate Studio · 03 Rust Canvas · Videos")
                .with_inner_size([1280.0, 820.0]),
            ..Default::default()
        },
        Box::new(move |_| {
            let mut studio = Studio::new(paths);
            if auto_stress {
                studio.diagnostics = true;
                studio.stress = Some(stress::Run::new(&mut studio.media));
            }
            Ok(Box::new(studio))
        }),
    )
}
