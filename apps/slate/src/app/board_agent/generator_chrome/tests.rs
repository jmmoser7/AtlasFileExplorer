use super::*;
use crate::app::board_flow::tests::generator_with_output;

#[test]
fn parameters_dispatch_provider_ids_and_model_switch_drops_stale_values() {
    let (mut h, id) = generator_with_output("generator-parameter-command");
    h.app.patch_nodes(&[id], |n| {
        let a = slate_doc::agent_chat::agent_mut(n).unwrap();
        a.provider = "codex".into();
        a.model = Some("advertised".into());
    });
    h.app.agents.models.insert(
        "codex".into(),
        vec![serde_json::from_value(serde_json::json!({
            "id":"advertised","name":"Advertised", "parameters":[
                {"id":"effort","name":"Thought level","values":[{"value":"high","name":"High"}]}
            ]
        }))
        .unwrap()],
    );
    let depth = h.app.tab().journal.undo_depth();
    let choice =
        |value| serde_json::json!({"node":id,"parameter":"effort","value":value}).to_string();
    assert!(!h.app.generator_set_parameter(Some(&choice("invented"))));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.generator_set_parameter(Some(&choice("high"))));
    let request = h.app.generation_request(id, false).unwrap();
    assert_eq!(
        request.model_parameters.get("effort").map(String::as_str),
        Some("high")
    );
    h.app.set_flow_model(id, true, "codex", "next");
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .model_parameters
            .is_empty()
    );
    assert_eq!(
        request.model_parameters.get("effort").map(String::as_str),
        Some("high")
    );
}

#[test]
fn activity_shows_only_the_current_request_and_does_not_enter_history() {
    let (mut h, id) = generator_with_output("generator-public-activity");
    let mut session = AgentSession {
        request: "current".into(),
        ..Default::default()
    };
    session.activity = ["old", "current"]
        .into_iter()
        .map(|request| atlas_agent::AgentActivity {
            id: request.into(),
            request: request.into(),
            kind: atlas_agent::AgentActivityKind::Commentary,
            text: format!("Building {request}"),
            at: 0,
            semantics: None,
        })
        .collect();
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(session));
    h.app.agents.requests.insert(id, "current".into());
    assert_eq!(
        h.app.generator_activity_headline(id).as_deref(),
        Some("Building current")
    );
    let data = h.app.generator_chrome_data(id);
    assert_eq!(data.entries().len(), 1);
    assert_eq!(data.entries().get(0).unwrap().text, "Building current");
    assert!(data.session.as_ref().unwrap().turns.is_empty());
    h.app.agents.requests.insert(id, "next".into());
    assert!(h.app.generator_activity_headline(id).is_none());
    assert!(h.app.generator_chrome_data(id).entries().is_empty());
}

#[test]
fn dashboard_default_is_machine_local_and_keeps_explicit_denials() {
    let (mut h, id) = generator_with_output("dashboard-global-permission");
    let path = h.base.join("test-agent-access.json");
    h.app.agents.access_path = Some(path.clone());
    atlas_ai::access::set_artifact_override_in(&path, "denied", Some(false)).unwrap();
    let before = h.app.doc().scene.nodes.clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.dashboard_set_full_access(Some("true")));
    assert!(h.app.dashboard_full_access());
    assert!(atlas_ai::access::artifact_granted_in(
        &path,
        "new-dashboard"
    ));
    assert!(!atlas_ai::access::artifact_granted_in(&path, "denied"));
    assert!(!atlas_ai::access::granted_in(&path, "new-chat"));
    assert_eq!(h.app.doc().scene.nodes, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.doc().scene.node(id).is_some());
    assert!(h.app.dashboard_set_full_access(Some("false")));
    assert!(!atlas_ai::access::artifact_granted_in(
        &path,
        "new-dashboard"
    ));
}

#[test]
fn artifact_model_defaults_do_not_change_text_or_image_spawns() {
    use crate::app::board_flow::SpawnKind;
    let (mut h, _) = generator_with_output("isolated-generator-defaults");
    let source = crate::app::board_flow::tests::place(&mut h, "reference.png", -700.0);
    h.app.remember_model(false, "openai-text", "text-model");
    h.app.remember_model(true, "openai-image", "image-model");
    h.app
        .remember_flow_model(SpawnKind::Artifact, "cursor", "artifact-model");
    assert_eq!(
        h.app.default_model(false),
        Some(("openai-text".into(), "text-model".into()))
    );
    assert_eq!(
        h.app.default_model(true),
        Some(("openai-image".into(), "image-model".into()))
    );
    assert_eq!(
        h.app.default_flow_model(SpawnKind::Artifact),
        Some(("cursor".into(), "artifact-model".into()))
    );
    for (kind, provider, model) in [
        ("artifact", "cursor", "artifact-model"),
        ("text", "openai-text", "text-model"),
        ("image", "openai-image", "image-model"),
    ] {
        let detail = serde_json::json!({"source":source.0,"kind":kind}).to_string();
        assert!(h.app.spawn_flow_node(Some(&detail)));
        let selected = *h.app.board_sel.iter().next().unwrap();
        let agent =
            slate_doc::agent_chat::agent(h.app.doc().scene.node(selected).unwrap()).unwrap();
        assert_eq!(agent.provider, provider);
        assert_eq!(agent.model.as_deref(), Some(model));
    }
}

#[test]
fn legacy_settings_do_not_seed_artifact_defaults_or_mix_provider_kinds() {
    use crate::app::board_flow::SpawnKind;
    let (mut h, _) = generator_with_output("legacy-generator-defaults");
    h.app.settings = serde_json::from_value(serde_json::json!({
        "agent_image_model":["comfy","legacy-image"],
        "agent_text_model":["codex","accidentally-saved-coding-model"]
    }))
    .unwrap();
    assert!(h.app.settings.agent_artifact_model.is_none());
    assert!(h.app.default_flow_model(SpawnKind::Artifact).is_none());
    assert!(
        h.app.default_model(false).is_none(),
        "an incompatible saved provider cannot become Ollama with a coding-model ID"
    );
    assert_eq!(
        h.app.default_model(true),
        Some(("comfy".into(), "legacy-image".into()))
    );
    h.app
        .remember_flow_model(SpawnKind::Artifact, "codex", "artifact-model");
    let restored: crate::app::settings::SlateSettings =
        serde_json::from_value(serde_json::to_value(&h.app.settings).unwrap()).unwrap();
    assert_eq!(
        restored.agent_artifact_model,
        Some(("codex".into(), "artifact-model".into()))
    );
    assert_eq!(
        restored.agent_text_model,
        Some(("codex".into(), "accidentally-saved-coding-model".into()))
    );
}
