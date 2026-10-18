use super::*;
use crate::app::tests::Harness;
use serde_json::json;

fn add(h: &mut Harness, name: &str) -> String {
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: name.into(),
        hat: "none".into(),
        provider: "local".into(),
        retired: false,
    };
    let who = profile.id.clone();
    h.app.join_presence(profile, None).unwrap();
    who
}

fn invoke(h: &mut Harness, command: &str, arguments: serde_json::Value) -> BoardReply {
    let mut agent = h.app.presence.agents.remove(0);
    agent.request = "scoped-action-request".into();
    h.app
        .presence
        .roster
        .session_mut(&agent.motion)
        .unwrap()
        .begin_request(id(&agent.request).unwrap())
        .unwrap();
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: agent.binding.clone(),
        request_id: agent.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: command.into(),
            targets: vec![],
            arguments,
        },
    };
    let reply = h.app.presence_call(&mut agent, &call).unwrap();
    h.app.presence.agents.insert(0, agent);
    reply
}

#[test]
fn proposal_mode_cannot_save_the_workbook() {
    let mut h = host();
    add(&mut h, "Save collaborator");
    let reply = invoke(&mut h, "app.save", json!({}));
    assert!(!reply.success);
    assert!(reply.result["error"]
        .as_str()
        .unwrap_or_default()
        .contains("mutation autonomy"));
}

#[test]
fn scoped_collaborator_feedback_is_local_bounded_and_does_not_open_ui() {
    let mut h = host();
    add(&mut h, "Feedback collaborator");
    let reply = invoke(
        &mut h,
        "app.feedback.report",
        json!({"what":"Saving a new board was unreachable","tried":"app.save","missing":"Save As support"}),
    );
    assert!(reply.success, "{:?}", reply.result);
    assert_eq!(reply.result["status"], "recorded_locally");
    let link = &h.app.presence.agents[0].dir;
    assert!(link.join("feedback-log.json").is_file());
    assert!(!link.join("feedback.json").exists());
    assert!(!link.join("feedback.result.json").exists());
    assert_eq!(
        h.app.feedback.phase,
        atlas_shell::feedback::FeedbackPhase::Closed
    );

    let too_long = invoke(
        &mut h,
        "app.feedback.report",
        json!({"what":"x".repeat(801)}),
    );
    assert!(!too_long.success);
    assert!(too_long.result["error"]
        .as_str()
        .unwrap_or_default()
        .contains("800 characters"));
}

#[test]
fn explicit_stop_keeps_consent_for_fresh_join_and_revoke_removes_it() {
    let mut h = host();
    let who = add(&mut h, "Consented collaborator");
    let profile = h.app.presence.agents[0].profile.clone();
    for action in [
        chrome::PresenceAction::SetAutonomy {
            id: who.clone(),
            enabled: true,
        },
        chrome::PresenceAction::SetFileInputs {
            id: who.clone(),
            enabled: true,
        },
        chrome::PresenceAction::SetOwnChats {
            id: who.clone(),
            enabled: true,
        },
    ] {
        h.app.apply_presence_action(&h.ctx, action).unwrap();
    }
    let policy = &h.app.settings.agent_consent_policies[0];
    assert!(policy.capabilities.mutation_autonomy);
    assert!(policy.capabilities.board_file_inputs);
    assert!(policy.capabilities.own_chats);
    let old = h.app.presence.agents[0].binding.clone();

    h.app.stop_presence(&who, false).unwrap();
    assert_eq!(h.app.settings.agent_consent_policies.len(), 1);
    h.app.join_presence(profile.clone(), None).unwrap();
    let rebound = &h.app.presence.agents[0];
    assert_ne!(rebound.binding.session_id, old.session_id);
    assert_ne!(rebound.binding.grant_id, old.grant_id);
    let grant = rebound.grant.as_ref().unwrap();
    assert!(grant.mutation_autonomy);
    assert!(grant
        .readable_sources
        .iter()
        .any(|source| source == "board-file-inputs"));
    assert!(grant
        .readable_sources
        .iter()
        .any(|source| { source == own_chats::OWN_CHATS }));

    h.app.stop_presence(&who, true).unwrap();
    assert!(h.app.settings.agent_consent_policies.is_empty());
    h.app.join_presence(profile, None).unwrap();
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    assert!(!grant.mutation_autonomy);
    assert!(grant.readable_sources.is_empty());
}

#[test]
fn codex_full_access_is_opt_in_rebound_per_session_and_revoked() {
    let mut h = host();
    let path = h.base.join("provider-access.json");
    h.app.set_agent_access_store_for_tests(path.clone());
    atlas_ai::access::set_artifact_default_full_access_in(&path, true).unwrap();
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: "Codex collaborator".into(),
        hat: "none".into(),
        provider: "codex".into(),
        retired: false,
    };
    let who = profile.id.clone();
    h.app.join_presence(profile.clone(), None).unwrap();
    let first_session = h.app.presence.agents[0].binding.session_id.clone();
    assert!(!h.app.presence_full_access_for(&who));
    assert!(!atlas_ai::access::granted_in(&path, &first_session));

    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::SetFullAccess {
                id: who.clone(),
                enabled: true,
            },
        )
        .unwrap();
    assert!(h.app.presence_full_access_for(&who));
    assert!(
        h.app.settings.agent_consent_policies[0]
            .capabilities
            .provider_full_access
    );
    h.app.stop_presence(&who, false).unwrap();
    h.app.join_presence(profile.clone(), None).unwrap();
    let rebound_session = h.app.presence.agents[0].binding.session_id.clone();
    assert_ne!(first_session, rebound_session);
    assert!(h.app.presence_full_access_for(&who));
    assert!(atlas_ai::access::granted_in(&path, &rebound_session));

    h.app.stop_presence(&who, true).unwrap();
    assert!(!atlas_ai::access::granted_in(&path, &rebound_session));
    assert!(h.app.settings.agent_consent_policies.is_empty());
    let _ = std::fs::remove_file(path);
}
