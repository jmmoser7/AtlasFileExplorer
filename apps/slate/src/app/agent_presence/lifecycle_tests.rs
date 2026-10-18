#[path = "session_test_support.rs"]
pub(super) mod session_support;
use super::super::tests::Harness;
use super::*;

mod delivery;
#[path = "participant_read_tests.rs"]
mod participant_reads;

fn host() -> Harness {
    let mut h = Harness::new("presence_helper_scope");
    h.app.at_home = false;
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h
}
fn profile(name: &str) -> StoredAgent {
    StoredAgent {
        id: atlas_agent::request_id(),
        name: name.into(),
        hat: "none".into(),
        provider: "local".into(),
        retired: false,
    }
}

#[test]
fn a_collaborator_cannot_join_the_home_placeholder_instead_of_a_workbook() {
    let mut h = host();
    h.app.at_home = true;
    assert!(h
        .app
        .join_presence(profile("Dot"), None)
        .unwrap_err()
        .contains("Open a board workbook"));
    assert!(h.app.presence.agents.is_empty());
    assert_eq!(h.app.presence.roster.profiles().count(), 0);
}

#[test]
fn many_departed_helpers_do_not_poison_the_parent_conversation_grant() {
    let mut h = host();
    let parent = profile("Dot");
    let parent_id = parent.id.clone();
    h.app.join_presence(parent, None).unwrap();
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .conversation_ids
        .push("explicit-board-chat".into());
    for _ in 0..140 {
        let helper = profile("Helper");
        let helper_id = helper.id.clone();
        h.app
            .join_presence(helper, Some(parent_id.clone()))
            .unwrap();
        h.app.retire_presence(&helper_id, "bow").unwrap();
        let helper = h
            .app
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == helper_id)
            .unwrap();
        helper.departure.as_mut().unwrap().0 = Instant::now() - std::time::Duration::from_secs(3);
        h.app.presence_pump(&h.ctx);
        let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
        assert!(grant.validate().is_ok());
        assert!(!grant.conversation_ids.contains(&helper_id));
        assert!(grant
            .conversation_ids
            .iter()
            .any(|id| id == "explicit-board-chat"));
    }
    assert!(h.app.presence.helper_endpoints.is_empty());
}

#[test]
fn a_full_conversation_scope_refuses_a_helper_before_allocating_its_identity() {
    let mut h = host();
    let parent = profile("Dot");
    let parent_id = parent.id.clone();
    h.app.join_presence(parent, None).unwrap();
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .conversation_ids = (0..128).map(|n| format!("chat-{n}")).collect();
    let profiles = h.app.presence.roster.profiles().count();
    assert!(h
        .app
        .join_presence(profile("Helper"), Some(parent_id))
        .unwrap_err()
        .contains("scope is full"));
    assert_eq!(h.app.presence.agents.len(), 1);
    assert_eq!(h.app.presence.roster.profiles().count(), profiles);
}

fn read_helper_results(h: &mut Harness, parent_id: &str) -> serde_json::Value {
    let index = h
        .app
        .presence
        .agents
        .iter()
        .position(|a| a.profile.id == parent_id)
        .unwrap();
    let mut parent = h.app.presence.agents.remove(index);
    let call = BoardCall {
        id: "collect-helper-results".into(),
        session: parent.binding.clone(),
        request_id: parent.request.clone(),
        sequence: 0,
        operation: BoardOperation::Read {
            node_ids: vec![],
            offset: 0,
            limit: 8,
        },
    };
    h.app.begin_presence_read(&mut parent, &call).unwrap();
    let reply = h
        .app
        .advance_presence_read(&mut parent, &mut 256, std::time::Duration::from_secs(1))
        .unwrap();
    h.app.presence.agents.insert(index, parent);
    assert!(reply.success, "{:?}", reply.result);
    reply.result["participants"].clone()
}

#[test]
fn retired_helper_reads_keep_progress_and_distinguish_success_failure_cancel_and_expiry() {
    let mut h = host();
    let parent = profile("Dot");
    let parent_id = parent.id.clone();
    h.app.join_presence(parent, None).unwrap();
    h.app
        .presence_send_chat(&parent_id, "Collect the helper outcomes")
        .unwrap();
    for (activity, status, expired, expected) in [
        ("success", "Ready", false, "success"),
        (
            "error",
            "Image provider failed after partial work",
            true,
            "error",
        ),
        ("offline", "Sidecar offline", true, "offline"),
        (
            "blocked",
            "A human changed an action target",
            true,
            "blocked",
        ),
        ("cancel", "Cancelled; ready for a new task", true, "cancel"),
        (
            "reasoning",
            "Waiting for connected sidecar",
            true,
            "expired",
        ),
    ] {
        let helper = profile("Helper");
        let helper_id = helper.id.clone();
        h.app
            .join_presence(helper, Some(parent_id.clone()))
            .unwrap();
        h.app
            .presence_send_chat(&helper_id, "Read the notes")
            .unwrap();
        let request = h.app.presence.agents.last().unwrap().request.clone();
        if activity == "cancel" {
            // Exercise the cancellation owner, then the normal idle cadence.
            h.app.stop_presence(&helper_id, false).unwrap();
        }
        let helper = h
            .app
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == helper_id)
            .unwrap();
        if activity != "cancel" {
            helper.activity = activity.into();
            helper.status = status.into();
            if matches!(activity, "success" | "error" | "offline" | "blocked") {
                helper.note_helper_outcome(activity, status);
            }
        }
        helper.had_response = activity == "success";
        helper.inflight = activity == "reasoning";
        helper.last_active = Instant::now() - std::time::Duration::from_secs(9);
        if expired {
            helper.joined_at =
                Instant::now() - std::time::Duration::from_secs(helper_scope::TASK_LIMIT_SECS + 1);
        }
        h.app.presence.conversations.edit(&helper_id).unwrap().push(
            "assistant",
            "I have read the first document",
            "Helper",
            Some("local"),
            &request,
            "streaming",
        );
        h.app.presence_pump(&h.ctx);
        let participants = read_helper_results(&mut h, &parent_id);
        let matches: Vec<_> = participants
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["id"] == helper_id)
            .collect();
        assert_eq!(matches.len(), 1, "{activity}: {participants}");
        let result = matches[0];
        assert_eq!(result["name"], "Helper");
        assert_eq!(result["parent_id"], parent_id);
        assert_eq!(result["request_id"], request);
        assert_eq!(result["retired"], true);
        assert_eq!(result["result"], "I have read the first document");
        assert_eq!(result["outcome"], expected);
        assert_eq!(result["expired"], expired);
        assert_eq!(
            result["status"],
            if expected == "expired" {
                "Helper reached its 300-second task limit"
            } else {
                status
            }
        );
        // Result provenance survives removal of the departed live avatar.
        h.app
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == helper_id)
            .unwrap()
            .departure
            .as_mut()
            .unwrap()
            .0 = Instant::now() - std::time::Duration::from_secs(3);
        h.app.presence_pump(&h.ctx);
        assert!(read_helper_results(&mut h, &parent_id)
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == result));
    }
}

#[test]
fn a_new_cancelled_helper_request_excludes_old_errors_checkpoint_and_recoverable_blocks() {
    for checkpoint in [false, true] {
        let mut h = host();
        let parent = profile("Dot");
        let parent_id = parent.id.clone();
        h.app.join_presence(parent, None).unwrap();
        h.app
            .presence_send_chat(&parent_id, "Collect the outcome")
            .unwrap();
        let helper = profile("Helper");
        let helper_id = helper.id.clone();
        h.app
            .join_presence(helper, Some(parent_id.clone()))
            .unwrap();
        h.app
            .presence_send_chat(&helper_id, "Read the notes")
            .unwrap();
        publish_helper_state(&mut h, &helper_id, atlas_agent::AgentStatus::Thinking);
        publish_helper_state(
            &mut h,
            &helper_id,
            atlas_agent::AgentStatus::Error("Old provider error".into()),
        );
        h.app.presence_address(&h.ctx, &helper_id).unwrap();
        h.app
            .presence_send_chat(&helper_id, "Try a new task")
            .unwrap();
        let request = h.app.presence.agents[1].request.clone();
        assert!(h.app.presence.agents[1].helper_outcome.is_none());
        assert_eq!(h.app.presence.agents[1].activity, "reasoning");
        if checkpoint {
            // A new request ID with only old turns must not import a reply.
            publish_helper_state(&mut h, &helper_id, atlas_agent::AgentStatus::Thinking);
        }
        let mut helper = h.app.presence.agents.remove(1);
        let call = BoardCall {
            id: "recoverable-invalid-node-read".into(),
            session: helper.binding.clone(),
            request_id: helper.request.clone(),
            sequence: 0,
            operation: BoardOperation::Read {
                node_ids: vec!["not-a-node".into()],
                offset: 0,
                limit: 8,
            },
        };
        let mut stale_call = call.clone();
        stale_call.request_id = "old-request".into();
        assert!(
            !h.app
                .presence_call(&mut helper, &stale_call)
                .unwrap()
                .success
        );
        assert!(helper.helper_outcome.is_none());
        assert!(!h.app.presence_call(&mut helper, &call).unwrap().success);
        assert_eq!(helper.helper_outcome.as_ref().unwrap().outcome, "blocked");
        assert!(helper.inflight);
        h.app.presence.agents.insert(1, helper);
        h.app.stop_presence(&helper_id, false).unwrap();
        h.app.presence_address(&h.ctx, &helper_id).unwrap();
        let helper = &mut h.app.presence.agents[1];
        helper.last_active = Instant::now() - std::time::Duration::from_secs(9);
        helper.joined_at =
            Instant::now() - std::time::Duration::from_secs(helper_scope::TASK_LIMIT_SECS + 1);
        h.app.presence_pump(&h.ctx);
        let participants = read_helper_results(&mut h, &parent_id);
        let result = participants
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == helper_id)
            .unwrap();
        assert_eq!(result["outcome"], "cancel");
        assert_eq!(result["status"], "Cancelled; ready for a new task");
        assert_eq!(result["request_id"], request);
        assert_eq!(result["result"], "");
        assert_eq!(result["expired"], true);
    }
}

fn publish_helper_state(h: &mut Harness, who: &str, status: atlas_agent::AgentStatus) {
    let helper = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == who)
        .unwrap();
    let dir = helper.dir.clone();
    let session = AgentSession {
        request: helper.request.clone(),
        status,
        turns: vec![
            atlas_agent::AgentTurn {
                role: "user".into(),
                text: "Read the notes".into(),
                at: 1,
            },
            atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "I have read the first document".into(),
                at: 2,
            },
        ],
        ..Default::default()
    };
    delivery::wait_request(&dir.join("request.json"), &session.request);
    session_support::publish_and_observe(h, who, &session);
}

#[test]
fn provider_failure_after_assistant_progress_survives_helper_expiry_in_board_reads() {
    let mut h = host();
    let parent = profile("Dot");
    let parent_id = parent.id.clone();
    h.app.join_presence(parent, None).unwrap();
    h.app
        .presence_send_chat(&parent_id, "Collect the outcome")
        .unwrap();
    let helper = profile("Helper");
    let helper_id = helper.id.clone();
    h.app
        .join_presence(helper, Some(parent_id.clone()))
        .unwrap();
    h.app
        .presence_send_chat(&helper_id, "Read the notes")
        .unwrap();
    publish_helper_state(&mut h, &helper_id, atlas_agent::AgentStatus::Thinking);
    assert!(h.app.presence.agents[1].inflight);
    let failure = "Provider failed while reading the second document";
    publish_helper_state(
        &mut h,
        &helper_id,
        atlas_agent::AgentStatus::Error(failure.into()),
    );
    let helper = &mut h.app.presence.agents[1];
    assert_eq!(helper.activity, "error");
    assert_eq!(helper.status, failure);
    assert!(!helper.inflight);
    assert!(helper.request.is_empty());
    h.app.presence_address(&h.ctx, &helper_id).unwrap();
    assert_eq!(h.app.presence.agents[1].activity, "listening");
    let helper = &mut h.app.presence.agents[1];
    helper.joined_at =
        Instant::now() - std::time::Duration::from_secs(helper_scope::TASK_LIMIT_SECS + 1);
    h.app.presence_pump(&h.ctx);
    let participants = read_helper_results(&mut h, &parent_id);
    let result = participants
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == helper_id)
        .unwrap();
    assert_eq!(result["outcome"], "error");
    assert_eq!(result["status"], failure);
    assert_eq!(result["expired"], true);
    assert_eq!(result["result"], "I have read the first document");
    assert_eq!(result["parent_id"], parent_id);
    assert_eq!(result["retired"], true);
}

#[test]
fn human_text_patch_sends_the_complete_message_and_agent_or_layout_patches_do_not() {
    // Authored @cards require the real durable queue, not a disconnected store.
    let (mut h, who) = messaging::mention_tests::setup();
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_text",
            &[],
            &serde_json::json!({"x":0,"y":0,"width":300,"height":100,"text":"Draft"}),
        )
        .unwrap();
    let node = prepared.targets[0];
    assert!(h.app.commit_scene(prepared.cmds));
    h.app.patch_nodes(&[node], |node| {
        let slate_doc::scene::NodeKind::Text(text) = &mut node.kind else {
            panic!("expected authored text")
        };
        text.text = "@Dot: Please organize the three images".into();
    });
    let request_id = messaging::mention_tests::idle_queued_request(&h, &who);
    assert!(!request_id.is_empty());
    messaging::mention_tests::until(&mut h, |app| app.presence.agents[0].request == request_id);
    let path = h.app.presence.agents[0].dir.join("request.json");
    let first = delivery::wait_request(&path, &request_id);
    assert_eq!(first.id, request_id);
    assert_eq!(first.prompt, "Please organize the three images");

    h.app.stop_presence(&who, false).unwrap();
    h.app.patch_nodes_as(
        &[node],
        slate_doc::scene::CmdAuthor::Agent("Dot".into()),
        |node| {
            let slate_doc::scene::NodeKind::Text(text) = &mut node.kind else {
                panic!("expected authored text")
            };
            text.text = "@Dot: An agent-authored message must not relay itself".into();
        },
    );
    assert!(h.app.presence.agents[0].request.is_empty());
    h.app.patch_nodes(&[node], |node| node.rect.x += 20.0);
    assert!(h.app.presence.agents[0].request.is_empty());
    assert_eq!(
        h.app
            .presence
            .conversations
            .get(&who)
            .unwrap()
            .turns
            .iter()
            .filter(|turn| turn.role == "user")
            .count(),
        1,
        "agent and layout patches must not enqueue another message"
    );
    h.app.patch_nodes(&[node], |node| {
        let slate_doc::scene::NodeKind::Text(text) = &mut node.kind else {
            panic!("expected authored text")
        };
        text.text = "@Dot: This later human edit is complete too".into();
    });
    let second_id = messaging::mention_tests::idle_queued_request(&h, &who);
    assert_ne!(second_id, request_id);
    messaging::mention_tests::until(&mut h, |app| app.presence.agents[0].request == second_id);
    let second = delivery::wait_request(&path, &second_id);
    assert_eq!(second.prompt, "This later human edit is complete too");
}
