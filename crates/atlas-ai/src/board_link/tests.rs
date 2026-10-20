use super::*;
use atlas_agent::collaborator::BoardOperation;
use std::time::{Duration, Instant};

fn fixture() -> (PathBuf, BoardLinkSession, CollaboratorGrant) {
    let dir = std::env::temp_dir().join(format!("slate-board-link-{}", atlas_agent::request_id()));
    let session = BoardLinkSession {
        session_id: "s".into(),
        grant_id: "g".into(),
        workspace_id: "w".into(),
    };
    let grant = CollaboratorGrant {
        id: "g".into(),
        workspace_id: "w".into(),
        readable_board: true,
        readable_nodes: vec![],
        readable_sources: vec![],
        commands: vec!["move".into()],
        conversation_ids: vec![],
        mutation_autonomy: false,
        helper_limit: 0,
        max_actions: 4,
    };
    (dir, session, grant)
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "board worker did not respond");
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn call(
    dir: &Path,
    session: &BoardLinkSession,
    id: &str,
    sequence: u64,
    request: &str,
) -> BoardCall {
    let call = BoardCall {
        id: id.into(),
        session: session.clone(),
        request_id: request.into(),
        sequence,
        operation: BoardOperation::Read {
            node_ids: vec![],
            offset: 0,
            limit: 32,
        },
    };
    crate::agent::atomic_write_json(&dir.join("board-calls").join(format!("{id}.json")), &call)
        .unwrap();
    call
}
#[test]
fn worker_roundtrip_is_bounded_and_revocation_removes_runtime_authority() {
    let (dir, session, grant) = fixture();
    let mut links = BoardLinks::default();
    links
        .set_session(&dir, session.clone(), grant, "r".into())
        .unwrap();
    wait(|| dir.join("board-session.json").exists());
    let sent = call(&dir, &session, "one", 0, "r");
    let mut received = Vec::new();
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });
    assert_eq!(received, vec![sent.clone()]);
    links
        .reply(
            &dir,
            BoardReply {
                id: sent.id,
                session_id: session.session_id.clone(),
                request_id: "r".into(),
                success: true,
                result: serde_json::json!({"objects":[]}),
            },
        )
        .unwrap();
    wait(|| dir.join("board-replies/one.json").exists());
    links.revoke(&dir);
    assert!(links.poll(&dir).is_empty());
    wait(|| !dir.join("board-session.json").exists());
    assert!(links
        .reply(&dir, BoardReply::error(&received[0], "late"))
        .is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn stale_requests_and_duplicate_sequences_never_reach_ui() {
    let (dir, session, grant) = fixture();
    let mut links = BoardLinks::default();
    links
        .set_session(&dir, session.clone(), grant, "new".into())
        .unwrap();
    wait(|| dir.join("board-session.json").exists());
    call(&dir, &session, "stale", 0, "old");
    wait(|| dir.join("board-replies/stale.json").exists());
    assert!(links.poll(&dir).is_empty());
    call(&dir, &session, "first", 0, "new");
    let mut received = Vec::new();
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });
    call(&dir, &session, "duplicate", 0, "new");
    wait(|| dir.join("board-replies/duplicate.json").exists());
    assert!(links.poll(&dir).is_empty());
    links.revoke(&dir);
    wait(|| !dir.join("board-session.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn changing_autonomy_keeps_the_active_call_reply_and_duplicate_fence() {
    let (dir, session, mut grant) = fixture();
    let mut links = BoardLinks::default();
    links
        .set_session(&dir, session.clone(), grant.clone(), "r".into())
        .unwrap();
    wait(|| dir.join("board-session.json").exists());
    let sent = call(&dir, &session, "active", 0, "r");
    let mut received = Vec::new();
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });
    grant.mutation_autonomy = true;
    links
        .set_session(&dir, session.clone(), grant, "r".into())
        .unwrap();
    wait(|| {
        std::fs::read(dir.join("board-session.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|v| v["grant"]["mutation_autonomy"] == true)
    });
    links
        .reply(&dir, BoardReply::error(&sent, "test result"))
        .unwrap();
    wait(|| dir.join("board-replies/active.json").exists());
    call(&dir, &session, "repeat", 0, "r");
    wait(|| dir.join("board-replies/repeat.json").exists());
    assert!(links.poll(&dir).is_empty());
    links.revoke(&dir);
    wait(|| !dir.join("board-session.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cancel_then_immediate_new_task_reuses_one_worker_without_stale_cleanup() {
    let (dir, session, grant) = fixture();
    let mut links = BoardLinks::default();
    links
        .set_session(&dir, session.clone(), grant.clone(), "old".into())
        .unwrap();
    wait(|| dir.join("board-session.json").exists());
    let sender = links.links[&dir].tx.clone();
    links.revoke(&dir);
    links
        .set_session(&dir, session.clone(), grant, "new".into())
        .unwrap();
    assert!(sender.same_channel(&links.links[&dir].tx));
    wait(|| {
        std::fs::read(dir.join("board-session.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            .is_some_and(|v| v["request_id"] == "new")
    });
    call(&dir, &session, "fresh", 0, "new");
    let mut received = Vec::new();
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });
    assert_eq!(received[0].request_id, "new");
    links.retain(&std::collections::HashSet::new());
    drop(sender);
    wait(|| !dir.join("board-session.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn persistent_bad_entries_cannot_starve_a_later_valid_mailbox_call() {
    let (dir, session, grant) = fixture();
    let mailbox = dir.join("board-calls");
    std::fs::create_dir_all(&mailbox).unwrap();
    for i in 0..128 {
        std::fs::write(mailbox.join(format!("junk-{i:03}.json")), b"not json").unwrap();
        std::fs::write(
            mailbox.join(format!("partial-{i:03}.tmp")),
            b"not yet committed",
        )
        .unwrap();
    }
    // Select an entry observed beyond the first batch instead of assuming the
    // filesystem sorts names. Overwriting its bytes does not insert a new entry.
    let later = std::fs::read_dir(&mailbox)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|s| s == "json"))
        .last()
        .unwrap()
        .path();
    let call_id = later.file_stem().unwrap().to_str().unwrap();
    let expected = call(&dir, &session, call_id, 0, "r");
    let position = std::fs::read_dir(&mailbox)
        .unwrap()
        .filter_map(Result::ok)
        .position(|e| e.path() == later)
        .unwrap();
    assert!(
        position >= 32,
        "fixture must place the valid call after the first scan budget"
    );
    let mut links = BoardLinks::default();
    links.set_session(&dir, session, grant, "r".into()).unwrap();
    let mut received = Vec::new();
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });
    assert_eq!(received, vec![expected]);
    assert!(mailbox.join("partial-000.tmp").exists());
    links.retain(&std::collections::HashSet::new());
    wait(|| !dir.join("board-session.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn limited_poll_keeps_the_remainder_for_the_next_frame() {
    let (tx, _rx) = bounded(1);
    let (out, calls) = bounded(16);
    let (dir, session, _) = fixture();
    let mut links = BoardLinks::default();
    links.links.insert(
        dir.clone(),
        Link {
            tx,
            calls,
            active: true,
            current: Some((session.clone(), "r".into())),
            replies: RefCell::default(),
        },
    );
    for sequence in 0..10 {
        out.send(BoardCall {
            id: format!("call-{sequence}"),
            session: session.clone(),
            request_id: "r".into(),
            sequence,
            operation: BoardOperation::Read {
                node_ids: vec![],
                offset: 0,
                limit: 32,
            },
        })
        .unwrap();
    }
    assert!(links.poll_limit(&dir, 0).is_empty());
    assert_eq!(links.poll_limit(&dir, 4).len(), 4);
    assert_eq!(links.poll_limit(&dir, 100).len(), 6);
    assert!(links.poll(&dir).is_empty());
}

#[test]
#[cfg(windows)]
fn cloud_placeholder_mailbox_bytes_are_never_opened() {
    let (dir, _, _) = fixture();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("placeholder.json");
    std::fs::write(&path, b"{\"private\":true}").unwrap();
    assert!(atlas_core::cloud::mark_offline(&path));
    let result = worker::read_bounded(&path);
    atlas_core::cloud::clear_offline(&path);
    assert!(result.is_none());
    assert!(worker::read_bounded(&path).is_some());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn saturated_reply_queue_retains_results_until_poll_and_fences_revocation() {
    let (dir, session, grant) = fixture();
    let (tx, rx) = bounded(1);
    let (_out, calls) = bounded(1);
    tx.send(Work::Revoke).unwrap();
    let mut links = BoardLinks::default();
    links.links.insert(
        dir.clone(),
        Link {
            tx,
            calls,
            active: true,
            current: Some((session.clone(), "r".into())),
            replies: RefCell::default(),
        },
    );
    let reply = |id: &str| BoardReply {
        id: id.into(),
        session_id: session.session_id.clone(),
        request_id: "r".into(),
        success: true,
        result: serde_json::json!({"status":"applied"}),
    };
    links.reply(&dir, reply("first")).unwrap();
    links.reply(&dir, reply("first")).unwrap();
    assert_eq!(links.links[&dir].replies.borrow().pending.len(), 1);
    assert!(matches!(rx.try_recv(), Ok(Work::Revoke)));
    assert!(links.poll_limit(&dir, 0).is_empty());
    let Work::Reply(delivered) = rx.try_recv().unwrap() else {
        panic!("retained reply was lost")
    };
    assert_eq!(delivered, reply("first"));
    assert!(links.links[&dir].replies.borrow().pending.is_empty());
    // Saturate again; revocation must clear the unsent response immediately.
    links.links[&dir].tx.send(Work::Revoke).unwrap();
    links.reply(&dir, reply("cancelled")).unwrap();
    assert_eq!(links.links[&dir].replies.borrow().pending.len(), 1);
    links.revoke(&dir);
    assert!(links.links[&dir].replies.borrow().pending.is_empty());
    assert!(links.reply(&dir, reply("late")).is_err());
    assert!(matches!(rx.try_recv(), Ok(Work::Revoke)));
    links
        .set_session(&dir, session.clone(), grant, "new".into())
        .unwrap();
    assert!(matches!(rx.try_recv(), Ok(Work::Bind(_))));
    assert!(links.reply(&dir, reply("old-request")).is_err());
    assert!(rx.try_recv().is_err());
}

#[test]
fn reply_backlog_has_a_hard_per_request_budget_and_bounds_payloads() {
    let (dir, session, _) = fixture();
    let (tx, _rx) = bounded(1);
    let (_out, calls) = bounded(1);
    tx.send(Work::Revoke).unwrap();
    let mut links = BoardLinks::default();
    links.links.insert(
        dir.clone(),
        Link {
            tx,
            calls,
            active: true,
            current: Some((session.clone(), "r".into())),
            replies: RefCell::default(),
        },
    );
    for i in 0..MAX_BOARD_CALLS {
        links
            .reply(
                &dir,
                BoardReply {
                    id: format!("reply-{i}"),
                    session_id: session.session_id.clone(),
                    request_id: "r".into(),
                    success: true,
                    result: serde_json::Value::String("x".repeat(MAX_BOARD_MESSAGE_BYTES as usize)),
                },
            )
            .unwrap();
    }
    let backlog = links.links[&dir].replies.borrow();
    assert_eq!(backlog.pending.len(), MAX_BOARD_CALLS);
    assert!(backlog
        .pending
        .iter()
        .all(|r| !r.success && serde_json::to_vec(r).unwrap().len() < 1024));
    drop(backlog);
    assert!(links
        .reply(
            &dir,
            BoardReply {
                id: "overflow".into(),
                session_id: session.session_id,
                request_id: "r".into(),
                success: true,
                result: serde_json::Value::Null
            }
        )
        .is_err());
    assert_eq!(
        links.links[&dir].replies.borrow().pending.len(),
        MAX_BOARD_CALLS
    );
}

#[test]
fn accepted_reply_survives_a_transient_worker_write_failure() {
    let (dir, session, grant) = fixture();
    let mut links = BoardLinks::default();
    links
        .set_session(&dir, session.clone(), grant, "r".into())
        .unwrap();
    wait(|| dir.join("board-session.json").exists());
    let sent = call(&dir, &session, "blocked", 0, "r");
    let mut received = Vec::new();
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });
    // A directory at the destination makes rename fail without changing any
    // machine permission or security policy. The worker must retain its reply.
    let destination = dir.join("board-replies/blocked.json");
    std::fs::create_dir(&destination).unwrap();
    links
        .reply(&dir, BoardReply::error(&sent, "retained result"))
        .unwrap();
    wait(|| dir.join("board-replies/blocked.json.tmp").exists());
    std::fs::remove_dir(&destination).unwrap();
    wait(|| destination.is_file());
    let delivered: BoardReply =
        serde_json::from_slice(&std::fs::read(&destination).unwrap()).unwrap();
    assert!(delivered.matches(&sent));
    assert_eq!(delivered.result["error"], "retained result");
    links.retain(&std::collections::HashSet::new());
    wait(|| !dir.join("board-session.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reply_files_stay_within_the_bridge_byte_limit() {
    let (dir, session, grant) = fixture();
    let mut links = BoardLinks::default();
    links
        .set_session(&dir, session.clone(), grant, "r".into())
        .unwrap();
    wait(|| dir.join("board-session.json").exists());
    let sent = call(&dir, &session, "large", 0, "r");
    let mut received = Vec::new();
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });

    let result = serde_json::json!({
        "objects": (0..1200)
            .map(|id| serde_json::json!({"id":id,"kind":"text","text":"Cooling roof"}))
            .collect::<Vec<_>>()
    });
    let reply = BoardReply {
        id: sent.id,
        session_id: session.session_id.clone(),
        request_id: "r".into(),
        success: true,
        result,
    };
    let compact = serde_json::to_vec(&reply).unwrap();
    let pretty = serde_json::to_string_pretty(&reply).unwrap();
    assert!(compact.len() <= MAX_BOARD_MESSAGE_BYTES as usize);
    assert!(pretty.len() > MAX_BOARD_MESSAGE_BYTES as usize);

    links.reply(&dir, reply.clone()).unwrap();
    let path = dir.join("board-replies/large.json");
    wait(|| path.is_file());
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes, compact);
    assert!(bytes.len() <= MAX_BOARD_MESSAGE_BYTES as usize);
    assert_eq!(serde_json::from_slice::<BoardReply>(&bytes).unwrap(), reply);

    received.clear();
    let sent = call(&dir, &session, "oversized", 1, "r");
    wait(|| {
        received.extend(links.poll(&dir));
        !received.is_empty()
    });

    let reply = BoardReply {
        id: sent.id,
        session_id: session.session_id.clone(),
        request_id: "r".into(),
        success: true,
        result: serde_json::json!({"text":"x".repeat(MAX_BOARD_MESSAGE_BYTES as usize)}),
    };
    assert!(serde_json::to_vec(&reply).unwrap().len() > MAX_BOARD_MESSAGE_BYTES as usize);
    links.reply(&dir, reply).unwrap();
    let path = dir.join("board-replies/oversized.json");
    wait(|| path.is_file());
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.len() <= MAX_BOARD_MESSAGE_BYTES as usize);
    let bounded: BoardReply = serde_json::from_slice(&bytes).unwrap();
    assert!(!bounded.success);
    assert_eq!(
        bounded.result["error"],
        "Board result exceeds the response limit; request fewer objects."
    );

    links.retain(&std::collections::HashSet::new());
    wait(|| !dir.join("board-session.json").exists());
    std::fs::remove_dir_all(dir).unwrap();
}
