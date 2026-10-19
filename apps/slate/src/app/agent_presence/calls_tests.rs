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
    std::fs::create_dir_all(&agent.dir).unwrap();
    agent.request = atlas_agent::request_id();
    agent.sequences.clear();
    agent.calls = 0;
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
    let reply = h
        .app
        .presence_call_with_context(&h.ctx, &mut agent, &call)
        .unwrap();
    h.app.presence.agents.insert(0, agent);
    reply
}

#[test]
fn high_task_sequences_are_tracked_without_bitmask_aliasing() {
    let mut h = host();
    add(&mut h, "Long workflow");
    let mut agent = h.app.presence.agents.remove(0);
    std::fs::create_dir_all(&agent.dir).unwrap();
    agent.request = atlas_agent::request_id();
    agent.grant.as_mut().unwrap().max_actions = MAX_TASK_ACTIONS;
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
        sequence: 257,
        operation: BoardOperation::Read {
            node_ids: vec![],
            offset: 0,
            limit: 1,
        },
    };
    assert!(h.app.presence_call(&mut agent, &call).is_none());
    let duplicate = h.app.presence_call(&mut agent, &call).unwrap();
    assert!(!duplicate.success);
    assert!(duplicate.result["error"]
        .as_str()
        .unwrap_or_default()
        .contains("Duplicate"));
    assert_eq!(agent.calls, 1);
    h.app.presence.agents.insert(0, agent);
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
fn opted_in_collaborator_saves_current_authored_board_text() {
    use slate_doc::scene::{NodeKind, TextAlign, TextNode, Typeface, WorldRect};

    let mut h = host();
    h.seed();
    let path = h.base.join("authorized-save.slate");
    let tab = h.app.tab().id;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    h.app.save_doc_to(tab, path.clone());
    while (h.app.asset_saves_pending.contains(&tab) || !h.app.asset_save_rx.is_empty())
        && std::time::Instant::now() < deadline
    {
        h.app.poll_asset_saves(&h.ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        h.app.asset_save_rx.is_empty(),
        "initial save worker did not finish"
    );
    assert!(!h.app.asset_saves_pending.contains(&tab));
    assert_eq!(h.app.tab().path.as_ref(), Some(&path));
    assert!(!h.app.tab().dirty);

    let who = add(&mut h, "Authorized saver");
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::SetAutonomy {
                id: who.clone(),
                enabled: true,
            },
        )
        .unwrap();
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    assert!(grant.mutation_autonomy);
    assert!(
        h.app.settings.agent_consent_policies[0]
            .capabilities
            .mutation_autonomy
    );

    let before = h.app.tab().journal.undo_depth();
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(24.0, 32.0, 260.0, 72.0),
        NodeKind::Text(TextNode {
            text: "Authorized authored text".into(),
            style: Default::default(),
            family: Typeface::Sans,
            size: 18.0,
            color: slate_doc::scene::Rgba::opaque(20, 20, 20),
            align: TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    );
    let node_id = node.id;
    h.app.add_nodes(vec![node]);
    assert!(h.app.tab().journal.undo_depth() > before);

    let reply = invoke(&mut h, "app.save", json!({}));
    assert!(reply.success, "{:?}", reply.result);
    assert_eq!(reply.result["status"], "save_requested");
    while (h.app.asset_saves_pending.contains(&tab) || !h.app.asset_save_rx.is_empty())
        && std::time::Instant::now() < deadline
    {
        h.app.poll_asset_saves(&h.ctx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.app.asset_save_rx.is_empty(), "save worker did not finish");
    assert!(!h.app.asset_saves_pending.contains(&tab));
    assert!(!h.app.tab().dirty);

    let saved = slate_doc::SlateDoc::load_from(&path).unwrap();
    let NodeKind::Text(text) = &saved.scene.node(node_id).unwrap().kind else {
        panic!("saved authored node changed kind");
    };
    assert_eq!(text.text, "Authorized authored text");
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
        json!({"what":"x".repeat(801),"tried":"app.save","missing":"Save As support"}),
    );
    assert!(!too_long.success);
    assert!(
        too_long.result["error"]
            .as_str()
            .unwrap_or_default()
            .contains("800 characters"),
        "unexpected validation result: {:?}",
        too_long.result
    );
}

#[test]
fn explicit_stop_and_retirement_keep_consent_for_fresh_join_and_revoke_removes_it() {
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
    h.app.retire_presence(&who, "bow").unwrap();
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
    h.app.retire_presence(&who, "bow").unwrap();
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
    h.app.retire_presence(&who, "bow").unwrap();
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
