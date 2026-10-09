//! Shared suggestion box: local report bundles, optional step recording.

mod bundle;
mod recorder;
mod ui;

pub use bundle::{default_reports_dir, repo_root_from, repo_root_from_exe, ReportJson};
pub use recorder::RecordedStep;
pub use recorder::{redact_paths_in_text, Recorder, RECORD_TIMEOUT};
pub use ui::{
    advanced_section, dialogs, paint_recording_chrome, suggestion_button, FeedbackUiOutput,
};

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FeedbackPrefs {
    #[serde(default)]
    pub reports_dir: Option<PathBuf>,
}

impl FeedbackPrefs {
    fn path(app_key: &str) -> PathBuf {
        atlas_core::index::data_dir().join(format!("{app_key}-feedback.json"))
    }

    pub fn load(app_key: &str) -> Self {
        std::fs::read_to_string(Self::path(app_key))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app_key: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let path = Self::path(app_key);
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackKind {
    Bug,
    Feature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackPhase {
    Closed,
    KindPicker,
    BugForm,
    FeatureForm,
    Recording,
}

pub struct FeedbackHub {
    pub prefs: FeedbackPrefs,
    pub phase: FeedbackPhase,
    pub kind: Option<FeedbackKind>,
    pub description: String,
    pub links_text: String,
    pub reproduce_wanted: bool,
    pub recorded_steps: Vec<RecordedStep>,
    pub attachments: Vec<(PathBuf, Vec<u8>)>,
    pub(crate) recorder: Option<Recorder>,
    pub screenshot_pending: bool,
    pub last_bundle_dir: PathBuf,
    pub attach_note: Option<String>,
    /// Set by the app each frame while its own text editor (one egui does
    /// not see as a focused field) is open.
    pub app_text_editing: bool,
    pub(crate) editing_last_frame: bool,
    pub(crate) capture_rx: Option<std::sync::mpsc::Receiver<Option<Vec<u8>>>>,
    form_return: FeedbackPhase,
    dispatch_depth: u32,
}

/// Largest dropped image read into a report.
const MAX_ATTACHMENT_BYTES: u64 = 32 * 1024 * 1024;

impl Default for FeedbackHub {
    fn default() -> Self {
        Self {
            prefs: FeedbackPrefs::default(),
            phase: FeedbackPhase::Closed,
            kind: None,
            description: String::new(),
            links_text: String::new(),
            reproduce_wanted: false,
            recorded_steps: Vec::new(),
            attachments: Vec::new(),
            recorder: None,
            screenshot_pending: false,
            last_bundle_dir: PathBuf::new(),
            attach_note: None,
            app_text_editing: false,
            editing_last_frame: false,
            capture_rx: None,
            form_return: FeedbackPhase::BugForm,
            dispatch_depth: 0,
        }
    }
}

impl FeedbackHub {
    pub fn open_picker(&mut self) {
        self.phase = FeedbackPhase::KindPicker;
    }

    pub fn begin_form(&mut self, kind: FeedbackKind) {
        self.kind = Some(kind);
        self.phase = match kind {
            FeedbackKind::Bug => FeedbackPhase::BugForm,
            FeedbackKind::Feature => FeedbackPhase::FeatureForm,
        };
    }

    pub fn recording(&self) -> bool {
        self.phase == FeedbackPhase::Recording
    }

    /// Called by the app's single command dispatch entry. Nested dispatches
    /// (a command that dispatches another) record only the outer id.
    pub fn enter_command(&mut self, id: &str) {
        if self.dispatch_depth == 0 {
            if let Some(rec) = self.recorder.as_mut() {
                rec.record_command(id);
            }
        }
        self.dispatch_depth += 1;
    }

    pub fn exit_command(&mut self) {
        self.dispatch_depth = self.dispatch_depth.saturating_sub(1);
    }

    /// History entries that did not come through dispatch (direct gestures
    /// on the canvas). Inside a dispatch the outer command already counts.
    pub fn on_history(&mut self, id: &str) {
        if self.dispatch_depth == 0 {
            if let Some(rec) = self.recorder.as_mut() {
                rec.record_command(id);
            }
        }
    }

    /// True while a report form is open: file drops belong to the form, not
    /// to the canvas underneath.
    pub fn owns_drops(&self) -> bool {
        matches!(
            self.phase,
            FeedbackPhase::BugForm | FeedbackPhase::FeatureForm
        )
    }

    /// Attach dropped image files. A cloud placeholder is refused before
    /// any byte is read, so a drop never hydrates a OneDrive file.
    pub fn attach_dropped(&mut self, paths: &[PathBuf]) {
        for path in paths {
            if self.attachments.iter().any(|(p, _)| p == path) {
                continue;
            }
            if atlas_core::cloud::is_dehydrated(path) {
                self.attach_note =
                    Some("Cloud-only file skipped — make it available offline first.".into());
                continue;
            }
            let fits = std::fs::metadata(path)
                .is_ok_and(|m| m.is_file() && m.len() <= MAX_ATTACHMENT_BYTES);
            match fits.then(|| std::fs::read(path).ok()).flatten() {
                Some(bytes) if image::guess_format(&bytes).is_ok() => {
                    self.attachments.push((path.clone(), bytes));
                    self.attach_note = None;
                }
                _ => {
                    self.attach_note = Some("Only image files up to 32 MB can be attached.".into())
                }
            }
        }
    }

    /// Attach the clipboard picture through the shared clipboard-image owner.
    pub fn attach_clipboard_image(&mut self) {
        match atlas_core::clipboard_image::read_png() {
            Some(png) => {
                let n = self.attachments.len() + 1;
                self.attachments
                    .push((PathBuf::from(format!("pasted-{n}.png")), png));
                self.attach_note = None;
            }
            None => self.attach_note = Some("The clipboard holds no image.".into()),
        }
    }

    pub fn start_recording(&mut self) {
        self.form_return = self.phase;
        self.phase = FeedbackPhase::Recording;
        self.recorder = Some(Recorder::start(now_ms()));
    }

    pub fn finish_recording(&mut self) {
        if let Some(rec) = self.recorder.take() {
            self.recorded_steps = rec.steps;
        }
    }

    pub fn form_after_recording(&self) -> FeedbackPhase {
        self.form_return
    }

    pub fn request_screenshot(&mut self, ctx: &eframe::egui::Context) {
        self.screenshot_pending = true;
        ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Screenshot(Default::default()));
    }

    pub fn reset_after_submit(&mut self) {
        self.phase = FeedbackPhase::Closed;
        self.description.clear();
        self.links_text.clear();
        self.reproduce_wanted = false;
        self.recorded_steps.clear();
        self.attachments.clear();
        self.attach_note = None;
        self.kind = None;
    }

    pub fn mailto_subject(&self, app_name: &str) -> String {
        let kind = self
            .kind
            .map(|k| match k {
                FeedbackKind::Bug => "Bug",
                FeedbackKind::Feature => "Feature",
            })
            .unwrap_or("Feedback");
        format!("{app_name} {kind}: {}", truncate(&self.description, 60))
    }

    pub fn mailto_summary(&self, app_name: &str, version: &str) -> Option<String> {
        if self.description.trim().is_empty() {
            return None;
        }
        Some(format!(
            "{app_name} {version}\n\n{}\n\nReport folder (attach screenshots from here):\n{}",
            self.description.trim(),
            self.last_bundle_dir.display()
        ))
    }

    pub fn set_last_bundle(&mut self, dir: PathBuf) {
        self.last_bundle_dir = dir;
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(hub: &FeedbackHub) -> Vec<String> {
        hub.recorder
            .as_ref()
            .unwrap()
            .steps
            .iter()
            .map(|s| s.label.clone())
            .collect()
    }

    #[test]
    fn a_dispatched_command_records_once_even_when_it_pushes_history() {
        let mut hub = FeedbackHub::default();
        hub.begin_form(FeedbackKind::Bug);
        hub.start_recording();
        hub.enter_command("board.align.left");
        hub.on_history("board.align.left");
        hub.enter_command("board.nested");
        hub.exit_command();
        hub.exit_command();
        hub.on_history("board.drag");
        hub.enter_command("app.feedback.finish_recording");
        hub.exit_command();
        assert_eq!(labels(&hub), ["board.align.left", "board.drag"]);
    }

    #[test]
    fn nothing_records_outside_a_recording() {
        let mut hub = FeedbackHub::default();
        hub.enter_command("board.align.left");
        hub.exit_command();
        hub.on_history("board.drag");
        assert!(hub.recorder.is_none());
        assert_eq!(hub.dispatch_depth, 0);
    }
}
