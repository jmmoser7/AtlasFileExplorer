use super::*;

fn fixture() -> (PathBuf, BoardLinkSession, PendingCalls) {
    let dir = std::env::temp_dir().join(format!("slate-neutral-tools-{}", crate::request_id()));
    std::fs::create_dir_all(dir.join("board-replies")).unwrap();
    let session = BoardLinkSession {
        session_id: "model-task-test".into(),
        grant_id: "grant-test".into(),
        workspace_id: "workspace-test".into(),
    };
    std::fs::write(
        dir.join("board-session.json"),
        serde_json::to_vec(&json!({"session":session,"request_id":"request-test"})).unwrap(),
    )
    .unwrap();
    (dir, session, PendingCalls::default())
}
fn start(exchange: &mut PendingCalls, dir: &Path, session: &BoardLinkSession) {
    exchange
        .start(
            json!(7),
            dir.to_path_buf(),
            session.clone(),
            "request-test".into(),
            BoardOperation::Read {
                node_ids: vec![],
                offset: 0,
                limit: 32,
            },
        )
        .unwrap();
}
#[test]
fn neutral_exchange_binds_reply_identity_and_cleans_only_its_call() {
    let (dir, session, mut exchange) = fixture();
    start(&mut exchange, &dir, &session);
    let call = exchange.calls.call(0);
    let path = dir.join("board-replies").join(format!("{}.json", call.id));
    let mut reply = BoardReply::error(call, "fixture response");
    reply.request_id = "another-request".into();
    std::fs::write(&path, serde_json::to_vec(&reply).unwrap()).unwrap();
    assert!(exchange
        .poll(&AtomicBool::new(false), Some(|_| false))
        .is_empty());
    reply.request_id = "request-test".into();
    std::fs::write(&path, serde_json::to_vec(&reply).unwrap()).unwrap();
    let done = exchange.poll(&AtomicBool::new(false), Some(|_| false));
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].token, json!(7));
    assert!(!path.exists() && exchange.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn neutral_exchange_revocation_and_cancel_fail_closed() {
    for cancel in [false, true] {
        let (dir, session, mut exchange) = fixture();
        start(&mut exchange, &dir, &session);
        assert!(exchange
            .poll(&AtomicBool::new(false), Some(|_| false))
            .is_empty());
        if !cancel {
            std::fs::remove_file(dir.join("board-session.json")).unwrap();
        }
        let done = exchange.poll(&AtomicBool::new(cancel), Some(|_| false));
        assert_eq!(done.len(), 1);
        assert!(!done[0].reply.success);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn neutral_exchange_keeps_sequence_and_requires_guard_and_bounds() {
    let (dir, session, mut exchange) = fixture();
    start(&mut exchange, &dir, &session);
    start(&mut exchange, &dir, &session);
    assert_eq!(exchange.calls.call(0).sequence, 0);
    assert_eq!(exchange.calls.call(1).sequence, 1);
    let path = dir.join("board-session.json");
    assert!(read_json(&path, None).is_none());
    assert!(read_json(&path, Some(|_| true)).is_none());
    std::fs::write(&path, vec![b' '; MAX_BOARD_MESSAGE_BYTES as usize + 1]).unwrap();
    assert!(read_json(&path, Some(|_| false)).is_none());
    assert!(exchange
        .start(
            json!(1),
            dir.clone(),
            session,
            "request-test".into(),
            BoardOperation::Invoke {
                command: "board.model.connector.invoke".into(),
                targets: vec!["1".into()],
                arguments: json!({"plan":"x".repeat(65536)})
            }
        )
        .is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
