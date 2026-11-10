//! Public Model memory is captured by the actual request builder, not a native file.
use super::*;
use atlas_agent::{AgentSession, AgentTurn};

fn reset_public_runtime(h: &mut Harness, provider: &str) {
    h.app.agents = Default::default();
    h.app.set_agent_programs_for_test(&[provider]);
    h.app.set_agent_catalog_for_test(
        provider,
        vec![atlas_agent::AgentModel {
            id: "native-task-model".into(),
            name: "Native task model".into(),
            is_default: true,
            capabilities: Some(atlas_agent::ModelCapabilities {
                text: true,
                model_tools: true,
                vision: true,
                ..Default::default()
            }),
            ..Default::default()
        }],
    );
}

fn configured(provider: &str) -> (Harness, NodeId, test_support::Peer, AgentRequest) {
    let (mut h, node, peer) = fixture(&format!("model-memory-{provider}"));
    h.app
        .set_agent_programs_for_test(&["codex", "claude-code", "cursor-cli"]);
    h.app.set_agent_catalog_for_test(
        provider,
        vec![atlas_agent::AgentModel {
            id: "native-task-model".into(),
            name: "Native task model".into(),
            is_default: true,
            capabilities: Some(atlas_agent::ModelCapabilities {
                text: true,
                model_tools: true,
                vision: true,
                ..Default::default()
            }),
            ..Default::default()
        }],
    );
    h.app.settings.agent_artifact_model = Some((provider.into(), "native-task-model".into()));
    submit(&mut h, node);
    let first = h.app.agents.dispatched.last().unwrap().1.clone();
    assert!(first.history.is_empty());
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap())
            .unwrap()
            .generator
            .preserve_history,
        Some(false)
    );
    h.app.cancel_model_task((h.app.tab().id, node));
    (h, node, peer, first)
}

fn public_reply(
    h: &mut Harness,
    node: NodeId,
    first: &AgentRequest,
    provider: &str,
) -> Vec<AgentTurn> {
    let turns = vec![
        AgentTurn {
            role: "user".into(),
            text: first.prompt.clone(),
            at: 1,
        },
        AgentTurn {
            role: "assistant".into(),
            text: "Earlier public model reply.".into(),
            at: 2,
        },
        AgentTurn {
            role: "assistant".into(),
            text: String::new(),
            at: 3,
        },
    ];
    let session = AgentSession {
        provider: provider.into(),
        request: first.id.clone(),
        conversation: "public-model-conversation".into(),
        status: atlas_agent::AgentStatus::Idle,
        turns: turns.clone(),
        ..Default::default()
    };
    let dir = h.app.agent_link_dir(node, &h.base).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &session).unwrap();
    // The normal source pump, including local-user echo reconciliation, loads it.
    h.app.set_agent_session_for_test(
        node,
        AgentSession {
            provider: provider.into(),
            ..Default::default()
        },
    );
    wait_public(h, node, &turns);
    turns
}

fn wait_public(h: &mut Harness, node: NodeId, turns: &[AgentTurn]) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        h.app.agent_pump(&h.ctx);
        if h.app
            .agents
            .session(node)
            .is_some_and(|session| session.turns == turns)
        {
            break;
        }
        assert!(
            Instant::now() < end,
            "saved public Model bundle did not load"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn stage(h: &mut Harness, node: NodeId) -> AgentRequest {
    try_stage(h, node);
    let key = (h.app.tab().id, node);
    let request = h
        .app
        .connectors
        .headless
        .tasks
        .staging
        .get(&key)
        .expect("real Model builder staged its accepted request")
        .request
        .clone();
    let prompt = "Refine the current isolated model using its declared inputs.";
    assert_eq!(request.prompt, prompt);
    assert_eq!(request.input_text().matches(prompt).count(), 1);
    assert!(request.inputs.context.is_empty());
    request
}

fn try_stage(h: &mut Harness, node: NodeId) {
    let prompt = "Refine the current isolated model using its declared inputs.";
    h.app.prepare_model_task_request(
        node,
        prompt.into(),
        h.base.clone(),
        InputSnapshot::default(),
        prompt.into(),
        InputSnapshot::default(),
    );
}

#[test]
fn model_memory_on_replays_public_checkpoint_after_prior_off_and_off_stays_empty() {
    for provider in ["claude-code", "cursor-cli", "codex"] {
        for preserve in [false, true] {
            let (mut h, node, _peer, first) = configured(provider);
            let turns = public_reply(&mut h, node, &first, provider);
            h.app.patch_nodes(&[node], |n| {
                slate_doc::agent_chat::agent_mut(n)
                    .unwrap()
                    .generator
                    .preserve_history = Some(preserve)
            });
            let request = stage(&mut h, node);
            if preserve {
                assert_eq!(request.history, turns[..2]);
                assert_eq!(
                    request
                        .input_text()
                        .matches("Earlier public model reply.")
                        .count(),
                    1
                );
            } else {
                assert!(request.history.is_empty());
                assert!(!request.input_text().contains("Earlier public model reply."));
            }
        }
    }
}

#[test]
fn reopened_model_memory_loads_its_public_bundle_and_replays_once() {
    for provider in ["claude-code", "cursor-cli"] {
        let (mut h, node, _peer, first) = configured(provider);
        let turns = public_reply(&mut h, node, &first, provider);
        h.app.patch_nodes(&[node], |n| {
            slate_doc::agent_chat::agent_mut(n)
                .unwrap()
                .generator
                .preserve_history = Some(true)
        });
        let saved = h.base.join("saved-model-memory.slate");
        h.app.doc().save_to(&saved).unwrap();
        h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&saved).unwrap();
        h.app.tab_mut().path = Some(saved);
        // Reopening has no derived transcript/source cache yet. An immediate
        // Send must either load the actual checkpoint or refuse, never pay with [].
        reset_public_runtime(&mut h, provider);
        *h.app.agents.prompt_mut(node) = "Unsent refinement stays authored.".into();
        try_stage(&mut h, node);
        let key = (h.app.tab().id, node);
        if let Some(staged) = h.app.connectors.headless.tasks.staging.remove(&key) {
            assert_eq!(staged.request.history, turns[..2]);
        } else {
            assert!(h
                .app
                .agent_failure_reason(node)
                .is_some_and(|reason| reason.contains("history is still loading")));
        }
        assert!(h.app.agents.dispatched.is_empty());
        assert_eq!(
            h.app.agents.prompt(node),
            Some("Unsent refinement stays authored.")
        );
        wait_public(&mut h, node, &turns);
        let request = stage(&mut h, node);
        assert_eq!(request.history, turns[..2]);
        assert_eq!(
            request
                .input_text()
                .matches("Earlier public model reply.")
                .count(),
            1
        );
    }
}

#[test]
fn model_memory_uses_completed_source_before_stale_nonempty_node_cache() {
    for provider in ["claude-code", "cursor-cli", "codex"] {
        let (mut h, node, _peer, first) = configured(provider);
        let turns = public_reply(&mut h, node, &first, provider);
        // The source completion pump can finish while the node still displays
        // the previous user row and an empty streaming assistant placeholder.
        h.app.set_agent_session_for_test(
            node,
            AgentSession {
                provider: provider.into(),
                request: first.id.clone(),
                status: atlas_agent::AgentStatus::Idle,
                turns: vec![turns[0].clone(), turns[2].clone()],
                ..Default::default()
            },
        );
        h.app.patch_nodes(&[node], |n| {
            slate_doc::agent_chat::agent_mut(n)
                .unwrap()
                .generator
                .preserve_history = Some(true);
        });
        let request = stage(&mut h, node);
        assert_eq!(request.history, turns[..2]);
        assert_eq!(
            request
                .input_text()
                .matches("Earlier public model reply.")
                .count(),
            1
        );
    }
}

#[test]
fn new_model_memory_on_accepts_confirmed_empty_history_and_off_or_legacy_do_not_wait() {
    for preserve in [Some(true), Some(false), None] {
        let (mut h, node, _peer, _) = configured("claude-code");
        assert!(!h
            .app
            .agent_link_dir(node, &h.base)
            .unwrap()
            .join("session.json")
            .exists());
        reset_public_runtime(&mut h, "claude-code");
        h.app.patch_nodes(&[node], |n| {
            slate_doc::agent_chat::agent_mut(n)
                .unwrap()
                .generator
                .preserve_history = preserve;
        });
        let key = (h.app.tab().id, node);
        let dispatched = h.app.agents.dispatched.len();
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            try_stage(&mut h, node);
            if let Some(staged) = h.app.connectors.headless.tasks.staging.get(&key) {
                assert!(staged.request.history.is_empty());
                break;
            }
            assert_eq!(
                preserve,
                Some(true),
                "OFF/legacy must not wait for Model memory"
            );
            assert!(
                Instant::now() < end,
                "new ON conversation never became ready"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(h.app.agents.dispatched.len(), dispatched);
    }
}

#[test]
fn unavailable_model_memory_never_stages_or_consumes_the_refinement() {
    let (mut h, node, _peer, _) = configured("claude-code");
    let dir = h.app.agent_link_dir(node, &h.base).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session.json"), b"not a public session").unwrap();
    reset_public_runtime(&mut h, "claude-code");
    h.app.patch_nodes(&[node], |n| {
        slate_doc::agent_chat::agent_mut(n)
            .unwrap()
            .generator
            .preserve_history = Some(true);
    });
    *h.app.agents.prompt_mut(node) = "Keep this unsent refinement.".into();
    let key = (h.app.tab().id, node);
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        try_stage(&mut h, node);
        assert!(!h.app.connectors.headless.tasks.staging.contains_key(&key));
        assert!(h.app.agents.dispatched.is_empty());
        assert_eq!(
            h.app.agents.prompt(node),
            Some("Keep this unsent refinement.")
        );
        if h.app
            .agent_failure_reason(node)
            .is_some_and(|reason| reason.contains("history is unavailable"))
        {
            break;
        }
        assert!(Instant::now() < end, "unavailable history was not reported");
        std::thread::sleep(Duration::from_millis(5));
    }
}
