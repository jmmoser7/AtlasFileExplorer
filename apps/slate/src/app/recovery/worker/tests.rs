use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "slate-recovery-worker-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }
    fn start(&self) -> Worker {
        let worker =
            Worker::start(Some(self.root.clone()), eframe::egui::Context::default()).unwrap();
        assert!(matches!(event(&worker), Event::Ready));
        worker
    }
    fn store(&self) -> RecoveryStore {
        RecoveryStore::new(self.root.clone())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn event(worker: &Worker) -> Event {
    worker.events.recv_timeout(Duration::from_secs(10)).unwrap()
}
fn stop(worker: Worker) {
    worker.jobs.send(Job::Stop).unwrap();
    worker
        .finished
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
}
fn snapshot(worker: &Worker, key: u64, name: &str) {
    worker
        .jobs
        .send(Job::Save {
            key,
            original: None,
            doc: Box::new(SlateDoc::new(name)),
            keep: 5,
        })
        .unwrap();
}

#[test]
fn unsaved_workbook_round_trips_through_worker_and_is_hidden_while_live() {
    let fixture = Fixture::new();
    let worker = fixture.start();
    snapshot(&worker, 1, "Never saved");
    assert!(matches!(event(&worker), Event::Saved(_)));
    assert!(fixture.store().scan().unwrap().entries.is_empty());
    stop(worker);
    let scan = fixture.store().scan().unwrap();
    assert_eq!(scan.entries.len(), 1);
    assert!(scan.entries[0].original_path.is_none());
    assert_eq!(
        fixture.store().load(&scan.entries[0]).unwrap().name,
        "Never saved"
    );
}

#[test]
fn queued_write_then_forget_cannot_resurrect_a_replaced_or_discarded_tab() {
    let fixture = Fixture::new();
    let worker = fixture.start();
    snapshot(&worker, 4, "Old document");
    worker.jobs.send(Job::Forget(4)).unwrap();
    snapshot(&worker, 5, "Replacement");
    assert!(matches!(event(&worker), Event::Saved(_)));
    assert!(matches!(event(&worker), Event::Done));
    assert!(matches!(event(&worker), Event::Saved(_)));
    stop(worker);
    let entries = fixture.store().scan().unwrap().entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tab, 5);
}

#[test]
fn unchanged_dirty_snapshot_does_not_use_up_retained_versions() {
    let fixture = Fixture::new();
    let worker = fixture.start();
    for _ in 0..3 {
        snapshot(&worker, 1, "Same document");
        assert!(matches!(event(&worker), Event::Saved(_)));
    }
    stop(worker);
    let snapshots = std::fs::read_dir(&fixture.root)
        .unwrap()
        .flat_map(|entry| std::fs::read_dir(entry.unwrap().path()).unwrap())
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "json")
        })
        .count();
    assert_eq!(snapshots, 1);
}

#[test]
fn changed_original_path_is_not_deduplicated_after_save_as() {
    let fixture = Fixture::new();
    let worker = fixture.start();
    let mut hashes = HashMap::new();
    let store = fixture.store();
    let session = store.begin_session().unwrap();
    let doc = SlateDoc::new("Save as");
    save(
        &session,
        &mut hashes,
        1,
        Some(fixture.root.join("first.slate")),
        &doc,
        5,
    )
    .unwrap();
    save(
        &session,
        &mut hashes,
        1,
        Some(fixture.root.join("second.slate")),
        &doc,
        5,
    )
    .unwrap();
    drop(session);
    stop(worker);
    let entries = store.scan().unwrap().entries;
    assert_eq!(
        entries[0].original_path,
        Some(fixture.root.join("second.slate"))
    );
}

#[test]
fn recovery_never_overwrites_original_workbook_bytes() {
    let fixture = Fixture::new();
    let original = fixture.root.join("original.slate");
    std::fs::write(&original, b"original contents").unwrap();
    let worker = fixture.start();
    worker
        .jobs
        .send(Job::Save {
            key: 1,
            original: Some(original.clone()),
            doc: Box::new(SlateDoc::new("Unsaved edits")),
            keep: 5,
        })
        .unwrap();
    assert!(matches!(event(&worker), Event::Saved(_)));
    stop(worker);
    assert_eq!(std::fs::read(original).unwrap(), b"original contents");
}

#[test]
fn storage_start_failure_is_reported_without_touching_existing_files() {
    let fixture = Fixture::new();
    let bad = fixture.root.join("not-a-directory");
    std::fs::write(&bad, b"preserve me").unwrap();
    let worker = Worker::start(Some(bad.clone()), eframe::egui::Context::default()).unwrap();
    assert!(matches!(event(&worker), Event::Fatal(_)));
    worker
        .finished
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(std::fs::read(bad).unwrap(), b"preserve me");
}
