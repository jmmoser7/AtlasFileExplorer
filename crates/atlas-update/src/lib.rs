//! One owner for installed-suite updates. All network and package work runs
//! on workers; the shell paints this state and apps decide when closing is safe.
//! Development builds are inert. No credentials or document paths enter a feed.

use std::sync::mpsc::{self, Receiver};

#[cfg(windows)]
mod windows;

pub const RELEASES_URL: &str = "https://github.com/jmmoser7/AtlasFileExplorer/releases";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Uninstalled,
    Checking,
    Current,
    Available { version: String, notes: String },
    Downloading,
    Ready { version: String },
    Scheduling,
    Scheduled,
    Failed(String),
}

pub struct Updater {
    pub state: State,
    pub visible: bool,
    pub version: String,
    pub channel: String,
    rx: Option<Receiver<Event>>,
    started: bool,
    #[cfg(windows)]
    release: Option<windows::Release>,
}

enum Event {
    #[cfg(windows)]
    Checked(String, String, Option<windows::Release>),
    #[cfg(windows)]
    Downloaded,
    #[cfg(windows)]
    Scheduled,
    #[cfg(windows)]
    ApplyFailed(String),
    Failed(String),
}

impl Default for Updater {
    fn default() -> Self {
        Self {
            state: State::Uninstalled,
            visible: false,
            version: option_env!("ATLAS_RELEASE_VERSION")
                .unwrap_or(env!("CARGO_PKG_VERSION"))
                .into(),
            channel: option_env!("ATLAS_RELEASE_CHANNEL")
                .unwrap_or("development")
                .into(),
            rx: None,
            started: false,
            #[cfg(windows)]
            release: None,
        }
    }
}

impl Updater {
    /// Call from the standalone window only (never a hosted Atlas viewport).
    pub fn poll(&mut self) {
        if !self.started {
            self.started = true;
            self.check(false);
        }
        let event = self.rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let Some(event) = event else { return };
        self.rx = None;
        match event {
            #[cfg(windows)]
            Event::Checked(version, channel, release) => {
                self.version = version;
                self.channel = channel;
                self.state = match &release {
                    Some(r) => State::Available {
                        version: r.info.TargetFullRelease.Version.clone(),
                        notes: r.info.TargetFullRelease.NotesMarkdown.clone(),
                    },
                    None => State::Current,
                };
                self.visible |= release.is_some();
                self.release = release;
            }
            #[cfg(windows)]
            Event::Downloaded => {
                if let Some(r) = &self.release {
                    self.state = State::Ready {
                        version: r.info.TargetFullRelease.Version.clone(),
                    };
                    self.visible = true;
                }
            }
            #[cfg(windows)]
            Event::Scheduled => self.state = State::Scheduled,
            #[cfg(windows)]
            Event::ApplyFailed(error) => {
                self.state = State::Failed(error);
                self.visible = true;
            }
            // A failed automatic check stays quiet; manual checks show errors.
            Event::Failed(error) => self.state = State::Failed(error),
        }
    }

    pub fn busy(&self) -> bool {
        self.rx.is_some()
    }

    pub fn check(&mut self, manual: bool) {
        self.started = true;
        self.visible |= manual;
        if self.busy() || matches!(self.state, State::Scheduled) {
            return;
        }
        #[cfg(windows)]
        if windows::installed_root().is_some() {
            self.state = State::Checking;
            self.release = None;
            self.work(windows::check);
        }
    }

    pub fn download(&mut self) {
        #[cfg(windows)]
        if !self.busy() && matches!(self.state, State::Available { .. }) {
            if let Some(release) = self.release.clone() {
                self.state = State::Downloading;
                self.work(move || {
                    release
                        .manager
                        .download_updates(&release.info, None)
                        .map_err(|e| e.to_string())?;
                    Ok(Event::Downloaded)
                });
            }
        }
    }

    /// The helper waits for EVERY running suite process to exit voluntarily.
    /// It never closes another window and keeps new instances out during apply.
    pub fn install(&mut self) {
        #[cfg(windows)]
        if !self.busy() && matches!(self.state, State::Ready { .. }) {
            if let Some(release) = self.release.clone() {
                self.state = State::Scheduling;
                self.work(move || windows::schedule(&release));
            }
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn work(&mut self, operation: impl FnOnce() -> Result<Event, String> + Send + 'static) {
        let (tx, rx) = mpsc::channel();
        self.rx = Some(rx);
        std::thread::spawn(move || {
            let event = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
                .unwrap_or_else(|_| {
                    Err("The update worker stopped unexpectedly. Please try again.".into())
                })
                .unwrap_or_else(Event::Failed);
            let _ = tx.send(event);
        });
    }
}

/// Must run before argument parsing and window creation. Keep the returned
/// handle alive until process exit; dropping it permits suite installation.
pub fn startup() -> Result<Option<std::fs::File>, String> {
    #[cfg(windows)]
    {
        windows::startup()
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_auto_check_is_quiet_but_manual_error_is_visible() {
        for manual in [false, true] {
            let mut updater = Updater {
                started: true,
                visible: manual,
                ..Default::default()
            };
            let (tx, rx) = mpsc::channel();
            updater.rx = Some(rx);
            tx.send(Event::Failed("offline".into())).unwrap();
            updater.poll();
            assert_eq!(updater.state, State::Failed("offline".into()));
            assert_eq!(updater.visible, manual);
            assert!(!updater.busy());
        }
    }

    #[test]
    fn scheduled_update_cannot_be_replaced_by_a_check() {
        let mut updater = Updater {
            state: State::Scheduled,
            ..Default::default()
        };
        updater.check(true);
        assert_eq!(updater.state, State::Scheduled);
    }
}
