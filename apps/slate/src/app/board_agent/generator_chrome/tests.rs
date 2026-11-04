use super::*;
use crate::app::board_flow::tests::generator_with_output;
use slate_doc::scene::{ConnectorEnd, Side};

include!("tests/text_history.rs");

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
        h.app.generator_activity_headline(id),
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
fn global_default_is_machine_local_and_keeps_explicit_denials() {
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
    assert!(atlas_ai::access::granted_in(&path, "new-chat"));
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
fn global_access_ui_covers_all_hosts_and_survives_reload_without_connector_authority_changes() {
    let (mut h, image) = generator_with_output("global-access-hosts");
    let path = h.base.join("global-agent-access.json");
    h.app.set_agent_access_store_for_tests(path.clone());
    let source = crate::app::board_flow::tests::place(&mut h, "reference.png", -700.0);
    let mut hosts = vec![image];
    for kind in ["agent", "text", "artifact"] {
        let detail = serde_json::json!({"source":source.0,"kind":kind}).to_string();
        assert!(h.app.spawn_flow_node(Some(&detail)));
        hosts.push(*h.app.board_sel.iter().next().unwrap());
    }
    let (model, _peer) = crate::app::application_connector::native::test_support::install(
        &mut h,
        atlas_connector::SourceMode::IsolatedDraft,
    );
    crate::app::application_connector::native::test_support::set_surface_descriptor(
        &mut h,
        model,
        atlas_connector::SurfaceDescriptor {
            surface_id: "headless".into(),
            mode: atlas_connector::SurfaceMode::Headless,
            process_ownership: atlas_connector::ProcessOwnership::LaunchedDedicated,
            native_handle: None,
            can_detach: true,
        },
    );
    h.app.patch_nodes(&[model], |n| {
        if let NodeKind::Image(image) = &mut n.kind {
            image.agent = slate_doc::scene::PortalNode::unbound_agent("model", "codex").agent;
        }
    });
    hosts.push(model);
    let binding = h.app.native_binding(model).unwrap();
    for &id in &hosts {
        assert!(!h.app.generator_chrome_data(id).access.unwrap().enabled);
    }
    let scene = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app
        .dispatch_generator_chrome(&h.ctx, image, None, Some(true));
    for &id in &hosts {
        let view = h.app.generator_chrome_data(id).access.unwrap();
        assert!(view.enabled);
        assert!(view.effective);
    }
    let mut restarted = crate::app::tests::Harness::new("global-access-restart");
    restarted.app.set_agent_access_store_for_tests(path.clone());
    assert!(restarted.app.agent_full_access("brand-new-conversation"));
    assert!(restarted.app.dashboard_full_access());
    let denied = slate_doc::agent_chat::agent(h.app.doc().scene.node(image).unwrap())
        .unwrap()
        .session
        .clone();
    h.app.set_agent_session_access(&denied, false).unwrap();
    assert!(!h.app.generator_chrome_data(image).access.unwrap().effective);
    h.app.tab_mut().read_only = true;
    let view = h.app.generator_chrome_data(hosts[1]).access.unwrap();
    assert!(view.enabled);
    assert!(!view.effective);
    assert!(!view.interactive);
    assert_eq!(h.app.doc().scene, scene);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let after = h.app.native_binding(model).unwrap();
    assert_eq!(after.worker, binding.worker);
    assert_eq!(after.observation, binding.observation);
    assert_eq!(after.source_root, binding.source_root);
    assert_eq!(after.binding, binding.binding);
    assert_eq!(after.commands, binding.commands);
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

#[test]
fn repeated_provider_row_lookup_uses_cached_presence_without_pack_probes() {
    let (mut h, id) = generator_with_output("cached-credential-rows");
    h.app
        .set_agent_programs_for_test(&["openai-image", "cursor"]);
    let before = h.app.ai.packs.probes();
    let data = h.app.generator_chrome_data(id);
    let first = chrome::ModelParameters::provider(&data, 0)
        .unwrap()
        .key_present;
    for _ in 0..1000 {
        assert_eq!(
            chrome::ModelParameters::provider(&data, 0)
                .unwrap()
                .key_present,
            first
        );
    }
    assert_eq!(h.app.ai.packs.probes(), before);
}

#[test]
fn admission_refreshes_other_instance_choices_and_pins_the_submitted_policy() {
    for provider in ["cursor", "codex"] {
        let (mut h, id) = generator_with_output(&format!("access-admission-{provider}"));
        h.app.set_agent_program(id, provider);
        let path = h.base.join("shared-access.json");
        h.app.set_agent_access_store_for_tests(path.clone());
        let mut other = crate::app::tests::Harness::new(&format!("access-writer-{provider}"));
        other.app.set_agent_access_store_for_tests(path.clone());
        let session = h.app.agent_session_for(id).unwrap().0;
        let ws = h.base.clone();
        for (turn, enabled, denied) in [(0, true, false), (1, false, false), (2, true, true)] {
            assert!(other.app.dashboard_set_full_access(Some(if enabled {
                "true"
            } else {
                "false"
            })));
            if denied {
                atlas_ai::access::set_in(&path, &session, false).unwrap();
            }
            // The next admission observes this independent process's store write.
            let request = AgentRequest {
                id: format!("turn-{turn}"),
                ..Default::default()
            };
            if provider == "cursor" {
                h.app.agents.sidecar_spawned.insert(session.clone());
            }
            let previous = h.app.agents.sidecar_policies.get(&session).copied();
            assert!(h
                .app
                .submit_prepared_agent_request(id, request.clone(), &ws));
            let policy = h.app.agents.sidecar_policies[&session];
            let expected = enabled && !denied;
            assert_eq!(policy.full_access, Some(expected));
            let view = h.app.generator_chrome_data(id).access.unwrap();
            assert_eq!(view.enabled, enabled);
            assert_eq!(view.effective, expected);
            assert_eq!(
                atlas_ai::runtime::request_full_access(
                    Some(&session),
                    &request,
                    Some(&h.app.agents.access_snapshot)
                ),
                expected
            );
            if provider == "cursor" && previous != Some(policy) {
                assert!(
                    !h.app.agents.sidecar_spawned.contains(&session),
                    "changed Cursor authority retires its old sidecar"
                );
            }
            let admitted = h.app.agents.access_snapshot.clone();
            // Later edits cannot alter the already submitted turn's policy.
            atlas_ai::access::set_global_default_full_access_in(&path, !enabled).unwrap();
            h.app.refresh_agent_access();
            assert_eq!(
                h.app.generator_chrome_data(id).access.unwrap().effective,
                expected
            );
            assert_eq!(
                atlas_ai::runtime::request_full_access(Some(&session), &request, Some(&admitted)),
                expected
            );
        }
    }
}

#[test]
fn escape_dismisses_provider_popup_before_agent_panel_on_actual_frames() {
    let (mut h, host) = generator_with_output("provider-escape-parent");
    h.app.sync_shape_properties();
    h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Agent);
    h.app.restore_generator_presentation(
        host,
        chrome::GeneratorPresentation {
            model_open: true,
            picker_page: chrome::PickerPage::Providers,
            ..Default::default()
        },
    );
    h.frame();
    assert!(h.app.generator_presentation(host).model_open);
    let depth = h.app.tab().journal.undo_depth();
    let escape = |input: &mut egui::RawInput| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    };
    h.frame_with(escape);
    assert!(!h.app.generator_presentation(host).model_open);
    assert_eq!(
        h.app.shape_properties.panel,
        Some(crate::app::board_properties::Panel::Agent)
    );
    assert!(h.app.board_sel.contains(&host));
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    h.frame_with(escape);
    assert_eq!(
        h.app.shape_properties.panel, None,
        "second explicit Escape closes the parent panel"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
