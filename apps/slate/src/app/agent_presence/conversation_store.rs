//! Machine-local conversation I/O. The frame loop never opens transcript files.
use super::conversation::{Conversation, ConversationChange};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReceiptConsumer {
    Default,
    BoardMentions,
}

pub(super) struct ConversationStore {
    threads: HashMap<String, Conversation>,
    ready: bool,
    error: Option<String>,
    dirty: HashSet<String>,
    next_save: Instant,
    tx: Option<mpsc::SyncSender<PendingSave>>,
    rx: Option<mpsc::Receiver<StoreEvent>>,
    worker: Option<std::thread::JoinHandle<()>>,
    pending: HashMap<String, PendingSave>,
    receipt: u64,
    saved: Vec<u64>,
    failed: Vec<u64>,
    outstanding: HashSet<u64>,
    consumers: HashMap<u64, ReceiptConsumer>,
}
struct PendingSave {
    id: String,
    changes: Vec<ConversationChange>,
    receipts: Vec<u64>,
}
enum StoreEvent {
    Loaded(HashMap<String, Conversation>, Option<String>),
    Failed { receipts: Vec<u64>, error: String },
    Saved(Vec<u64>),
}
impl Default for ConversationStore {
    fn default() -> Self {
        let mut store = Self {
            threads: HashMap::new(),
            ready: cfg!(test),
            error: None,
            dirty: HashSet::new(),
            next_save: Instant::now(),
            tx: None,
            rx: None,
            worker: None,
            pending: HashMap::new(),
            receipt: 0,
            saved: Vec::new(),
            failed: Vec::new(),
            outstanding: HashSet::new(),
            consumers: HashMap::new(),
        };
        if !cfg!(test) {
            let dir = atlas_core::index::data_dir().join("presence-conversations");
            store.connect(dir);
        }
        store
    }
}
impl ConversationStore {
    fn connect(&mut self, dir: PathBuf) {
        let (tx, commands) = mpsc::sync_channel(16);
        let (events, rx) = mpsc::channel();
        self.worker = Some(std::thread::spawn(move || run(dir, commands, events)));
        self.tx = Some(tx);
        self.rx = Some(rx);
    }
    #[cfg(test)]
    pub(super) fn at_directory(dir: PathBuf) -> Self {
        let mut store = Self::default();
        store.ready = false;
        store.connect(dir);
        store
    }
    pub(super) fn get(&self, id: &str) -> Option<&Conversation> {
        self.threads.get(id)
    }
    pub(super) fn edit(&mut self, id: &str) -> Result<&mut Conversation, String> {
        self.poll();
        if !self.ready {
            return Err("Loading previous messages; please wait".into());
        }
        Ok(self.threads.entry(id.into()).or_default())
    }
    pub(super) fn status(&self) -> Option<String> {
        if !self.ready {
            Some("Loading previous messages…".into())
        } else {
            self.error.clone()
        }
    }
    pub(super) fn dirty(&mut self, id: &str) {
        if self.dirty.is_empty() {
            self.next_save = Instant::now() + Duration::from_millis(600);
        }
        self.dirty.insert(id.into());
    }
    pub(super) fn save(&mut self, id: &str) {
        self.enqueue(id, 0);
    }
    pub(super) fn save_with_receipt(&mut self, id: &str) -> u64 {
        self.save_with_receipt_for(id, ReceiptConsumer::Default)
            .expect("default receipts retain the existing caller budget")
    }
    /// Ownership is installed before enqueue can synchronously fail. Canceled
    /// claims still count until their terminal event is drained by their owner.
    pub(super) fn save_with_receipt_for(
        &mut self,
        id: &str,
        consumer: ReceiptConsumer,
    ) -> Result<u64, String> {
        if consumer == ReceiptConsumer::BoardMentions
            && self.consumers.values().filter(|c| **c == consumer).count() >= 64
        {
            return Err("Board message persistence queue is full".into());
        }
        self.receipt = self.receipt.wrapping_add(1).max(1);
        self.consumers.insert(self.receipt, consumer);
        self.outstanding.insert(self.receipt);
        self.enqueue(id, self.receipt);
        Ok(self.receipt)
    }
    pub(super) fn take_receipts(&mut self) -> Vec<u64> {
        self.take_receipts_for(ReceiptConsumer::Default, false)
    }
    pub(super) fn take_failed_receipts(&mut self) -> Vec<u64> {
        self.take_receipts_for(ReceiptConsumer::Default, true)
    }
    pub(super) fn take_receipts_for(
        &mut self,
        consumer: ReceiptConsumer,
        failed: bool,
    ) -> Vec<u64> {
        let events = if failed {
            &mut self.failed
        } else {
            &mut self.saved
        };
        let mut owned = Vec::new();
        events.retain(|id| {
            if self.consumers.get(id) == Some(&consumer) {
                self.consumers.remove(id);
                owned.push(*id);
                false
            } else {
                true
            }
        });
        owned
    }
    fn enqueue(&mut self, id: &str, receipt: u64) {
        self.dirty.remove(id);
        if self.tx.is_none() {
            if receipt != 0 && self.outstanding.remove(&receipt) {
                self.failed.push(receipt);
                self.error = Some("Conversation persistence is unavailable".into());
            }
            return;
        }
        if let Some(thread) = self.threads.get_mut(id) {
            let pending = self
                .pending
                .entry(id.into())
                .or_insert_with(|| PendingSave {
                    id: id.into(),
                    changes: Vec::new(),
                    receipts: Vec::new(),
                });
            pending.changes.extend(thread.take_changes());
            if receipt != 0 {
                pending.receipts.push(receipt);
            }
        } else if receipt != 0 && self.outstanding.remove(&receipt) {
            self.failed.push(receipt);
        }
        self.flush();
    }
    fn flush(&mut self) {
        let Some(tx) = &self.tx else {
            return;
        };
        let ids: Vec<_> = self.pending.keys().cloned().take(16).collect();
        for id in ids {
            let pending = self.pending.remove(&id).unwrap();
            match tx.try_send(pending) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(pending)) => {
                    self.pending.insert(id, pending);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_pending)) => {
                    self.failed.extend(self.outstanding.drain());
                    self.error =
                        Some("Conversation storage stopped; messages remain in this window".into());
                }
            }
        }
    }
    pub(super) fn poll(&mut self) {
        for _ in 0..32 {
            let event = match self.rx.as_ref().map(|rx| rx.try_recv()) {
                Some(Ok(event)) => event,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    self.failed.extend(self.outstanding.drain());
                    self.error =
                        Some("Conversation storage stopped; messages remain in this window".into());
                    break;
                }
                _ => break,
            };
            match event {
                StoreEvent::Loaded(threads, error) => {
                    self.threads = threads;
                    self.error = error;
                    self.ready = true;
                }
                StoreEvent::Failed { receipts, error } => {
                    self.failed.extend(
                        receipts
                            .into_iter()
                            .filter(|id| self.outstanding.remove(id)),
                    );
                    self.error = Some(error);
                }
                StoreEvent::Saved(receipts) => {
                    self.saved.extend(
                        receipts
                            .into_iter()
                            .filter(|id| self.outstanding.remove(id)),
                    );
                    if self
                        .error
                        .as_ref()
                        .is_some_and(|e| e.starts_with("Could not save conversation:"))
                    {
                        self.error = None;
                    }
                }
            }
        }
        self.flush();
        if Instant::now() >= self.next_save {
            let dirty: Vec<_> = self.dirty.drain().collect();
            for id in dirty {
                self.save(&id);
            }
        }
    }
}
impl Drop for ConversationStore {
    fn drop(&mut self) {
        let dirty: Vec<_> = self.dirty.drain().collect();
        for id in dirty {
            self.save(&id);
        }
        if let Some(tx) = &self.tx {
            for (_, pending) in self.pending.drain() {
                let _ = tx.send(pending);
            }
        }
        self.tx.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredConversation {
    id: String,
    conversation: Conversation,
}
#[derive(serde::Serialize)]
struct StoredConversationRef<'a> {
    id: &'a str,
    conversation: &'a Conversation,
}
fn file_name(id: &str) -> String {
    // An opaque participant ID is never interpreted as a filesystem path.
    let mut name = String::with_capacity(id.len() * 2 + 5);
    use std::fmt::Write;
    for byte in id.bytes() {
        let _ = write!(name, "{byte:02x}");
    }
    name.push_str(".json");
    name
}
fn load(dir: &std::path::Path) -> (HashMap<String, Conversation>, Option<String>) {
    let mut threads = HashMap::new();
    let mut error = None;
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (threads, None),
        Err(e) => return (threads, Some(format!("Could not load conversations: {e}"))),
    };
    for entry in entries
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|x| x == "json"))
    {
        match std::fs::read(entry.path()).and_then(|bytes| {
            serde_json::from_slice::<StoredConversation>(&bytes).map_err(std::io::Error::other)
        }) {
            Ok(mut stored) => {
                // Presence and authority are never restored. An interrupted
                // stream remains a readable stopped message after a crash.
                if stored.id.starts_with("human:") {
                    if stored.conversation.draft.is_empty() {
                        if let Some(turn) = stored.conversation.turns.iter().rev().find(|t| {
                            t.role == "user" && matches!(t.status.as_str(), "pending" | "accepted")
                        }) {
                            stored.conversation.draft = turn.text.clone();
                        }
                    }
                    for turn in stored
                        .conversation
                        .turns
                        .iter_mut()
                        .filter(|t| t.role == "user")
                    {
                        if matches!(turn.status.as_str(), "pending" | "accepted") {
                            turn.status = "failed".into();
                        }
                    }
                }
                let mut interrupted = Vec::new();
                for turn in &mut stored.conversation.turns {
                    if turn.board_origin.is_some()
                        && matches!(turn.status.as_str(), "queued" | "dispatching")
                    {
                        interrupted.push(if turn.status == "dispatching" {
                            "Board message delivery is uncertain after restart; it was not retried."
                        } else { "Queued board message was not sent after restart; send a new message to resume." });
                        turn.status = "not-sent".into();
                    }
                    if matches!(turn.status.as_str(), "streaming" | "sent") {
                        turn.status = "stopped".into();
                    }
                }
                for reason in interrupted {
                    stored
                        .conversation
                        .push("system", reason, "Slate", None, "", "complete");
                }
                stored.conversation.rebuild_runtime();
                threads.insert(stored.id, stored.conversation);
            }
            Err(e) => {
                error = Some(format!(
                    "A conversation could not be loaded; its file was kept: {e}"
                ))
            }
        }
    }
    (threads, error)
}
fn run(dir: PathBuf, rx: mpsc::Receiver<PendingSave>, tx: mpsc::Sender<StoreEvent>) {
    let (mut threads, error) = load(&dir);
    // This one initial mirror copy runs on the worker. Later saves carry only
    // the mutation journal; the frame loop never clones retained history.
    if tx.send(StoreEvent::Loaded(threads.clone(), error)).is_err() {
        return;
    }
    while let Ok(first) = rx.recv() {
        let mut pending = HashMap::from([(first.id.clone(), first)]);
        for update in rx.try_iter().take(32) {
            let previous = pending
                .entry(update.id.clone())
                .or_insert_with(|| PendingSave {
                    id: update.id.clone(),
                    changes: Vec::new(),
                    receipts: Vec::new(),
                });
            previous.changes.extend(update.changes);
            previous.receipts.extend(update.receipts);
        }
        for (id, update) in pending {
            let conversation = threads.entry(id.clone()).or_default();
            conversation.apply_changes(update.changes);
            let result = std::fs::create_dir_all(&dir).and_then(|()| {
                atlas_ai::agent::atomic_write_json(
                    &dir.join(file_name(&id)),
                    &StoredConversationRef {
                        id: &id,
                        conversation,
                    },
                )
            });
            if let Err(error) = result {
                let _ = tx.send(StoreEvent::Failed {
                    receipts: update.receipts,
                    error: format!("Could not save conversation: {error}"),
                });
            } else {
                let _ = tx.send(StoreEvent::Saved(update.receipts));
            }
        }
    }
}

#[cfg(test)]
#[path = "conversation_store_durability_tests.rs"]
mod durability_tests;
#[cfg(test)]
#[path = "conversation_store_tests.rs"]
mod tests;
