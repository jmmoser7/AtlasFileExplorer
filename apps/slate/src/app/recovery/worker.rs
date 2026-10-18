//! The only recovery I/O thread. The UI permits at most one document in flight.
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};

use slate_doc::recovery::{RecoveryEntry, RecoverySession, RecoveryStore};
use slate_doc::SlateDoc;

pub(super) enum Job {
    Save {
        key: u64,
        original: Option<PathBuf>,
        doc: Box<SlateDoc>,
        keep: usize,
    },
    Forget(u64),
    Load(RecoveryEntry),
    Discard(RecoveryEntry),
    Stop,
}

pub(super) enum Event {
    Ready,
    Scan {
        entries: Vec<RecoveryEntry>,
        issues: Vec<String>,
    },
    Saved(u64),
    SaveFailed(u64, String),
    Loaded(Box<SlateDoc>),
    Discarded(RecoveryEntry),
    Done,
    Failed(String),
    Fatal(String),
}

pub(super) struct Worker {
    pub folder: Arc<OnceLock<PathBuf>>,
    pub jobs: Sender<Job>,
    pub events: Receiver<Event>,
    pub finished: Receiver<()>,
}

impl Worker {
    pub fn start(root: Option<PathBuf>, ctx: eframe::egui::Context) -> io::Result<Self> {
        let (jobs, rx) = mpsc::channel();
        let (tx, events) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let folder = Arc::new(OnceLock::new());
        let resolved_folder = folder.clone();
        std::thread::Builder::new()
            .name("slate-recovery".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let root = super::folder::resolve_root(root);
                    let _ = resolved_folder.set(root.clone());
                    run(root, rx, &tx, &ctx)
                }));
                let message = match result {
                    Ok(Ok(())) => {
                        let _ = done.send(());
                        return;
                    }
                    Ok(Err(error)) => format!("Recovery storage unavailable: {error}"),
                    Err(_) => {
                        "The recovery worker stopped unexpectedly; existing copies are preserved"
                            .into()
                    }
                };
                let _ = tx.send(Event::Fatal(message));
                ctx.request_repaint();
                let _ = done.send(());
            })?;
        Ok(Self {
            folder,
            jobs,
            events,
            finished,
        })
    }
}

fn run(
    root: PathBuf,
    rx: Receiver<Job>,
    tx: &Sender<Event>,
    ctx: &eframe::egui::Context,
) -> io::Result<()> {
    let store = RecoveryStore::new(root);
    let session = store.begin_session()?;
    let mut fingerprints = HashMap::new();
    emit(tx, ctx, Event::Ready);
    while let Ok(job) = rx.recv() {
        let result = match job {
            Job::Save {
                key,
                original,
                doc,
                keep,
            } => Ok(save(&session, &mut fingerprints, key, original, &doc, keep)
                .unwrap_or_else(|error| Event::SaveFailed(key, error.to_string()))),
            Job::Forget(key) => {
                fingerprints.remove(&key);
                session.discard(key).map(|_| Event::Done)
            }
            Job::Load(entry) => store.load(&entry).map(|doc| Event::Loaded(Box::new(doc))),
            Job::Discard(entry) => store.discard(&entry).map(|_| Event::Discarded(entry)),
            Job::Stop => return Ok(()),
        };
        emit(
            tx,
            ctx,
            result.unwrap_or_else(|error| Event::Failed(error.to_string())),
        );
    }
    Ok(())
}

fn emit(tx: &Sender<Event>, ctx: &eframe::egui::Context, event: Event) {
    let _ = tx.send(event);
    ctx.request_repaint();
}

/// Hash serialization into a stream: no second complete serialized document in memory.
#[derive(Default)]
struct Fingerprint(DefaultHasher, u64);
impl Write for Fingerprint {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes);
        self.1 += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn save(
    session: &RecoverySession,
    fingerprints: &mut HashMap<u64, (u64, u64)>,
    key: u64,
    original: Option<PathBuf>,
    doc: &SlateDoc,
    keep: usize,
) -> io::Result<Event> {
    let mut fingerprint = Fingerprint::default();
    original.hash(&mut fingerprint.0);
    keep.hash(&mut fingerprint.0);
    serde_json::to_writer(&mut fingerprint, doc).map_err(io::Error::other)?;
    let hash = (fingerprint.0.finish(), fingerprint.1);
    if fingerprints.get(&key) == Some(&hash) {
        return Ok(Event::Saved(key));
    }
    let entry = session.write(key, original, doc, keep)?;
    if let Some(warning) = entry.warning {
        return Err(io::Error::other(warning));
    }
    fingerprints.insert(key, hash);
    Ok(Event::Saved(key))
}

pub(super) fn reveal(root: &std::path::Path) -> io::Result<()> {
    std::fs::create_dir_all(root)?;
    #[cfg(windows)]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(root).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests;
