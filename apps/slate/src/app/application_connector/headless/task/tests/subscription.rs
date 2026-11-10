use super::*;

#[test]
fn subscription_model_task_reuses_scoped_admission_and_fences_the_captured_provider() {
    for provider in ["claude-code", "cursor-cli"] {
        let (mut h, node, _peer) = fixture(&format!("model-task-{provider}"));
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
                    ..Default::default()
                }),
                ..Default::default()
            }],
        );
        h.app.settings.agent_artifact_model = Some((provider.into(), "native-task-model".into()));
        submit(&mut h, node);
        let key = (h.app.tab().id, node);
        let task = &h.app.connectors.headless.tasks.active[&key];
        assert_eq!(task.target.provider, provider);
        assert!(task.validate(&h.app, true).is_ok());
        assert!(!h.app.presence.has_live_agents(h.app.tab().id));
        assert!(h.app.attached_model_native_access(node).is_none());
        h.app.patch_nodes(&[node], |n| {
            slate_doc::agent_chat::agent_mut(n).unwrap().provider = "codex".into()
        });
        h.app.pump_model_tasks(&h.ctx);
        assert!(!h.app.model_task_is_running(node));
        assert!(h
            .app
            .agent_failure_reason(node)
            .unwrap()
            .contains("changed"));
    }
}

#[test]
fn provider_text_completion_without_a_native_model_is_a_visible_failure() {
    let (mut h, node, _peer) = fixture("model-task-no-artifact");
    submit(&mut h, node);
    let key = (h.app.tab().id, node);
    let task = &h.app.connectors.headless.tasks.active[&key];
    let request = task.permit.identity.request.clone();
    let dir = task.provider_dir.clone();
    let before = serde_json::to_vec(h.app.doc()).unwrap();
    atlas_ai::agent::atomic_write_json(
        &dir.join("session.json"),
        &atlas_agent::AgentSession {
            request,
            status: atlas_agent::AgentStatus::Idle,
            provider: "codex".into(),
            turns: vec![atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "Completed!".into(),
                at: 1,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.app.model_task_is_running(node) && Instant::now() < deadline {
        h.app.pump_model_tasks(&h.ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!h.app.model_task_is_running(node));
    assert!(h
        .app
        .agent_failure_reason(node)
        .unwrap()
        .contains("without a verified native model"));
    assert_eq!(serde_json::to_vec(h.app.doc()).unwrap(), before);
}
