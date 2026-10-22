use super::*;
fn fixture() -> (CollaboratorGrant, BoardCall) {
    let grant = CollaboratorGrant {
        id: "g".into(),
        workspace_id: "w".into(),
        readable_board: false,
        readable_nodes: vec!["1".into()],
        readable_sources: vec![],
        commands: vec!["move".into()],
        conversation_ids: vec![],
        mutation_autonomy: false,
        helper_limit: 2,
        max_actions: 4,
    };
    let call = BoardCall {
        id: "call-1".into(),
        session: BoardLinkSession {
            session_id: "s".into(),
            grant_id: "g".into(),
            workspace_id: "w".into(),
        },
        request_id: "r".into(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "move".into(),
            targets: vec!["1".into()],
            arguments: serde_json::json!({"x":1}),
        },
    };
    (grant, call)
}
#[test]
fn explicit_scope_fences_identity_commands_targets_and_budget() {
    let (grant, call) = fixture();
    assert!(grant.authorize(&call, &call.session, "r").is_ok());
    assert!(grant.authorize(&call, &call.session, "old").is_err());
    let mut outside = call.clone();
    outside.operation = BoardOperation::Invoke {
        command: "move".into(),
        targets: vec!["2".into()],
        arguments: Value::Null,
    };
    assert!(grant.authorize(&outside, &call.session, "r").is_err());
    outside = call.clone();
    outside.sequence = 4;
    assert!(grant.authorize(&outside, &call.session, "r").is_err());
    outside = call.clone();
    outside.id = "../escape".into();
    assert!(outside.validate().is_err());
    outside = call.clone();
    outside.operation = BoardOperation::Invoke {
        command: "shell".into(),
        targets: vec![],
        arguments: Value::Null,
    };
    assert!(grant.authorize(&outside, &call.session, "r").is_err());
}
#[test]
fn helpers_cannot_expand_scope_autonomy_or_spawn_further() {
    let (grant, _) = fixture();
    let mut helper = grant.clone();
    helper.id = "child".into();
    helper.helper_limit = 0;
    assert!(grant.allows_helper(&helper));
    helper.mutation_autonomy = true;
    assert!(!grant.allows_helper(&helper));
    helper.mutation_autonomy = false;
    helper.readable_board = true;
    assert!(!grant.allows_helper(&helper));
    helper.readable_board = false;
    helper.helper_limit = 1;
    assert!(!grant.allows_helper(&helper));
}
#[test]
fn ordinary_chat_has_no_collaborator_context_and_old_requests_still_read() {
    let mut request: crate::AgentRequest =
        serde_json::from_str(r#"{"id":"r","prompt":"hello","at":0}"#).unwrap();
    assert!(request.collaborator.is_none());
    let (_, call) = fixture();
    request.collaborator = Some(call.session);
    let prompt = request.input_text();
    assert!(prompt.contains("slate_board_read"));
    assert!(!prompt.contains("board_summary"));
    assert!(!prompt.contains("place.json"));
    assert_eq!(crate::display_prompt(&prompt), "hello");
    let guide = board_link_guide(&request, "C:/literal-{{session}}");
    assert!(guide.contains("C:/literal-{{session}}"));
}
#[test]
fn oversized_arguments_and_unknown_operations_are_rejected() {
    let (_, mut call) = fixture();
    call.operation = BoardOperation::Invoke {
        command: "move".into(),
        targets: vec![],
        arguments: Value::String("x".repeat(MAX_BOARD_MESSAGE_BYTES as usize)),
    };
    assert!(call.validate().is_err());
    assert!(serde_json::from_str::<BoardOperation>(r#"{"kind":"execute","code":"x"}"#).is_err());
}

#[test]
fn read_pagination_defaults_remain_compatible_and_bounded() {
    let (_, mut call) = fixture();
    call.operation = serde_json::from_str(r#"{"kind":"read","node_ids":[]}"#).unwrap();
    assert_eq!(
        call.operation,
        BoardOperation::Read {
            node_ids: vec![],
            offset: 0,
            limit: 32
        }
    );
    assert!(call.validate().is_ok());
    for (offset, limit) in [(1_000_001, 32), (0, 0), (0, 65)] {
        call.operation = BoardOperation::Read {
            node_ids: vec![],
            offset,
            limit,
        };
        assert!(call.validate().is_err());
    }
    call.operation = BoardOperation::Read {
        node_ids: vec![],
        offset: 1_000_000,
        limit: 64,
    };
    assert!(call.validate().is_ok());
}

#[test]
fn task_action_limit_is_large_bounded_and_keeps_backlog_separate() {
    let (mut grant, mut call) = fixture();
    grant.max_actions = MAX_TASK_ACTIONS;
    call.sequence = 4095;
    assert!(grant.validate().is_ok());
    assert!(call.validate().is_ok());
    assert!(grant.authorize(&call, &call.session, "r").is_ok());
    call.sequence = u64::from(MAX_TASK_ACTIONS);
    assert!(call.validate().is_err());
    grant.max_actions = MAX_TASK_ACTIONS + 1;
    assert!(grant.validate().is_err());
    assert_eq!(MAX_BOARD_CALLS, 128);
}

#[test]
fn participant_reads_are_typed_bounded_and_grant_scoped() {
    let (mut grant, mut call) = fixture();
    grant.conversation_ids = vec!["helper-1".into()];
    call.operation = serde_json::from_str(r#"{"kind":"read_participants"}"#).unwrap();
    assert_eq!(
        call.operation,
        BoardOperation::ReadParticipants {
            participant_ids: vec![],
            offset: 0,
            limit: 8,
            text_cursor: None,
            text_limit: None,
            text_generation: None,
        }
    );
    assert!(grant.authorize(&call, &call.session, "r").is_ok());
    call.operation = BoardOperation::ReadParticipants {
        participant_ids: vec!["helper-2".into()],
        offset: 0,
        limit: 8,
        text_cursor: None,
        text_limit: None,
        text_generation: None,
    };
    assert!(grant.authorize(&call, &call.session, "r").is_err());
    for (offset, limit) in [(1_000_001, 8), (0, 0), (0, 9)] {
        call.operation = BoardOperation::ReadParticipants {
            participant_ids: vec![],
            offset,
            limit,
            text_cursor: None,
            text_limit: None,
            text_generation: None,
        };
        assert!(call.validate().is_err());
    }
    call.operation = BoardOperation::ReadParticipants {
        participant_ids: vec!["helper-1".into()],
        offset: 8,
        limit: 8,
        text_cursor: Some(4),
        text_limit: Some(4096),
        text_generation: None,
    };
    assert!(
        call.validate().is_err(),
        "continuations must carry a generation"
    );
    if let BoardOperation::ReadParticipants {
        text_generation, ..
    } = &mut call.operation
    {
        *text_generation = Some("helper-request-1".into());
    }
    assert!(call.validate().is_ok());
    if let BoardOperation::ReadParticipants { text_limit, .. } = &mut call.operation {
        *text_limit = Some(3);
    }
    assert!(call.validate().is_err());
}
