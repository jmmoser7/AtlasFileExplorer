mod common;
use atlas_connector::*;
use common::*;

#[test]
fn explicit_binding_and_exact_source_revision_are_required() {
    let mut session = session();
    expect_error(
        begin(&mut session, inspect("before-bind")),
        ErrorCode::SourceNotBound,
    );
    bind(&mut session);
    assert_eq!(
        session.binding().unwrap().source.locator.preferred(),
        Some("models/example.dat")
    );
    let mut wrong = inspect("wrong-source");
    wrong.source.as_mut().unwrap().source_id = "source-2".into();
    expect_error(begin(&mut session, wrong), ErrorCode::SourceMismatch);
    let mut stale = inspect("stale");
    stale.source.as_mut().unwrap().revision = "older-or-newer-is-opaque".into();
    expect_error(begin(&mut session, stale), ErrorCode::StaleRevision);
}

#[test]
fn duplicate_pending_and_completed_ids_never_redispatch() {
    let mut session = session();
    bind(&mut session);
    let req = inspect("once");
    assert!(matches!(
        begin(&mut session, req.clone()).unwrap(),
        Begin::Dispatch(_)
    ));
    expect_error(begin(&mut session, req.clone()), ErrorCode::RequestPending);
    let mut changed = req.clone();
    changed.operation = Operation::DescribeCommands;
    expect_error(begin(&mut session, changed), ErrorCode::RequestIdConflict);
    let reply = session
        .complete(
            "once",
            ReplyBody::CommandResult {
                value: serde_json::json!({"ok":true}),
                revision: None,
            },
        )
        .unwrap();
    assert_eq!(begin(&mut session, req).unwrap(), Begin::Replay(reply));
}

#[test]
fn timeout_is_uncertain_terminal_and_cannot_be_retried_or_replaced() {
    let mut session = session();
    bind(&mut session);
    let req = inspect("uncertain");
    begin(&mut session, req.clone()).unwrap();
    let receipt = session.timeout("uncertain").unwrap();
    assert!(matches!(receipt.outcome, ReplyBody::OutcomeUnknown { .. }));
    assert_eq!(begin(&mut session, req).unwrap(), Begin::Replay(receipt));
    expect_error(
        session.complete(
            "uncertain",
            ReplyBody::CommandResult {
                value: serde_json::json!({}),
                revision: None,
            },
        ),
        ErrorCode::OutcomeUnknown,
    );
}

#[test]
fn old_generation_and_delayed_results_cannot_update_replacement_source() {
    let mut session = session();
    bind(&mut session);
    let req = inspect("late");
    begin(&mut session, req.clone()).unwrap();
    session
        .observe_source_state(
            &SourceScope {
                source_id: "source-1".into(),
                revision: "rev-b".into(),
            },
            false,
        )
        .unwrap();
    expect_error(
        session.complete(
            "late",
            ReplyBody::CommandResult {
                value: serde_json::json!({}),
                revision: None,
            },
        ),
        ErrorCode::StaleRevision,
    );
    session.reconnect(2).unwrap();
    assert!(session.binding().is_none());
    expect_error(begin(&mut session, req.clone()), ErrorCode::StaleGeneration);
    let reply = ReplyEnvelope {
        session_id: req.session_id,
        generation: 1,
        request_id: req.request_id,
        source: req.source,
        outcome: ReplyBody::Ack,
    };
    expect_error(session.accept_reply(reply), ErrorCode::StaleGeneration);
}

#[test]
fn request_count_and_total_retention_are_bounded_without_eviction() {
    let hello = MockConnector::new(identity("alpha")).hello();
    let policy = HostPolicy {
        max_cached_requests: 1,
        ..HostPolicy::default()
    };
    let (mut session, _) = Session::negotiate(&policy, &hello, "test-session".into(), 1).unwrap();
    let first = request("catalog", Operation::DescribeCommands, None);
    begin(&mut session, first.clone()).unwrap();
    session.timeout("catalog").unwrap();
    expect_error(
        begin(
            &mut session,
            request("another", Operation::DescribeCommands, None),
        ),
        ErrorCode::RequestCacheFull,
    );
    assert!(matches!(
        begin(&mut session, first).unwrap(),
        Begin::Replay(_)
    ));
    let policy = HostPolicy {
        max_cached_bytes: 2048,
        ..HostPolicy::default()
    };
    let (mut session, _) = Session::negotiate(&policy, &hello, "test-session".into(), 1).unwrap();
    begin(
        &mut session,
        request("one", Operation::DescribeCommands, None),
    )
    .unwrap();
    expect_error(
        begin(
            &mut session,
            request("two", Operation::DescribeCommands, None),
        ),
        ErrorCode::RequestCacheFull,
    );
    assert!(session.retained_bytes() <= policy.max_cached_bytes);
}

#[test]
fn wire_claim_is_not_trusted_authority() {
    let mut session = session();
    bind(&mut session);
    let req = inspect("spoof");
    let actual = CallerAuthority::Agent {
        agent_id: "agent-1".into(),
        workspace_autonomy: true,
    };
    expect_error(session.begin(req, &actual), ErrorCode::AuthorityDenied);
}

#[test]
fn agents_cannot_mutate_sources_or_drafts_even_when_descriptor_claims_and_autonomy_allow() {
    for effect in [CommandEffect::SourceWriteBack, CommandEffect::DraftMutation] {
        let mut hello = MockConnector::new(identity("alpha")).hello();
        hello.commands[0].effect = effect;
        hello.commands[0].undo = UndoOwner::NativeApplication;
        hello.commands[0].agent_allowed = true;
        let (mut session, _) =
            Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
        bind(&mut session);
        let mut req = inspect("agent-write");
        req.caller = CallerAuthority::Agent {
            agent_id: "agent".into(),
            workspace_autonomy: true,
        };
        expect_error(begin(&mut session, req), ErrorCode::AuthorityDenied);
    }
}

#[test]
fn human_write_direction_names_target_and_result_does_not_fake_slate_undo() {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.commands[0].effect = CommandEffect::SourceWriteBack;
    hello.commands[0].undo = UndoOwner::NativeApplication;
    let (mut session, _) =
        Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
    bind(&mut session);
    expect_error(
        begin(&mut session, inspect("no-target")),
        ErrorCode::AuthorityDenied,
    );
    let mut req = inspect("human-write");
    req.caller = CallerAuthority::Human {
        action_id: "human-write".into(),
        target_source: Some("source-1".into()),
    };
    begin(&mut session, req).unwrap();
    session
        .complete(
            "human-write",
            ReplyBody::CommandResult {
                value: serde_json::json!({"native_undo_receipt":"u-1"}),
                revision: Some("rev-b".into()),
            },
        )
        .unwrap();
    assert_eq!(session.binding().unwrap().source.revision, "rev-b");
    assert!(session.binding().unwrap().dirty);
}
