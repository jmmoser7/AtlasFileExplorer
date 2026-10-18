use super::bundle::{self, ReportJson};
use super::{FeedbackHub, FeedbackKind, FeedbackPhase, FeedbackPrefs};
use crate::theme::Palette;
use eframe::egui::{self, Color32, CornerRadius, Id, Rect, RichText, Stroke, StrokeKind, Vec2};
use std::path::PathBuf;

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
        match hub.recorder.as_mut() {
            Some(rec) => {
                if rec.stall_poll_due() {
                    rec.ingest_stalls(&session_log.last_stalls());
                }
                if rec.timed_out() {
                    hub.finish_recording();
                    hub.phase = hub.form_after_recording();
                    return out;
                }
                ctx.request_repaint_after(rec.remaining());
            }
            None => hub.phase = hub.form_after_recording(),
        }
        // Enter belongs to a text field or board text editor while one holds
        // the keyboard. A field that just committed on this Enter released
        // focus earlier in the frame, so last frame's state counts too.
        let editing_now = ctx.wants_keyboard_input() || hub.app_text_editing;
        let editing = editing_now || hub.editing_last_frame;
        hub.editing_last_frame = editing_now;
        if !editing && ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.any()) {
            hub.finish_recording();
            hub.phase = hub.form_after_recording();
            out.command = Some("app.feedback.finish_recording");
        }
        return out;
    }

    poll_screenshot(ctx, hub);

    super::panels::show(ctx, palette, hub, app_name, version)
}

/// The capture arrives as an event; its PNG encode runs on a worker so a
/// large window never stalls the frame.
fn poll_screenshot(ctx: &egui::Context, hub: &mut FeedbackHub) {
    if let Some(rx) = &hub.capture_rx {
        match rx.try_recv() {
            Ok(png) => {
                if let Some(bytes) = png {
                    hub.attachments
                        .push((PathBuf::from("window-capture.png"), bytes));
                } else {
                    hub.attach_note = Some("The window capture could not be encoded.".into());
                }
                hub.capture_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => hub.capture_rx = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }
    if !hub.screenshot_pending {
        return;
    }
    let image = ctx.input(|i| {
        i.raw.events.iter().find_map(|event| match event {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(image) = image {
        hub.screenshot_pending = false;
        let (tx, rx) = std::sync::mpsc::channel();
        hub.capture_rx = Some(rx);
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(color_image_to_png(&image));
            repaint.request_repaint();
        });
    }
}

fn color_image_to_png(img: &egui::ColorImage) -> Option<Vec<u8>> {
    let [w, h] = img.size;
    let rgba: Vec<u8> = img.pixels.iter().flat_map(|px| px.to_array()).collect();
    atlas_core::clipboard_image::encode_png(w as u32, h as u32, &rgba)
}

pub(super) fn submit(
    hub: &mut FeedbackHub,
    app_name: &str,
    version: &str,
) -> Result<PathBuf, String> {
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

/// Mail clients and the shell cap a mailto URL; the bundle holds the rest.
const MAILTO_BODY_CHARS: usize = 1500;

pub(super) fn open_mailto(subject: &str, body: &str) {
    let subject = urlencoding(subject);
    let body = urlencoding(&super::truncate(body, MAILTO_BODY_CHARS));
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
        // explorer hands the URL to the mailto handler without a cmd.exe
        // parse, so `%` and `&` in the encoded body stay literal.
        let _ = std::process::Command::new("explorer").arg(url).spawn();
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
    let saved = prefs
        .reports_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let buf_id = ui.id().with("feedback_reports_dir");
    let mut dir_text = ui
        .data(|d| d.get_temp::<String>(buf_id))
        .unwrap_or_else(|| saved.clone());
    ui.add(
        egui::TextEdit::singleline(&mut dir_text)
            .hint_text(bundle::default_reports_dir().display().to_string())
            .desired_width(f32::INFINITY),
    );
    if ui.small_button("Use default folder").clicked() {
        prefs.reports_dir = None;
        dir_text.clear();
    }
    if ui.small_button("Apply path").clicked() {
        let trimmed = dir_text.trim();
        prefs.reports_dir = if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        };
    }
    ui.data_mut(|d| d.insert_temp(buf_id, dir_text));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &egui::Context, hub: &mut FeedbackHub, enter: bool, field: bool) {
        let log = atlas_core::session_log::SessionLog::memory("feedback-test");
        let mut input = egui::RawInput::default();
        if enter {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let _ = ctx.run(input, |ctx| {
            if field {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let mut text = String::new();
                    let id = Id::new("feedback-test-field");
                    ui.add(egui::TextEdit::singleline(&mut text).id(id));
                    ui.memory_mut(|m| m.request_focus(id));
                });
            }
            dialogs(ctx, &Palette::dark(), hub, "test", "0", &log);
        });
    }

    fn recording_hub() -> FeedbackHub {
        let mut hub = FeedbackHub::default();
        hub.begin_form(FeedbackKind::Bug);
        hub.start_recording();
        hub
    }

    #[test]
    fn enter_in_a_focused_field_does_not_end_the_recording() {
        let ctx = egui::Context::default();
        let mut hub = recording_hub();
        for _ in 0..3 {
            frame(&ctx, &mut hub, false, true);
        }
        frame(&ctx, &mut hub, true, true);
        assert!(hub.recording());
    }

    #[test]
    fn enter_in_the_apps_own_text_editor_does_not_end_the_recording() {
        let ctx = egui::Context::default();
        let mut hub = recording_hub();
        hub.app_text_editing = true;
        frame(&ctx, &mut hub, false, false);
        // The editor committed on this Enter and closed earlier in the frame.
        hub.app_text_editing = false;
        frame(&ctx, &mut hub, true, false);
        assert!(hub.recording());
    }

    #[test]
    fn enter_with_nothing_edited_ends_the_recording() {
        let ctx = egui::Context::default();
        let mut hub = recording_hub();
        frame(&ctx, &mut hub, false, false);
        frame(&ctx, &mut hub, true, false);
        assert!(!hub.recording());
        assert_eq!(hub.phase, FeedbackPhase::BugForm);
    }
}
