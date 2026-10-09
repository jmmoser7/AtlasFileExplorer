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
    form_return: FeedbackPhase,
}

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
            form_return: FeedbackPhase::BugForm,
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

    pub fn on_command(&mut self, id: &str, detail: Option<&str>) {
        if let Some(rec) = self.recorder.as_mut() {
            rec.record_command(id, detail);
        }
    }

    pub fn start_recording(&mut self, _session_log: &atlas_core::session_log::SessionLog) {
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
