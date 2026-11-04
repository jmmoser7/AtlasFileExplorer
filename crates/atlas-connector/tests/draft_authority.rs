mod common;
use atlas_connector::*;
use common::*;
fn draft_session(version: ProtocolVersion, mode: SourceMode) -> Session {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.versions = vec![version];
    hello.commands[0].effect = CommandEffect::DraftMutation;
    hello.commands[0].agent_allowed = true;
    hello.commands[0].undo = UndoOwner::NativeApplication;
    let (mut session, _) =
        Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
    begin(
        &mut session,
        request(
            "bind",
            Operation::BindSource {
                source: source(),
                mode,
                selection_id: "selected".into(),
            },
            None,
        ),
    )
    .unwrap();
    session
        .complete(
            "bind",
            ReplyBody::SourceBound {
                binding: SourceBinding {
                    source: source(),
                    document_id: "native-doc".into(),
                    mode,
                    dirty: false,
                },
            },
        )
        .unwrap();
    session
}
fn edit(id: &str, agent: &str, autonomy: bool) -> RequestEnvelope {
    let mut request = inspect(id);
    request.caller = CallerAuthority::Agent {
        agent_id: agent.into(),
        workspace_autonomy: autonomy,
    };
    request
}
fn grant(session: &mut Session) {
    session
        .grant_draft_authority(
            "explicit-user-grant",
            "architect",
            &source().scope(),
            &[MOCK_INSPECT_COMMAND.into()],
        )
        .unwrap();
}
#[test]
fn grant_is_required_and_scoped_to_agent_command_and_draft() {
    let mut session = draft_session(V0_2, SourceMode::IsolatedDraft);
    expect_error(
        begin(&mut session, edit("no-grant", "architect", true)),
        ErrorCode::AuthorityDenied,
    );
    grant(&mut session);
    expect_error(
        begin(&mut session, edit("other-agent", "critic", true)),
        ErrorCode::AuthorityDenied,
    );
    expect_error(
        begin(&mut session, edit("no-autonomy", "architect", false)),
        ErrorCode::AuthorityDenied,
    );
    begin(&mut session, edit("allowed", "architect", true)).unwrap();
    session
        .complete(
            "allowed",
            ReplyBody::CommandResult {
                value: serde_json::json!({"native_undo":"u1"}),
                revision: Some("rev-b".into()),
            },
        )
        .unwrap();
    assert!(session.binding().unwrap().dirty);
    assert!(session.revoke_draft_authority("explicit-user-grant"));
    let mut denied = edit("revoked", "architect", true);
    denied.source.as_mut().unwrap().revision = "rev-b".into();
    expect_error(begin(&mut session, denied), ErrorCode::AuthorityDenied);
}
#[test]
fn linked_originals_and_v01_cannot_receive_draft_grants() {
    for (version, mode) in [
        (V0_1, SourceMode::IsolatedDraft),
        (V0_2, SourceMode::Linked),
    ] {
        let mut session = draft_session(version, mode);
        expect_error(
            session.grant_draft_authority(
                "g",
                "architect",
                &source().scope(),
                &[MOCK_INSPECT_COMMAND.into()],
            ),
            ErrorCode::AuthorityDenied,
        );
    }
}
#[test]
fn grants_do_not_survive_reconnect_or_rebind() {
    let mut session = draft_session(V0_2, SourceMode::IsolatedDraft);
    grant(&mut session);
    session.reconnect(2).unwrap();
    assert!(!session.revoke_draft_authority("explicit-user-grant"));
}
#[test]
fn source_writeback_cannot_be_granted() {
    let mut hello = MockConnector::new(identity("alpha")).hello();
    hello.versions = vec![V0_2];
    hello.commands[0].effect = CommandEffect::SourceWriteBack;
    hello.commands[0].agent_allowed = true;
    hello.commands[0].undo = UndoOwner::NativeApplication;
    let (mut session, _) =
        Session::negotiate(&HostPolicy::default(), &hello, "test-session".into(), 1).unwrap();
    bind(&mut session);
    expect_error(
        session.grant_draft_authority(
            "g",
            "architect",
            &source().scope(),
            &[MOCK_INSPECT_COMMAND.into()],
        ),
        ErrorCode::AuthorityDenied,
    );
    expect_error(
        begin(&mut session, edit("write", "architect", true)),
        ErrorCode::AuthorityDenied,
    );
}
