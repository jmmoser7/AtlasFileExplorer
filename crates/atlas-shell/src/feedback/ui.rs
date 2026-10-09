use super::bundle::{self, ReportJson};
use super::{FeedbackHub, FeedbackKind, FeedbackPhase, FeedbackPrefs};
use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{
    self, Color32, CornerRadius, Id, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use std::path::PathBuf;

pub fn suggestion_button(
    ctx: &egui::Context,
    palette: &Palette,
    dock_id: &str,
    canvas: Rect,
    hub: &mut FeedbackHub,
) -> bool {
    let t = tokens::current().readouts.clone();
    let hit = t.chevron_hit.max(20.0);
    let chevron = canvas.left_bottom() + Vec2::new(t.chevron_inset_x, -t.chevron_inset_y);
    let pos = chevron + Vec2::new(-hit - 6.0, 0.0);
    let mut open = false;
    egui::Area::new(Id::new(("suggestion_box", dock_id)))
        .fixed_pos(pos)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(hit), Sense::click());
            let hovered = resp.hovered();
            if hovered {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    palette.ink.gamma_multiply(t.chevron_hover_fill),
                );
            }
            let ink = palette.ink.gamma_multiply(if hovered {
                t.chevron_hover_opacity
            } else {
                t.chevron_idle_opacity
            });
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "💬",
                egui::FontId::proportional(14.0),
                ink,
            );
            if resp
                .on_hover_text("Suggestion box — report a bug or request a feature")
                .clicked()
            {
                open = true;
            }
        });
    if open {
        hub.open_picker();
    }
    open
}

pub fn paint_recording_chrome(ctx: &egui::Context, canvas: Rect, palette: &Palette) {
    if !canvas.is_positive() {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        Id::new("feedback_recording_border"),
    ));
    painter.rect_stroke(
        canvas.shrink(1.0),
        CornerRadius::same(8),
        Stroke::new(2.0_f32, palette.accent),
        StrokeKind::Inside,
    );
    let top = canvas.left_top() + Vec2::new(0.0, 4.0);
    egui::Area::new(Id::new("feedback_recording_banner"))
        .fixed_pos(top)
        .pivot(egui::Align2::LEFT_TOP)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(palette.accent.gamma_multiply(0.92))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Recording steps — press Enter to finish")
                            .color(palette.bg)
                            .strong(),
                    );
                });
        });
}

pub struct FeedbackUiOutput {
    pub command: Option<&'static str>,
    pub toast: Option<String>,
}

pub fn dialogs(
    ctx: &egui::Context,
    palette: &Palette,
    hub: &mut FeedbackHub,
    app_name: &str,
    version: &str,
    session_log: &atlas_core::session_log::SessionLog,
) -> FeedbackUiOutput {
    let mut out = FeedbackUiOutput {
        command: None,
        toast: None,
    };
    if hub.phase == FeedbackPhase::Recording {
        if let Some(rec) = hub.recorder.as_mut() {
            rec.ingest_stalls(&session_log.last_stalls());
            if rec.timed_out() {
                hub.finish_recording();
                hub.phase = hub.form_after_recording();
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.any()) {
            hub.finish_recording();
            hub.phase = hub.form_after_recording();
            out.command = Some("app.feedback.finish_recording");
        }
        return out;
    }

    poll_screenshot(ctx, hub);

    match hub.phase {
        FeedbackPhase::Closed | FeedbackPhase::Recording => return out,
        FeedbackPhase::KindPicker => {
            let mut open = true;
            egui::Window::new("Suggestion box")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(320.0)
                .show(ctx, |ui| {
                    ui.label("What would you like to send?");
                    ui.add_space(8.0);
                    if ui.button("Report a bug").clicked() {
                        hub.begin_form(FeedbackKind::Bug);
                    }
                    if ui.button("Request a feature").clicked() {
                        hub.begin_form(FeedbackKind::Feature);
                    }
                });
            if !open {
                hub.phase = FeedbackPhase::Closed;
            }
        }
        FeedbackPhase::BugForm | FeedbackPhase::FeatureForm => {
            let kind = hub.kind.unwrap();
            let title = match kind {
                FeedbackKind::Bug => "Report a bug",
                FeedbackKind::Feature => "Request a feature",
            };
            let mut open = true;
            let mut close_after = false;
            egui::Window::new(title)
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label(RichText::new("Description").strong());
                    ui.add(
                        egui::TextEdit::multiline(&mut hub.description)
                            .desired_width(f32::INFINITY)
                            .desired_rows(4),
                    );
                    ui.add_space(6.0);
                    attachments_ui(ui, palette, hub, ctx);
                    ui.add_space(6.0);
                    ui.label(RichText::new("Links (one per line)").strong());
                    ui.add(
                        egui::TextEdit::multiline(&mut hub.links_text)
                            .desired_width(f32::INFINITY)
                            .desired_rows(2),
                    );
                    if matches!(kind, FeedbackKind::Bug) {
                        ui.add_space(6.0);
                        ui.checkbox(&mut hub.reproduce_wanted, "Reproduce (record steps)");
                        if !hub.recorded_steps.is_empty() {
                            ui.label(RichText::new("Recorded steps").strong());
                            let mut remove: Option<usize> = None;
                            for (i, step) in hub.recorded_steps.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{} ms — {}", step.at_ms, step.label));
                                    if ui.small_button("✕").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            }
                            if let Some(i) = remove {
                                hub.recorded_steps.remove(i);
                            }
                        }
                        if hub.reproduce_wanted && hub.recorded_steps.is_empty() {
                            if ui.button("Start recording").clicked() {
                                hub.start_recording(session_log);
                                close_after = true;
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Submit").clicked() {
                            match submit(hub, app_name, version) {
                                Ok(dir) => {
                                    hub.set_last_bundle(dir.clone());
                                    out.toast = Some(format!("Saved report to {}", dir.display()));
                                    hub.reset_after_submit();
                                    close_after = true;
                                }
                                Err(e) => out.toast = Some(e),
                            }
                        }
                        if ui.button("Email…").clicked() {
                            if let Some(body) = hub.mailto_summary(app_name, version) {
                                open_mailto(&hub.mailto_subject(app_name), &body);
                                let dir = if hub.last_bundle_dir.as_os_str().is_empty() {
                                    hub.prefs
                                        .reports_dir
                                        .clone()
                                        .unwrap_or_else(bundle::default_reports_dir)
                                } else {
                                    hub.last_bundle_dir.clone()
                                };
                                out.toast = Some(format!("Attach files from {}", dir.display()));
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            hub.phase = FeedbackPhase::Closed;
                            close_after = true;
                        }
                    });
                });
            if close_after {
                open = false;
            }
            if !open && hub.phase != FeedbackPhase::Recording {
                if matches!(
                    hub.phase,
                    FeedbackPhase::BugForm | FeedbackPhase::FeatureForm
                ) {
                    hub.phase = FeedbackPhase::Closed;
                }
            }
        }
    }
    out
}

fn attachments_ui(
    ui: &mut egui::Ui,
    palette: &Palette,
    hub: &mut FeedbackHub,
    ctx: &egui::Context,
) {
    ui.label(RichText::new("Images").strong());
    ui.label(
        RichText::new("Drop image files here or paste from the clipboard.")
            .small()
            .color(palette.sub),
    );
    let drop = ui
        .allocate_rect(ui.available_rect_before_wrap(), Sense::hover())
        .rect;
    ui.painter().rect_stroke(
        drop,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, palette.sub.gamma_multiply(0.35)),
        StrokeKind::Inside,
    );
    ui.allocate_ui_at_rect(drop, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            if ui.button("Capture window").clicked() {
                hub.request_screenshot(ctx);
            }
            if !hub.attachments.is_empty() {
                ui.label(format!("{} attachment(s)", hub.attachments.len()));
            }
        });
    });
    ingest_drops(ctx, hub, drop);
    ingest_paste(ctx, hub);
}

fn ingest_drops(ctx: &egui::Context, hub: &mut FeedbackHub, rect: Rect) {
    if !ctx.input(|i| i.pointer.any_pressed()) {
        return;
    }
    let pos = ctx.input(|i| i.pointer.interact_pos());
    if pos.is_none_or(|p| !rect.contains(p)) {
        return;
    }
    for file in ctx.input(|i| i.raw.dropped_files.clone()) {
        if let Some(path) = file.path {
            if let Ok(bytes) = std::fs::read(&path) {
                if image_bytes_ok(&bytes) {
                    hub.attachments.push((path, bytes));
                }
            }
        }
    }
}

fn ingest_paste(_ctx: &egui::Context, _hub: &mut FeedbackHub) {}

fn poll_screenshot(ctx: &egui::Context, hub: &mut FeedbackHub) {
    if !hub.screenshot_pending {
        return;
    }
    ctx.input(|i| {
        for event in &i.raw.events {
            if let egui::Event::Screenshot { image, .. } = event {
                if let Some(bytes) = color_image_to_png(image.as_ref()) {
                    hub.attachments
                        .push((PathBuf::from("window-capture.png"), bytes));
                }
                hub.screenshot_pending = false;
            }
        }
    });
}

fn image_bytes_ok(bytes: &[u8]) -> bool {
    image::guess_format(bytes).is_ok()
}

fn color_image_to_png(img: &egui::ColorImage) -> Option<Vec<u8>> {
    let [w, h] = img.size;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in &img.pixels {
        rgba.extend_from_slice(&px.to_array());
    }
    let buf = image::RgbaImage::from_raw(w as u32, h as u32, rgba)?;
    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut out);
    image::DynamicImage::ImageRgba8(buf)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

fn submit(hub: &mut FeedbackHub, app_name: &str, version: &str) -> Result<PathBuf, String> {
    if hub.description.trim().is_empty() {
        return Err("Add a short description first.".into());
    }
    let kind = hub.kind.ok_or("Missing report kind")?;
    let reports_dir = hub
        .prefs
        .reports_dir
        .clone()
        .unwrap_or_else(bundle::default_reports_dir);
    std::fs::create_dir_all(&reports_dir).map_err(|e| e.to_string())?;
    let links: Vec<String> = hub
        .links_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    let report = ReportJson {
        app: app_name.to_string(),
        version: version.to_string(),
        git_hash: bundle::git_hash_short(),
        os: std::env::consts::OS.to_string(),
        kind: match kind {
            FeedbackKind::Bug => "bug",
            FeedbackKind::Feature => "feature",
        }
        .into(),
        description: hub.description.trim().to_string(),
        links,
        steps: hub.recorded_steps.clone(),
        reproduce: hub.reproduce_wanted,
        created_at: chrono::Local::now().to_rfc3339(),
        bundle_dir: String::new(),
    };
    let dir = bundle::write_bundle(&reports_dir, &report, &hub.attachments)?;
    Ok(dir)
}

fn open_mailto(subject: &str, body: &str) {
    let subject = urlencoding(subject);
    let body = urlencoding(body);
    let url = format!("mailto:?subject={subject}&body={body}");
    open_url(&url);
}

fn urlencoding(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn open_url(url: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(url)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

pub fn advanced_section(ui: &mut egui::Ui, prefs: &mut FeedbackPrefs, sub: Color32) {
    ui.label(RichText::new("Suggestion box").small().strong());
    ui.label(
        RichText::new(
            "Bug and feature reports are saved as folders on disk (no network). \
             Default: feedback/client in a dev checkout, otherwise LocalAppData.",
        )
        .small()
        .color(sub),
    );
    let mut dir_text = prefs
        .reports_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut dir_text)
            .hint_text(bundle::default_reports_dir().display().to_string())
            .desired_width(f32::INFINITY),
    );
    if ui.small_button("Use default folder").clicked() {
        prefs.reports_dir = None;
    }
    if ui.small_button("Apply path").clicked() {
        let trimmed = dir_text.trim();
        prefs.reports_dir = if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        };
    }
}
