//! Local recovery copies, including untitled workbooks. Never writes the source.
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use eframe::egui;
use serde::{Deserialize, Serialize};
use slate_doc::recovery::RecoveryEntry;

use super::SlateApp;
use schedule::Schedule;
use worker::{Event, Job, Worker};

mod discovery;
mod folder;
mod schedule;
mod ui;
mod worker;
pub(super) use ui::recovery_settings_ui;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoveryPreferences {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub retained_versions: usize,
}

impl Default for RecoveryPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 10,
            retained_versions: 5,
        }
    }
}

impl RecoveryPreferences {
    pub fn clamped(mut self) -> Self {
        self.clamp();
        self
    }

    pub fn clamp(&mut self) {
        self.interval_seconds = self.interval_seconds.clamp(5, 300);
        self.retained_versions = self.retained_versions.clamp(1, 10);
    }
}

#[derive(Default)]
pub(super) struct RecoveryRuntime {
    folder: folder::FolderAccess,
    worker: Option<Worker>,
    scanner: Option<discovery::Scanner>,
    scan_pending: bool,
    scan_retry_at: Option<Instant>,
    writer_ready: bool,
    schedule: Schedule,
    busy: usize,
    retry_at: Option<Instant>,
    last_error_toast: Option<Instant>,
    status: Option<String>,
    failed_keys: HashMap<u64, String>,
    last_saved: HashMap<u64, Instant>,
    entries: Vec<RecoveryEntry>,
    issues: Vec<String>,
    window_open: bool,
    confirm_discard: Option<String>,
    stopping: bool,
    root: Option<PathBuf>,
    #[cfg(test)]
    active_in_tests: bool,
    #[cfg(test)]
    skip_discovery_in_tests: bool,
}

impl RecoveryRuntime {
    fn send(&mut self, job: Job) -> bool {
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.jobs.send(job).is_ok())
        {
            self.busy += 1;
            true
        } else {
            false
        }
    }

    fn fail(&mut self, message: String) -> Option<String> {
        self.status = Some(message.clone());
        let now = Instant::now();
        if self
            .last_error_toast
            .is_none_or(|at| now.duration_since(at) >= Duration::from_secs(30))
        {
            self.last_error_toast = Some(now);
            Some(format!("Recovery: {message}"))
        } else {
            None
        }
    }

    fn disconnected(&mut self) {
        self.schedule.interrupted(Instant::now());
        self.worker = None;
        self.writer_ready = false;
        self.busy = 0;
        self.retry_at = Some(Instant::now() + Duration::from_secs(30));
    }

    fn start(&mut self, ctx: &egui::Context) -> Option<String> {
        #[cfg(test)]
        if !self.active_in_tests {
            return None;
        }
        let discovery_message = self.start_discovery(ctx);
        if self.worker.is_some()
            || self.stopping
            || self.retry_at.is_some_and(|at| Instant::now() < at)
        {
            return discovery_message;
        }
        match Worker::start(self.root.clone(), ctx.clone()) {
            Ok(worker) => {
                if self.folder.path.get().is_none() {
                    self.folder.path = worker.folder.clone();
                }
                self.worker = Some(worker);
                self.busy = 1;
                discovery_message
            }
            Err(error) => {
                self.disconnected();
                self.request_discovery();
                self.fail(format!("Could not start recovery worker: {error}"))
            }
        }
    }

    fn next_event(&mut self) -> Option<Event> {
        match self.worker.as_ref().map(|worker| worker.events.try_recv()) {
            Some(Ok(event)) => {
                self.busy = self.busy.saturating_sub(1);
                return Some(event);
            }
            Some(Err(TryRecvError::Disconnected)) => {
                self.disconnected();
                return Some(Event::Fatal(
                    "Recovery worker disconnected; existing copies are preserved".into(),
                ));
            }
            _ => {}
        }
        match self.scanner.as_ref()?.events.try_recv() {
            Ok(event) => {
                self.scan_pending = false;
                if matches!(event, Event::Failed(_)) {
                    self.scan_retry_at = Some(Instant::now() + Duration::from_secs(30));
                }
                Some(event)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.scanner = None;
                self.scan_pending = false;
                self.scan_retry_at = Some(Instant::now() + Duration::from_secs(30));
                Some(Event::Failed(
                    "Recovery discovery disconnected; copies are preserved".into(),
                ))
            }
        }
    }

    fn request_discovery(&mut self) {
        if !self.scan_pending {
            self.scan_pending = self
                .scanner
                .as_ref()
                .is_some_and(|scanner| scanner.request());
        }
    }

    fn start_discovery(&mut self, ctx: &egui::Context) -> Option<String> {
        #[cfg(test)]
        if self.skip_discovery_in_tests {
            return None;
        }
        if self.stopping || self.scan_retry_at.is_some_and(|at| Instant::now() < at) {
            return None;
        }
        let retrying = self.scan_retry_at.take().is_some();
        let created = self.scanner.is_none();
        if created {
            match discovery::Scanner::start(self.root.clone(), ctx.clone()) {
                Ok(scanner) => self.scanner = Some(scanner),
                Err(error) => {
                    self.scan_retry_at = Some(Instant::now() + Duration::from_secs(30));
                    return self.fail(format!("Could not start recovery discovery: {error}"));
                }
            }
        }
        if retrying || (created && self.writer_ready) {
            self.request_discovery();
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn for_test(root: PathBuf) -> Self {
        Self {
            root: Some(root),
            active_in_tests: true,
            ..Self::default()
        }
    }
}

impl SlateApp {
    pub(super) fn recovery_frame(&mut self, ctx: &egui::Context) {
        if let Some(message) = self.recovery.start(ctx) {
            self.toast(message);
        }
        for _ in 0..8 {
            let Some(event) = self.recovery.next_event() else {
                break;
            };
            match event {
                Event::Ready => {
                    self.recovery.writer_ready = true;
                    self.recovery.status = self.recovery.failed_keys.values().next().cloned();
                    self.recovery.request_discovery();
                }
                Event::Scan { entries, issues } => {
                    self.recovery.status = self.recovery.failed_keys.values().next().cloned();
                    self.recovery.window_open |= !entries.is_empty() || !issues.is_empty();
                    self.recovery.entries = entries;
                    self.recovery.issues = issues;
                }
                Event::Saved(key) => {
                    self.recovery.schedule.completed(key, true, Instant::now());
                    if !self.recovery.schedule.contains_key(key) {
                        continue;
                    }
                    if self.recovery.failed_keys.remove(&key).is_some() {
                        self.recovery.status = self.recovery.failed_keys.values().next().cloned();
                    }
                    self.recovery.last_saved.insert(key, Instant::now());
                }
                Event::SaveFailed(key, error) => {
                    self.recovery.schedule.completed(key, false, Instant::now());
                    if !self.recovery.schedule.contains_key(key) {
                        continue;
                    }
                    self.recovery.failed_keys.insert(key, error.clone());
                    if let Some(message) = self.recovery.fail(error) {
                        self.toast(message);
                    }
                }
                Event::Loaded(doc) => {
                    self.install_recovered_doc(*doc);
                }
                Event::Discarded(entry) => {
                    self.recovery
                        .entries
                        .retain(|e| entry_id(e) != entry_id(&entry));
                    self.toast("Recovery copies discarded");
                }
                Event::Done => {}
                Event::Failed(error) => {
                    if let Some(message) = self.recovery.fail(error) {
                        self.toast(message);
                    }
                }
                Event::Fatal(error) => {
                    self.recovery.disconnected();
                    self.recovery.request_discovery();
                    if let Some(message) = self.recovery.fail(error) {
                        self.toast(message);
                    }
                }
            }
        }
        if let Some(Err(error)) = self.recovery.folder.poll() {
            if let Some(message) = self
                .recovery
                .fail(format!("Could not open recovery folder: {error}"))
            {
                self.toast(message);
            }
        }
        let now = Instant::now();
        let active_has_draft = self.recovery_has_text_drafts();
        for (index, tab) in self.tabs.iter().enumerate() {
            self.recovery.schedule.observe(
                tab.id,
                tab.dirty || (index == self.active_tab && active_has_draft),
                now,
            );
        }
        if self.settings.recovery.enabled
            && self.recovery.worker.is_some()
            && self.recovery.busy == 0
        {
            let ids: Vec<_> = self.tabs.iter().map(|tab| tab.id).collect();
            let interval =
                Duration::from_secs(self.settings.recovery.interval_seconds.clamp(5, 300));
            if let Some((index, key)) = self.recovery.schedule.next(&ids, now, interval) {
                let tab = &self.tabs[index];
                let mut doc = tab.doc.clone();
                doc.view.cam_x = tab.cam.offset.x;
                doc.view.cam_y = tab.cam.offset.y;
                doc.view.zoom = tab.cam.z;
                let id = tab.id;
                if index == self.active_tab {
                    self.capture_recovery_text(&mut doc);
                }
                let job = Job::Save {
                    key,
                    original: tab.path.clone(),
                    doc: Box::new(doc),
                    keep: self.settings.recovery.retained_versions.clamp(1, 10),
                };
                if self.recovery.send(job) {
                    self.recovery.schedule.captured(id, now);
                }
            }
        }
        // Recovery deadlines must also advance while the mouse is idle.
        if !self.recovery.stopping {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        ui::window(self, ctx);
    }

    /// Call before a tab is removed or its document is replaced.
    pub(super) fn recovery_forget_tab(&mut self, tab_id: u64) {
        if let Some(key) = self.recovery.schedule.forget(tab_id) {
            self.recovery.last_saved.remove(&key);
            if self.recovery.failed_keys.remove(&key).is_some() {
                self.recovery.status = self.recovery.failed_keys.values().next().cloned();
            }
            self.recovery.send(Job::Forget(key));
        }
    }

    /// Local storage only; bounded exit wait never touches source files.
    pub(super) fn recovery_shutdown(&mut self) {
        let active_has_draft = self.recovery_has_text_drafts();
        let clean: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(index, tab)| !tab.dirty && !(*index == self.active_tab && active_has_draft))
            .map(|(_, tab)| tab.id)
            .collect();
        for id in clean {
            self.recovery_forget_tab(id);
        }
        self.recovery.stopping = true;
        self.recovery.scanner = None;
        if let Some(worker) = self.recovery.worker.take() {
            let _ = worker.jobs.send(Job::Stop);
            let _ = worker.finished.recv_timeout(Duration::from_secs(2));
        }
    }

    pub(super) fn dispatch_recovery(
        &mut self,
        ctx: &egui::Context,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        if let Some(message) = self.recovery.start(ctx) {
            self.toast(message);
        }
        match command {
            "app.recovery.open" => self.open_recovery(),
            "app.recovery.folder" => self.open_recovery_folder(),
            "app.recovery.restore" => self.recover_snapshot(detail),
            "app.recovery.discard" => self.discard_recovery(detail),
            "app.recovery.configure" => self.configure_recovery(detail),
            _ => false,
        }
    }

    pub(super) fn open_recovery(&mut self) -> bool {
        self.recovery.window_open = true;
        self.recovery.request_discovery();
        true
    }

    pub(super) fn open_recovery_folder(&mut self) -> bool {
        match self.recovery.folder.reveal(self.recovery.root.clone()) {
            Ok(()) => true,
            Err(error) => {
                if let Some(message) = self
                    .recovery
                    .fail(format!("Could not open recovery folder: {error}"))
                {
                    self.toast(message);
                }
                false
            }
        }
    }

    pub(super) fn recover_snapshot(&mut self, detail: Option<&str>) -> bool {
        if self.recovery.busy != 0 || self.recovery.scan_pending {
            return false;
        }
        let Some(entry) = self
            .recovery
            .entries
            .iter()
            .find(|entry| Some(entry_id(entry).as_str()) == detail)
            .cloned()
        else {
            return false;
        };
        self.recovery.send(Job::Load(entry))
    }

    pub(super) fn discard_recovery(&mut self, detail: Option<&str>) -> bool {
        let Some(detail) = detail else { return false };
        if let Some(id) = detail.strip_prefix("confirm:") {
            if self.recovery.confirm_discard.as_deref() != Some(id)
                || self.recovery.busy != 0
                || self.recovery.scan_pending
            {
                return false;
            }
            let Some(entry) = self
                .recovery
                .entries
                .iter()
                .find(|entry| entry_id(entry) == id)
                .cloned()
            else {
                return false;
            };
            self.recovery.confirm_discard = None;
            return self.recovery.send(Job::Discard(entry));
        }
        if self
            .recovery
            .entries
            .iter()
            .any(|entry| entry_id(entry) == detail)
        {
            self.recovery.confirm_discard = Some(detail.into());
            true
        } else {
            false
        }
    }

    pub(super) fn configure_recovery(&mut self, detail: Option<&str>) -> bool {
        let Some(mut prefs) =
            detail.and_then(|value| serde_json::from_str::<RecoveryPreferences>(value).ok())
        else {
            return false;
        };
        prefs.clamp();
        self.settings.recovery = prefs;
        self.settings.save();
        true
    }
}

fn entry_id(entry: &RecoveryEntry) -> String {
    format!("{}/{}", entry.session, entry.tab)
}

#[cfg(test)]
mod tests;
