//! Machine-local conversation I/O. The frame loop never opens transcript files.
use super::conversation::{Conversation, ConversationChange};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

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
        self.receipt = self.receipt.wrapping_add(1).max(1);
        self.outstanding.insert(self.receipt);
        self.enqueue(id, self.receipt);
        self.receipt
    }
    pub(super) fn take_receipts(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.saved)
    }
    pub(super) fn take_failed_receipts(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.failed)
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
                for turn in &mut stored.conversation.turns {
                    if matches!(turn.status.as_str(), "streaming" | "sent") {
                        turn.status = "stopped".into();
                    }
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
mod tests {
    use super::*;
    #[test]
    fn a_full_worker_queue_coalesces_only_deltas_and_preserves_every_receipt_in_order() {
        let mut store = ConversationStore::default();
        let (tx, rx) = mpsc::sync_channel(1);
        store.tx = Some(tx);
        let conversation = store.edit("self").unwrap();
        for index in 0..1000 {
            conversation.push(
                "user",
                "Retained",
                "Jamie",
                None,
                &format!("old-{index}"),
                "complete",
            );
        }
        store.save("self"); // fills the bounded queue; the next two saves coalesce
        store
            .edit("self")
            .unwrap()
            .push("user", "New", "Jamie", None, "new", "pending");
        let first = store.save_with_receipt("self");
        store
            .edit("self")
            .unwrap()
            .set_turn_status(1000, "accepted");
        let second = store.save_with_receipt("self");
        let baseline = rx.try_recv().unwrap();
        let mut mirror = Conversation::default();
        mirror.apply_changes(baseline.changes);
        store.flush();
        let update = rx.try_recv().unwrap();
        assert_eq!(update.receipts, vec![first, second]);
        assert_eq!(update.changes.len(), 2);
        assert!(matches!(&update.changes[0],ConversationChange::Append(turn) if turn.text=="New"));
        assert!(
            matches!(&update.changes[1],ConversationChange::Status {index:1000,status} if status=="accepted")
        );
        mirror.apply_changes(update.changes);
        assert_eq!(&mirror, store.get("self").unwrap());
        assert!(
            store.take_receipts().is_empty(),
            "a queued update is never a durable receipt"
        );
    }
    #[test]
    fn disconnected_storage_never_acknowledges_later_saves_as_memory_only_success() {
        let mut store = ConversationStore::default();
        let (tx, rx) = mpsc::sync_channel(1);
        store.tx = Some(tx);
        drop(rx);
        store.edit("human:peer").unwrap().push(
            "user",
            "Keep this",
            "Jamie",
            None,
            "one",
            "pending",
        );
        let first = store.save_with_receipt("human:peer");
        assert_eq!(store.take_failed_receipts(), vec![first]);
        store.tx.take();
        store
            .edit("human:peer")
            .unwrap()
            .set_draft("Still available");
        let second = store.save_with_receipt("human:peer");
        assert_eq!(store.take_failed_receipts(), vec![second]);
        assert!(store.take_receipts().is_empty());
        assert_eq!(store.get("human:peer").unwrap().turns[0].text, "Keep this");
        assert_eq!(store.get("human:peer").unwrap().draft, "Still available");
    }
    fn wait_ready(store: &mut ConversationStore) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !store.ready {
            store.poll();
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    #[test]
    fn persistence_receipt_follows_atomic_write_and_restart_keeps_visible_history_and_draft() {
        let dir =
            std::env::temp_dir().join(format!("slate-conversation-{}", atlas_agent::request_id()));
        let mut store = ConversationStore::at_directory(dir.clone());
        wait_ready(&mut store);
        let c = store.edit("self").unwrap();
        c.push("user", "First", "Jamie", None, "request", "complete");
        c.set_draft("unfinished");
        let one = store.save_with_receipt("self");
        store.edit("self").unwrap().push(
            "assistant",
            "Partial",
            "Dot",
            Some("local"),
            "stream",
            "streaming",
        );
        let two = store.save_with_receipt("self");
        let mut receipts = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !receipts.contains(&one) || !receipts.contains(&two) {
            store.poll();
            receipts.extend(store.take_receipts());
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let saved: StoredConversation =
            serde_json::from_slice(&std::fs::read(dir.join(file_name("self"))).unwrap()).unwrap();
        assert_eq!(saved.conversation.turns.len(), 2);
        assert_eq!(saved.conversation.draft, "unfinished");
        drop(store);
        let mut restored = ConversationStore::at_directory(dir.clone());
        wait_ready(&mut restored);
        let c = restored.get("self").unwrap();
        assert_eq!(c.turns[0].status, "complete");
        assert_eq!(c.turns[1].status, "stopped");
        assert_eq!(c.draft, "unfinished");
        drop(restored);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn opaque_identity_never_becomes_a_relative_path() {
        let name = file_name("../../human:example");
        assert!(name.ends_with(".json"));
        assert!(!name.contains('/'));
        assert!(!name.contains('\\'));
    }
}
