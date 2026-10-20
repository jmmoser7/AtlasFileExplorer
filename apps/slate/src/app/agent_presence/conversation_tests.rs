use super::super::tests::Harness;
use super::*;
use std::collections::BTreeMap;

fn host() -> Harness {
    let mut h = Harness::new("presence_conversation");
    h.app.at_home = false;
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.ai.config.workspace_dir = Some(h.base.clone());
    h
}
fn add(h: &mut Harness) -> String {
    let id = atlas_agent::request_id();
    h.app
        .join_presence(
            StoredAgent {
                id: id.clone(),
                name: "Dot".into(),
                hat: "cap".into(),
                provider: "local".into(),
                retired: false,
            },
            None,
        )
        .unwrap();
    id
}
#[test]
fn self_chat_is_durable_visible_text_without_a_provider_or_live_membership() {
    let mut h = host();
    h.app.presence_update_identity("Jamie", None).unwrap();
    h.app.presence_save_draft("self", "Remember this").unwrap();
    h.app.presence_send_chat("self", "Remember this").unwrap();
    let c = h.app.presence_conversation("self").unwrap();
    assert_eq!(c.turns.len(), 1);
    assert_eq!(c.turns[0].author, "Jamie");
    assert_eq!(c.turns[0].text, "Remember this");
    assert!(c.draft.is_empty());
    assert!(c.bindings.is_empty());
    assert!(h.app.presence.agents.is_empty());
    let encoded = serde_json::to_string(c).unwrap();
    let restored: conversation::Conversation = serde_json::from_str(&encoded).unwrap();
    assert_eq!(&restored, c);
}

#[test]
fn identical_live_updates_keep_both_answers_and_the_original_response_boundary() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "hey!").unwrap();
    h.app.presence.agents[0].profile.provider = "codex".into(); // Display/merge fixture only; no provider starts.
    let a = &h.app.presence.agents[0];
    let mut c = h.app.presence_conversation(&who).unwrap().clone();
    let turn = |role: &str, text: &str| atlas_agent::AgentTurn {
        role: role.into(),
        text: text.into(),
        at: 1,
    };
    let mut snapshot = AgentSession {
        request: a.request.clone(),
        status: atlas_agent::AgentStatus::Thinking,
        turns: vec![turn("user", "hey!"), turn("assistant", "First reply")],
        ..Default::default()
    };
    c.absorb(a, &snapshot);
    c.push("user", "hey!", "You", Some("codex"), "steer-2", "complete");
    snapshot
        .turns
        .extend([turn("user", "hey!"), turn("assistant", "Second reply")]);
    assert_eq!(c.reply_start(a, &snapshot), Some(1));
    c.absorb(a, &snapshot);
    assert_eq!(
        c.turns
            .iter()
            .filter(|t| matches!(t.role.as_str(), "user" | "assistant"))
            .map(|t| (t.role.as_str(), t.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("user", "hey!"),
            ("assistant", "First reply"),
            ("user", "hey!"),
            ("assistant", "Second reply")
        ]
    );
}

#[test]
fn same_harness_and_invalid_destination_leave_a_running_request_untouched() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "Read the frame").unwrap();
    let dir = h.app.presence.agents[0].dir.clone();
    let request = h.app.presence.agents[0].request.clone();
    h.app.presence_set_harness(&who, "local", "choose").unwrap();
    assert!(h.app.presence_set_harness(&who, "made-up", "stop").is_err());
    assert_eq!(h.app.presence.agents[0].dir, dir);
    assert_eq!(h.app.presence.agents[0].request, request);
    assert!(h.app.presence.agents[0].inflight);
}
#[test]
fn listening_pause_allows_a_message_and_failed_send_keeps_the_draft() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence.agents[0].paused = true;
    h.app.presence_save_draft(&who, "Hello Dot").unwrap();
    h.app.presence_send_chat(&who, "Hello Dot").unwrap();
    assert!(h.app.presence.agents[0].paused);
    assert!(h.app.presence_conversation(&who).unwrap().draft.is_empty());
    let first_request = h.app.presence.agents[0].request.clone();
    h.app.presence_send_chat(&who, "One more question").unwrap();
    assert_eq!(h.app.presence.agents[0].request, first_request);
    assert_eq!(
        h.app
            .presence_conversation(&who)
            .unwrap()
            .turns
            .last()
            .unwrap()
            .status,
        "queued"
    );
    assert!(h.app.presence.agents[0].paused);
    assert!(h.app.presence.agents[0].chat_only);
    h.app.presence_save_draft(&who, "Keep this draft").unwrap();
    let conversation = h.app.presence_conversation(&who).unwrap().clone();
    let request = h.app.presence.agents[0].request.clone();
    let binding = h.app.presence.agents[0].binding.clone();
    let grant = h.app.presence.agents[0].grant.clone();
    // An update preserves the running task; an unavailable workspace must not
    // consume its draft or silently queue a message that cannot be delivered.
    h.app.ai.config.workspace_dir = None;
    let error = h
        .app
        .presence_send_chat(&who, "Keep this draft")
        .unwrap_err();
    assert!(error.contains("workspace"), "{error}");
    assert_eq!(h.app.presence_conversation(&who).unwrap(), &conversation);
    assert_eq!(h.app.presence.agents[0].request, request);
    assert_eq!(h.app.presence.agents[0].binding, binding);
    assert_eq!(h.app.presence.agents[0].grant, grant);
    assert!(h.app.presence.agents[0].paused);
    assert!(h.app.presence.agents[0].chat_only);
    assert!(h.app.presence.agents[0].inflight);
}
#[test]
fn replay_excludes_markers_and_keeps_visible_turns_once_across_a_handoff() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "First question").unwrap();
    let a = &h.app.presence.agents[0];
    let request = a.request.clone();
    let snapshot = AgentSession {
        request: request.clone(),
        conversation: "local-vendor-thread".into(),
        provider: "local".into(),
        turns: vec![
            atlas_agent::AgentTurn {
                role: "user".into(),
                text: "First question".into(),
                at: 1,
            },
            atlas_agent::AgentTurn {
                role: "system".into(),
                text: "private instructions".into(),
                at: 1,
            },
            atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "Visible answer".into(),
                at: 2,
            },
        ],
        ..Default::default()
    };
    let thread = h.app.presence.conversations.edit(&who).unwrap();
    assert!(thread.absorb(a, &snapshot));
    assert!(!thread.absorb(a, &snapshot));
    thread.push("system", "Harness changed", "Slate", None, "", "complete");
    let replay = thread.replay();
    assert_eq!(replay.len(), 2);
    assert_eq!(replay[1].text, "Visible answer");
    assert!(!serde_json::to_string(&replay)
        .unwrap()
        .contains("private instructions"));
}
#[test]
fn task_local_handoff_waits_for_work_and_stop_cancels_the_pending_choice() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "Arrange these").unwrap();
    h.app
        .pin_pack_for_test("codex", atlas_ai::packs::PackHealth::Ok);
    h.app.agents.set_catalog_for_test("codex", vec![]);
    h.app.presence_set_harness(&who, "codex", "choose").unwrap();
    assert_eq!(
        h.app.presence.agents[0].switch_offer.as_deref(),
        Some("codex")
    );
    h.app
        .presence_set_harness(&who, "codex", "after-task")
        .unwrap();
    h.app.finish_presence_handoffs();
    assert_eq!(h.app.presence.agents[0].profile.provider, "local");
    h.app.stop_presence(&who, false).unwrap();
    assert!(h.app.presence.agents[0].pending_harness.is_none());
    assert_eq!(h.app.presence.agents[0].profile.provider, "local");
    assert!(h
        .app
        .presence_conversation(&who)
        .unwrap()
        .turns
        .iter()
        .any(|t| t.status == "stopped"));
}
#[test]
fn model_choices_reject_unknown_parameters_and_are_independent_per_harness() {
    let mut h = host();
    let who = add(&mut h);
    h.app.agents.set_catalog_for_test(
        "local",
        vec![atlas_agent::AgentModel {
            id: "actual".into(),
            name: "Actual".into(),
            ..Default::default()
        }],
    );
    h.app
        .presence_set_model(&who, Some("actual".into()), BTreeMap::new())
        .unwrap();
    assert!(h
        .app
        .presence_set_model(
            &who,
            Some("actual".into()),
            BTreeMap::from([("fake".into(), "bad".into())])
        )
        .is_err());
    assert_eq!(
        h.app.presence_model_choice(&who, "local").model.as_deref(),
        Some("actual")
    );
    assert!(h.app.presence_model_choice(&who, "codex").model.is_none());
}

#[test]
fn successful_handoff_keeps_identity_position_draft_and_history_but_uses_a_fresh_binding() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "Original message").unwrap();
    h.app
        .presence_save_draft(&who, "Unsent next message")
        .unwrap();
    let old = h.app.presence.agents[0].binding.clone();
    let motion = h.app.presence.agents[0].motion.clone();
    let position = h
        .app
        .presence
        .roster
        .session(&motion)
        .unwrap()
        .sample()
        .position;
    h.app
        .pin_pack_for_test("codex", atlas_ai::packs::PackHealth::Ok);
    h.app.agents.set_catalog_for_test("codex", vec![]);
    h.app
        .presence_set_harness(&who, "codex", "after-task")
        .unwrap();
    h.app.presence.agents[0].inflight = false;
    h.app.presence.agents[0].had_response = true;
    h.app.finish_presence_handoffs();
    let a = &h.app.presence.agents[0];
    assert_eq!(a.profile.name, "Dot");
    assert_eq!(a.profile.hat, "cap");
    assert_eq!(a.motion, motion);
    assert_eq!(
        h.app
            .presence
            .roster
            .session(&motion)
            .unwrap()
            .sample()
            .position,
        position
    );
    assert_ne!(a.binding.session_id, old.session_id);
    assert_eq!(
        atlas_ai::access::session_of(&a.dir).as_deref(),
        Some(a.binding.session_id.as_str())
    );
    assert!(a.runtime.is_none() && a.cursor.is_none());
    assert!(!a.grant.as_ref().unwrap().mutation_autonomy);
    let c = h.app.presence_conversation(&who).unwrap();
    assert_eq!(c.draft, "Unsent next message");
    assert_eq!(c.turns[0].text, "Original message");
    assert_eq!(c.bindings.len(), 1); // old binding retained only as provenance
    assert!(c.turns.last().unwrap().text.contains("next message"));
}

#[test]
fn stopping_a_parent_fences_transitive_descendants_and_cancels_ancestor_handoffs() {
    let mut h = host();
    let parent = add(&mut h);
    let helper = add(&mut h);
    let grandchild = add(&mut h);
    h.app.presence.agents[1].parent = Some(parent.clone());
    h.app.presence.agents[2].parent = Some(helper.clone());
    // Defensive lifecycle coverage: public grants currently disallow grandchildren.
    for who in [&parent, &helper, &grandchild] {
        h.app.presence_send_chat(who, "Bounded task").unwrap();
    }
    h.app.presence.agents[0].pending_harness =
        Some(("codex".into(), h.app.presence.agents[0].request.clone()));
    h.app.stop_presence(&grandchild, false).unwrap();
    assert!(h.app.presence.agents[0].pending_harness.is_none());
    assert!(h.app.presence.agents[0].inflight);
    h.app.stop_presence(&parent, true).unwrap();
    assert!(h
        .app
        .presence
        .agents
        .iter()
        .all(|a| a.request.is_empty() && !a.inflight && a.grant.is_none()));
}

#[test]
fn a_request_id_relabel_on_old_provider_turns_cannot_duplicate_an_earlier_reply_or_artifact() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "Same question").unwrap();
    let first_request = h.app.presence.agents[0].request.clone();
    let mut old = AgentSession {
        request: first_request,
        turns: vec![
            atlas_agent::AgentTurn {
                role: "user".into(),
                text: "Same question".into(),
                at: 1,
            },
            atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "Earlier answer".into(),
                at: 2,
            },
        ],
        ..Default::default()
    };
    let a = &h.app.presence.agents[0];
    h.app
        .presence
        .conversations
        .edit(&who)
        .unwrap()
        .absorb(a, &old);
    h.app.presence.agents[0].inflight = false;
    h.app.presence.agents[0].snapshot = Some(Arc::new(old.clone()));
    h.app.presence_send_chat(&who, "Same question").unwrap();
    old.request = h.app.presence.agents[0].request.clone();
    old.status = atlas_agent::AgentStatus::Thinking;
    let a = &h.app.presence.agents[0];
    let thread = h.app.presence.conversations.edit(&who).unwrap();
    assert_eq!(a.reply_from, 2);
    assert!(!thread.absorb(a, &old));
    assert_eq!(
        thread
            .turns
            .iter()
            .filter(|t| t.role == "assistant")
            .count(),
        1
    );
    old.turns.push(atlas_agent::AgentTurn {
        role: "user".into(),
        text: "Same question".into(),
        at: 3,
    });
    old.turns.push(atlas_agent::AgentTurn {
        role: "assistant".into(),
        text: "New answer".into(),
        at: 4,
    });
    old.artifacts.push(atlas_agent::AgentArtifact {
        id: "old-artifact".into(),
        turn: 1,
        kind: atlas_agent::ArtifactKind::Created,
        source: "old.svg".into(),
        title: "Earlier graphic".into(),
    });
    old.artifacts.push(atlas_agent::AgentArtifact {
        id: "new-artifact".into(),
        turn: 3,
        kind: atlas_agent::ArtifactKind::Created,
        source: "new.svg".into(),
        title: "New graphic".into(),
    });
    assert!(thread.absorb(a, &old));
    assert_eq!(
        thread
            .turns
            .iter()
            .filter(|t| t.role == "assistant")
            .count(),
        2
    );
    assert_eq!(thread.artifacts.len(), 1);
    assert_eq!(thread.artifacts[0].artifact.id, "new-artifact");
    assert_eq!(thread.bindings[0].through, thread.turns.len());
}

#[test]
fn human_spawn_helper_obeys_the_same_live_capacity_limit_as_provider_calls() {
    let mut h = host();
    let who = add(&mut h);
    for _ in 0..4 {
        h.app
            .apply_presence_action(
                &h.ctx,
                chrome::PresenceAction::SpawnHelper { id: who.clone() },
            )
            .unwrap();
    }
    let error = h
        .app
        .apply_presence_action(&h.ctx, chrome::PresenceAction::SpawnHelper { id: who })
        .unwrap_err();
    assert!(error.contains("Helper limit"));
    assert_eq!(h.app.presence.agents.len(), 5);
}

#[path = "conversation_regressions.rs"]
mod regressions;

#[path = "conversation_order_tests.rs"]
mod conversation_order_tests;
