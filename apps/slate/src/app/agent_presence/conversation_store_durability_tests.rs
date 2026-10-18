use super::*;
fn ready(store: &mut ConversationStore) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !store.ready {
        store.poll();
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn saved(store: &mut ConversationStore, expected: &[u64]) {
    let mut receipts = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while expected.iter().any(|id| !receipts.contains(id)) {
        store.poll();
        receipts.extend(store.take_receipts());
        assert!(store.take_failed_receipts().is_empty());
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
#[test]
fn actual_write_failure_reports_every_owned_receipt_without_durable_ack_or_losing_text() {
    let base =
        std::env::temp_dir().join(format!("slate-store-failure-{}", atlas_agent::request_id()));
    std::fs::create_dir_all(&base).unwrap();
    let blocked = base.join("conversation-directory");
    // A real file occupies the required directory. Both read_dir and atomic
    // persistence fail naturally on every supported OS; no synthetic error event.
    std::fs::write(&blocked, b"Keep this file").unwrap();
    let mut store = ConversationStore::at_directory(blocked.clone());
    ready(&mut store);
    let thread = store.edit("human:peer").unwrap();
    thread.push(
        "user",
        "Unsent content",
        "Jamie",
        None,
        "message-one",
        "pending",
    );
    let first = store.save_with_receipt("human:peer");
    store
        .edit("human:peer")
        .unwrap()
        .set_draft("Keep this draft");
    let second = store.save_with_receipt("human:peer");
    let mut failed = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !failed.contains(&first) || !failed.contains(&second) {
        store.poll();
        failed.extend(store.take_failed_receipts());
        assert!(store.take_receipts().is_empty());
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(store
        .status()
        .unwrap()
        .contains("Could not save conversation"));
    assert_eq!(
        store.get("human:peer").unwrap().turns[0].text,
        "Unsent content"
    );
    assert_eq!(store.get("human:peer").unwrap().draft, "Keep this draft");
    assert_eq!(std::fs::read(&blocked).unwrap(), b"Keep this file");
    drop(store);
    std::fs::remove_file(blocked).unwrap();
    std::fs::remove_dir(base).unwrap();
}
#[test]
fn interrupted_human_deliveries_restart_as_retryable_with_same_ids_and_preserved_drafts() {
    let dir = std::env::temp_dir().join(format!(
        "slate-human-recovery-{}",
        atlas_agent::request_id()
    ));
    let mut store = ConversationStore::at_directory(dir.clone());
    ready(&mut store);
    let c = store.edit("human:peer").unwrap();
    c.push(
        "user",
        "Pending message",
        "Jamie",
        None,
        "pending-id",
        "pending",
    );
    c.push(
        "user",
        "Accepted message",
        "Jamie",
        None,
        "accepted-id",
        "accepted",
    );
    c.push(
        "user",
        "Delivered message",
        "Jamie",
        None,
        "delivered-id",
        "delivered",
    );
    let ids: Vec<_> = c.turns.iter().map(|t| t.id.clone()).collect();
    let first = store.save_with_receipt("human:peer");
    let c = store.edit("human:with-draft").unwrap();
    c.push(
        "user",
        "Interrupted message",
        "Jamie",
        None,
        "other-id",
        "pending",
    );
    c.set_draft("Human's newer draft");
    let second = store.save_with_receipt("human:with-draft");
    saved(&mut store, &[first, second]);
    drop(store);
    let mut restored = ConversationStore::at_directory(dir.clone());
    ready(&mut restored);
    let c = restored.get("human:peer").unwrap();
    assert_eq!(c.request_index("pending-id"), Some(0));
    assert_eq!(c.request_index("accepted-id"), Some(1));
    assert_eq!(c.request_index("delivered-id"), Some(2));
    assert_eq!(
        c.turns.iter().map(|t| t.id.clone()).collect::<Vec<_>>(),
        ids
    );
    assert_eq!(
        c.turns
            .iter()
            .map(|t| t.status.as_str())
            .collect::<Vec<_>>(),
        vec!["failed", "failed", "delivered"]
    );
    assert_eq!(c.draft, "Accepted message");
    assert_eq!(
        restored.get("human:with-draft").unwrap().draft,
        "Human's newer draft"
    );
    drop(restored);
    std::fs::remove_dir_all(dir).unwrap();
}
