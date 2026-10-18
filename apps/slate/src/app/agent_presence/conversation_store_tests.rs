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
    store
        .edit("human:peer")
        .unwrap()
        .push("user", "Keep this", "Jamie", None, "one", "pending");
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

#[test]
fn typed_receipts_survive_default_drains_cancellation_and_failure() {
    let mut store = ConversationStore::default();
    let (tx, commands) = mpsc::sync_channel(4);
    let (events, rx) = mpsc::channel();
    store.tx = Some(tx);
    store.rx = Some(rx);
    store
        .edit("human:peer")
        .unwrap()
        .push("user", "Human", "You", None, "h", "pending");
    store
        .edit("agent")
        .unwrap()
        .push("user", "Board", "You", None, "b", "queued");
    let human = store.save_with_receipt("human:peer");
    let board = store
        .save_with_receipt_for("agent", ReceiptConsumer::BoardMentions)
        .unwrap();
    let _ = commands.try_recv().unwrap();
    let _ = commands.try_recv().unwrap();
    // Cancellation does not release the claim before the late terminal event.
    assert_eq!(
        store.consumers.get(&board),
        Some(&ReceiptConsumer::BoardMentions)
    );
    events.send(StoreEvent::Saved(vec![human, board])).unwrap();
    store.poll();
    assert_eq!(store.take_receipts(), vec![human]);
    assert!(store.take_failed_receipts().is_empty());
    assert_eq!(
        store.take_receipts_for(ReceiptConsumer::BoardMentions, false),
        vec![board]
    );
    let late = store
        .save_with_receipt_for("agent", ReceiptConsumer::BoardMentions)
        .unwrap();
    let _ = commands.try_recv().unwrap();
    events
        .send(StoreEvent::Failed {
            receipts: vec![late],
            error: "disk failure".into(),
        })
        .unwrap();
    store.poll();
    assert!(store.take_failed_receipts().is_empty());
    assert_eq!(
        store.take_receipts_for(ReceiptConsumer::BoardMentions, true),
        vec![late]
    );
    assert!(store.consumers.is_empty());
}
#[test]
fn board_claim_budget_includes_undrained_synchronous_failures() {
    let mut store = ConversationStore::default();
    for _ in 0..64 {
        store
            .save_with_receipt_for("agent", ReceiptConsumer::BoardMentions)
            .unwrap();
    }
    assert!(store
        .save_with_receipt_for("agent", ReceiptConsumer::BoardMentions)
        .is_err());
    assert!(store.take_failed_receipts().is_empty());
    assert_eq!(
        store
            .take_receipts_for(ReceiptConsumer::BoardMentions, true)
            .len(),
        64
    );
    assert!(store
        .save_with_receipt_for("agent", ReceiptConsumer::BoardMentions)
        .is_ok());
}
