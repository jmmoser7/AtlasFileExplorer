//! One independent scanner: historical decoding never delays the next capture.
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};

use eframe::egui;
use slate_doc::recovery::RecoveryStore;

use super::worker::Event;

pub(super) struct Scanner {
    requests: SyncSender<()>,
    pub events: Receiver<Event>,
}

impl Scanner {
    pub fn start(root: Option<PathBuf>, ctx: egui::Context) -> io::Result<Self> {
        Self::spawn(ctx, move || {
            let root = super::folder::resolve_root(root.clone());
            match RecoveryStore::new(root).scan() {
                Ok(scan) => Event::Scan {
                    entries: scan.entries,
                    issues: scan.issues,
                },
                Err(error) => Event::Failed(format!("Could not discover recovery copies: {error}")),
            }
        })
    }

    fn spawn(
        ctx: egui::Context,
        mut scan: impl FnMut() -> Event + Send + 'static,
    ) -> io::Result<Self> {
        let (requests, rx) = mpsc::sync_channel(1);
        let (tx, events) = mpsc::channel();
        std::thread::Builder::new()
            .name("slate-recovery-discovery".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(&mut scan));
                    let event = result.unwrap_or_else(|_| {
                        Event::Failed(
                            "Recovery discovery stopped unexpectedly; copies are preserved".into(),
                        )
                    });
                    if tx.send(event).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })?;
        Ok(Self { requests, events })
    }

    pub fn request(&self) -> bool {
        matches!(
            self.requests.try_send(()),
            Ok(()) | Err(TrySendError::Full(()))
        )
    }

    #[cfg(test)]
    pub(super) fn test_pair() -> (Self, mpsc::Sender<Event>, Receiver<()>) {
        let (requests, rx) = mpsc::sync_channel(1);
        let (tx, events) = mpsc::channel();
        (Self { requests, events }, tx, rx)
    }
}

#[cfg(test)]
mod tests {
    use super::super::worker::{Job, Worker};
    use super::*;
    use std::time::Duration;

    #[test]
    fn blocked_historical_discovery_cannot_delay_first_dirty_snapshot() {
        let root = std::env::temp_dir().join(format!(
            "slate-scan-independent-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (entered, started) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let scanner = Scanner::spawn(egui::Context::default(), move || {
            entered.send(()).unwrap();
            gate.recv().unwrap();
            Event::Scan {
                entries: Vec::new(),
                issues: Vec::new(),
            }
        })
        .unwrap();
        assert!(scanner.request());
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let worker = Worker::start(Some(root.clone()), egui::Context::default()).unwrap();
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(5)).unwrap(),
            Event::Ready
        ));
        worker
            .jobs
            .send(Job::Save {
                key: 1,
                original: None,
                doc: Box::new(slate_doc::SlateDoc::new("protected before discovery")),
                keep: 5,
            })
            .unwrap();
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(5)).unwrap(),
            Event::Saved(1)
        ));
        release.send(()).unwrap();
        worker.jobs.send(Job::Stop).unwrap();
        worker
            .finished
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        drop(scanner);
        assert_eq!(
            RecoveryStore::new(root.clone())
                .scan()
                .unwrap()
                .entries
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
