//! Off-thread writes for text-language-model linked `response.txt` files.
//! The frame loop enqueues; a bounded worker performs filesystem I/O.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};
use slate_doc::scene::NodeId;
use slate_doc::ItemId;

const QUEUE_DEPTH: usize = 32;
const STREAM_FLUSH: Duration = Duration::from_millis(500);

struct WriteJob {
    node: NodeId,
    gen: u64,
    item: ItemId,
    path: PathBuf,
    text: String,
}

struct WriteDone {
    node: NodeId,
    gen: u64,
    item: ItemId,
    ok: bool,
}

struct Slot {
    gen: u64,
    item: ItemId,
    path: PathBuf,
    policy: TextOutputPolicy,
    deferred: Option<String>,
}

/// Change-only / throttled flush policy (tested without I/O).
#[derive(Clone, Debug, Default)]
pub(crate) struct TextOutputPolicy {
    pub last_written: String,
    pub last_flush: Option<Instant>,
}

impl TextOutputPolicy {
    pub fn should_flush(&self, now: Instant, text: &str, streaming: bool) -> bool {
        if text == self.last_written {
            return false;
        }
        if !streaming {
            return true;
        }
        self.last_flush
            .map(|t| now.duration_since(t) >= STREAM_FLUSH)
            .unwrap_or(true)
    }

    pub fn record_flush(&mut self, now: Instant, text: String) {
        self.last_written = text;
        self.last_flush = Some(now);
    }
}

#[derive(Default)]
pub(crate) struct AgentTextOutputWriter {
    jobs: Option<SyncSender<WriteJob>>,
    results: Option<Receiver<WriteDone>>,
    slots: HashMap<NodeId, Slot>,
}

impl AgentTextOutputWriter {
    fn ensure_worker(&mut self) {
        if self.jobs.is_some() {
            return;
        }
        let (job_tx, job_rx) = mpsc::sync_channel::<WriteJob>(QUEUE_DEPTH);
        let (done_tx, done_rx) = crossbeam_channel::bounded(QUEUE_DEPTH);
        thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                if let Some(parent) = job.path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let ok = std::fs::write(&job.path, &job.text).is_ok();
                if done_tx
                    .send(WriteDone {
                        node: job.node,
                        gen: job.gen,
                        item: job.item,
                        ok,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.jobs = Some(job_tx);
        self.results = Some(done_rx);
    }

    fn slot_mut(&mut self, id: NodeId, item: ItemId, path: PathBuf) -> &mut Slot {
        self.slots.entry(id).or_insert_with(|| Slot {
            gen: 1,
            item,
            path: path.clone(),
            policy: TextOutputPolicy::default(),
            deferred: None,
        });
        let slot = self.slots.get_mut(&id).unwrap();
        if slot.item != item || slot.path != path {
            slot.gen = slot.gen.wrapping_add(1);
            slot.item = item;
            slot.path = path;
            slot.policy = TextOutputPolicy::default();
            slot.deferred = None;
        }
        slot
    }

    fn try_send(&mut self, id: NodeId, gen: u64, item: ItemId, path: PathBuf, text: String) {
        self.ensure_worker();
        let job = WriteJob {
            node: id,
            gen,
            item,
            path,
            text,
        };
        if let Some(tx) = &self.jobs {
            let _ = tx.try_send(job);
        }
    }

    /// A new run or empty reset: bump generation and queue a clear write.
    pub(crate) fn begin_run(&mut self, id: NodeId, item: ItemId, path: PathBuf) {
        let gen = {
            let slot = self.slot_mut(id, item, path.clone());
            slot.gen = slot.gen.wrapping_add(1);
            slot.policy = TextOutputPolicy::default();
            slot.deferred = None;
            slot.gen
        };
        self.try_send(id, gen, item, path, String::new());
    }

    pub(crate) fn write_now(&mut self, id: NodeId, item: ItemId, path: PathBuf, text: String) {
        let now = Instant::now();
        let gen = {
            let slot = self.slot_mut(id, item, path.clone());
            slot.deferred = None;
            slot.policy.record_flush(now, text.clone());
            slot.gen
        };
        self.try_send(id, gen, item, path, text);
    }

    /// Mirror streaming assistant text into the linked file (frame loop only enqueues).
    pub(crate) fn sync(
        &mut self,
        id: NodeId,
        item: ItemId,
        path: &Path,
        text: &str,
        streaming: bool,
    ) {
        let now = Instant::now();
        let path = path.to_path_buf();
        let (gen, flush) = {
            let slot = self.slot_mut(id, item, path.clone());
            let flush = slot.policy.should_flush(now, text, streaming);
            if flush {
                slot.deferred = None;
                slot.policy.record_flush(now, text.to_string());
            } else {
                slot.deferred = Some(text.to_string());
            }
            (slot.gen, flush)
        };
        if flush {
            self.try_send(id, gen, item, path, text.to_string());
        }
    }

    pub(crate) fn retire(&mut self, ids: &[NodeId]) {
        for id in ids {
            if let Some(slot) = self.slots.remove(id) {
                let _ = slot.gen.wrapping_add(1);
            }
        }
    }

    /// Drain completed writes; returns item ids whose snippet cache should reload.
    pub(crate) fn pump(&mut self) -> Vec<ItemId> {
        let Some(rx) = &self.results else {
            return Vec::new();
        };
        let mut stale = Vec::new();
        let mut follow_ups = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(done) => {
                    let Some(slot) = self.slots.get(&done.node) else {
                        continue;
                    };
                    if done.gen != slot.gen || !done.ok {
                        continue;
                    }
                    stale.push(done.item);
                    if let Some(slot) = self.slots.get(&done.node) {
                        let Some(deferred) = slot.deferred.clone() else {
                            continue;
                        };
                        if deferred == slot.policy.last_written {
                            continue;
                        }
                        let now = Instant::now();
                        if slot.policy.should_flush(now, &deferred, true) {
                            follow_ups.push((
                                done.node,
                                slot.gen,
                                slot.item,
                                slot.path.clone(),
                                deferred,
                                now,
                            ));
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.results = None;
                    self.jobs = None;
                    break;
                }
            }
        }
        for (node, gen, item, path, text, now) in follow_ups {
            if let Some(slot) = self.slots.get_mut(&node) {
                slot.policy.record_flush(now, text.clone());
                slot.deferred = None;
            }
            self.try_send(node, gen, item, path, text);
        }
        stale
    }

    /// Whether the linked document already has authored bytes (snippet cache only).
    pub(crate) fn file_has_content(
        &self,
        id: NodeId,
        snippets: &HashMap<ItemId, Option<String>>,
    ) -> bool {
        let Some(slot) = self.slots.get(&id) else {
            return false;
        };
        snippets
            .get(&slot.item)
            .and_then(|s| s.as_ref())
            .is_some_and(|t| !t.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_unchanged_text() {
        let mut p = TextOutputPolicy::default();
        let t0 = Instant::now();
        assert!(p.should_flush(t0, "hello", true));
        p.record_flush(t0, "hello".into());
        assert!(!p.should_flush(t0, "hello", true));
        assert!(!p.should_flush(t0, "hello", false));
    }

    #[test]
    fn throttles_while_streaming() {
        let mut p = TextOutputPolicy::default();
        let t0 = Instant::now();
        p.record_flush(t0, "a".into());
        assert!(!p.should_flush(t0 + Duration::from_millis(100), "ab", true));
        assert!(p.should_flush(t0 + Duration::from_millis(500), "ab", true));
    }

    #[test]
    fn flushes_final_when_not_streaming() {
        let mut p = TextOutputPolicy::default();
        let t0 = Instant::now();
        p.record_flush(t0, "partial".into());
        assert!(p.should_flush(t0 + Duration::from_millis(50), "partial done", false));
    }
}
