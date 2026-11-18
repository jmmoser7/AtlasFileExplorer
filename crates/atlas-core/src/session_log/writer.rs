//! A single ordered writer for frame diagnostics. Pending work is bounded
//! even if the storage device temporarily stops accepting writes.

use super::{Line, JSONL_ROTATE, PENDING_CAP};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Default)]
pub(super) struct Batch {
    pub lines: Vec<Line>,
    pub latest: Option<String>,
}

impl Batch {
    pub(super) fn write(&self, app: &str, log: &Path, latest: &Path) {
        if !self.lines.is_empty() {
            if std::fs::metadata(log).is_ok_and(|meta| meta.len() > JSONL_ROTATE) {
                let _ = std::fs::rename(log, log.with_file_name(format!("{app}-prev.jsonl")));
            }
            let mut buf = Vec::new();
            for line in &self.lines {
                if serde_json::to_writer(&mut buf, line).is_ok() {
                    buf.push(b'\n');
                }
            }
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log)
            {
                let _ = file.write_all(&buf);
            }
        }
        if let Some(json) = &self.latest {
            // Two running Slate windows can publish to the same latest path.
            // Each writer must finish its own temporary file before replacing it.
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let serial = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let tmp = latest.with_extension(format!("json.{}.{serial}.tmp", std::process::id()));
            if std::fs::write(&tmp, json)
                .and_then(|()| std::fs::rename(&tmp, latest))
                .is_err()
            {
                let _ = std::fs::remove_file(tmp);
            }
        }
    }
}

#[derive(Default)]
struct Queue {
    batch: Batch,
    requested: u64,
    finished: u64,
    stop: bool,
}

pub(super) struct Writer {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    pub(super) fn spawn(app: &'static str, log: PathBuf, latest: PathBuf) -> Option<Self> {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let worker = Arc::clone(&queue);
        let thread = thread::Builder::new()
            .name(format!("{app}-session-log"))
            .spawn(move || {
                let (lock, wake) = &*worker;
                let mut state = lock.lock().unwrap();
                loop {
                    state = wake
                        .wait_while(state, |s| !s.stop && s.requested == s.finished)
                        .unwrap();
                    if state.requested == state.finished {
                        return;
                    }
                    let batch = std::mem::take(&mut state.batch);
                    let generation = state.requested;
                    drop(state);
                    batch.write(app, &log, &latest);
                    state = lock.lock().unwrap();
                    state.finished = generation;
                    wake.notify_all();
                }
            })
            .ok()?;
        Some(Self {
            queue,
            thread: Some(thread),
        })
    }

    pub(super) fn submit(&self, batch: Batch, wait: bool) {
        let (lock, wake) = &*self.queue;
        let mut state = lock.lock().unwrap();
        // A mark or shutdown already waits for durability. Make room by
        // draining accepted events before appending its final batch.
        if wait && state.batch.lines.len() + batch.lines.len() > PENDING_CAP {
            let accepted = state.requested;
            state = wake.wait_while(state, |s| s.finished < accepted).unwrap();
        }
        let excess = (state.batch.lines.len() + batch.lines.len()).saturating_sub(PENDING_CAP);
        state.batch.lines.drain(..excess);
        state.batch.lines.extend(batch.lines);
        if batch.latest.is_some() {
            state.batch.latest = batch.latest;
        }
        state.requested += 1;
        let generation = state.requested;
        wake.notify_all();
        if wait {
            drop(wake.wait_while(state, |s| s.finished < generation).unwrap());
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap().stop = true;
        wake.notify_all();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_log::SessionLog;

    #[test]
    fn stalled_storage_keeps_the_newest_frames_and_checkpoint_in_bounded_space() {
        // Hold the writer still while several frame batches arrive.
        let writer = Writer {
            queue: Arc::new((Mutex::new(Queue::default()), Condvar::new())),
            thread: None,
        };
        let log = SessionLog::memory("blocked-storage");
        let line = log.inner.lock().unwrap().base_line();
        for n in 0..(PENDING_CAP * 2) {
            let mut sample = line.clone();
            sample.n = n as u32;
            writer.submit(
                Batch {
                    lines: vec![sample],
                    latest: Some(n.to_string()),
                },
                false,
            );
        }
        let state = writer.queue.0.lock().unwrap();
        assert_eq!(state.batch.lines.len(), PENDING_CAP);
        assert_eq!(state.batch.lines[0].n, PENDING_CAP as u32);
        let newest = (PENDING_CAP * 2 - 1) as u32;
        assert_eq!(state.batch.lines.last().unwrap().n, newest);
        assert_eq!(state.batch.latest, Some(newest.to_string()));
    }
}
