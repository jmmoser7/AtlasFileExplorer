use super::*;
use crate::app::board_flow::tests::generator_with_output;
use slate_doc::scene::{ConnectorEnd, Side};

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
fn text_output_selection_is_persisted_display_state_without_rewinding_latest_reply() {
    let (mut h, id) = generator_with_output("text-shown-output");
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.bundle = Some(slate_doc::scene::SourceUri {
            locator: "history/session.json".into(),
        });
    });
    let turns = [
        ("user", "first"),
        ("assistant", "old output"),
        ("user", "next"),
        ("assistant", "latest output"),
    ]
    .into_iter()
    .map(|(role, text)| atlas_agent::AgentTurn {
        role: role.into(),
        text: text.into(),
        at: 0,
    })
    .collect();
    h.app.agents.sessions.insert(
        id,
        std::sync::Arc::new(AgentSession {
            turns,
            ..Default::default()
        }),
    );
    h.app.patch_nodes(&[id], |node| {
        let mut agent = slate_doc::agent_chat::agent(node).unwrap().clone();
        agent.view = atlas_agent::PortalView::Text;
        let mut text = slate_doc::scene::TextNode::agent_note(agent);
        text.text = "Authored words".into();
        node.kind = NodeKind::Text(text);
    });
    let before = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.select_text_output(Some(
        &serde_json::json!({"node":id,"through":2}).to_string()
    )));
    assert_eq!(h.app.agent_reply(id).as_deref(), Some("latest output"));
    assert_eq!(
        h.app.displayed_text_output(id).as_deref(),
        Some("old output")
    );
    let selected = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(selected.session, before.session);
    assert_eq!(selected.channel, before.channel);
    assert_eq!(selected.model_parameters, before.model_parameters);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let roundtrip: slate_doc::scene::AgentPortalRef =
        serde_json::from_value(serde_json::to_value(selected).unwrap()).unwrap();
    assert_eq!(
        roundtrip
            .generator
            .shown_text
            .as_ref()
            .unwrap()
            .response
            .through,
        2
    );
    assert!(!h.app.select_text_output(Some(
        &serde_json::json!({"node":id,"through":1}).to_string()
    )));
    assert_eq!(
        h.app.agent_note_reply(id).as_deref(),
        Some("old output"),
        "visible text must match selected downstream snapshot despite existing authored text"
    );
    assert_eq!(
        h.app
            .export_agent_replies()
            .unwrap()
            .get(&id)
            .map(String::as_str),
        Some("old output")
    );
    h.app.place_agent_portal_at(egui::pos2(900.0, 0.0));
    let downstream = *h.app.board_sel.iter().next().unwrap();
    let wire = h.app.build_connector_bound(
        ConnectorEnd::Anchored {
            node: id,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: downstream,
            side: Side::Top,
            t: 0.3,
        },
        &[],
        Some(slate_doc::agent_inputs::WireBinding {
            input_b: true,
            kind: slate_doc::agent_inputs::InputKind::Text,
            output: None,
            order: vec![],
            all_images: false,
            consumed: false,
            slot: Some(atlas_agent::InputSlot::Prompt.id().into()),
        }),
    );
    h.app.add_nodes(vec![wire]);
    let wired_text = |app: &SlateApp| {
        let snapshot = app.agent_input_snapshot(downstream).unwrap();
        let text = snapshot
            .wired
            .iter()
            .find(|item| item.node == id.0)
            .unwrap()
            .text
            .clone();
        let jobs = slate_doc::agent_inputs::plan_grafted_inputs(
            &snapshot,
            &[atlas_agent::InputSlot::Prompt.id().to_string()]
                .into_iter()
                .collect(),
            1,
        )
        .unwrap();
        assert_eq!(
            jobs[0]
                .inputs
                .wired
                .iter()
                .find(|item| item.node == id.0)
                .unwrap()
                .text,
            text
        );
        text
    };
    assert_eq!(wired_text(&h.app), "old output");
    let depth = h.app.tab().journal.undo_depth();
    h.app.text_edit = Some((id, "old output".into()));
    assert_eq!(wired_text(&h.app), "old output");
    h.app.text_edit.as_mut().unwrap().1 = "Live native words".into();
    assert_eq!(wired_text(&h.app), "Live native words");
    assert_eq!(
        h.app.resolved_selected_text_output(id).unwrap(),
        None,
        "editor suspends preview before any authored change"
    );
    h.app.text_edit = None; // Escape
    assert_eq!(wired_text(&h.app), "old output");
    assert_eq!(h.app.agent_note_reply(id).as_deref(), Some("old output"));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.text_edit = Some((id, "old output".into()));
    h.app.commit_text_edit(); // no-op preserves the display reference
    assert_eq!(wired_text(&h.app), "old output");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.text_edit = Some((id, "Edited author words".into()));
    h.app.commit_text_edit();
    assert_eq!(wired_text(&h.app), "Edited author words");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .generator
            .shown_text
            .is_none()
    );
    assert!(h.app.agent_note_reply(id).is_none());
    assert!(
        matches!(&h.app.doc().scene.node(id).unwrap().kind,NodeKind::Text(text) if text.text=="Edited author words")
    );
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.undo"), None);
    assert_eq!(h.app.agent_note_reply(id).as_deref(), Some("old output"));
    let saved = serde_json::to_value(h.app.doc().scene.node(id).unwrap()).unwrap();
    let loaded: slate_doc::scene::Node = serde_json::from_value(saved).unwrap();
    assert!(slate_doc::agent_chat::agent(&loaded)
        .unwrap()
        .generator
        .shown_text
        .is_some());
    // The same downstream wire must use the linked editor buffer, even though
    // pure snapshot dispatch still sees the durable selected-output reference.
    let item = h
        .app
        .doc_mut()
        .add_item("linked.txt".into(), "linked.txt", 10, 0, "linked-edit");
    h.app
        .cache_snippet(item, Some("Linked authored words".into()));
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent(node).unwrap().clone();
        let mut image = slate_doc::scene::ImageNode::new(item);
        image.agent = Some(Box::new(agent));
        node.kind = NodeKind::Image(image);
    });
    assert_eq!(wired_text(&h.app), "old output");
    let edit = crate::app::board::TextDocEdit {
        node: id,
        item,
        path: "linked.txt".into(),
        rect: h.app.doc().scene.node(id).unwrap().rect,
        buffer: "Linked authored words".into(),
        original: "Linked authored words".into(),
        claim_focus: false,
        auto_size: false,
    };
    let depth = h.app.tab().journal.undo_depth();
    h.app.text_doc_edit = Some(edit.clone());
    h.app.text_doc_edit.as_mut().unwrap().buffer = "Live linked words".into();
    assert_eq!(wired_text(&h.app), "Live linked words");
    h.app.text_doc_edit = None; // Escape restores preview and wire selection.
    assert_eq!(wired_text(&h.app), "old output");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.text_doc_edit = Some(edit.clone());
    h.app.commit_text_doc_edit(); // No-op keeps the durable output selection.
    assert_eq!(wired_text(&h.app), "old output");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.text_doc_edit = Some(edit);
    h.app.text_doc_edit.as_mut().unwrap().buffer = "Committed linked words".into();
    assert_eq!(wired_text(&h.app), "Committed linked words");
    h.app.commit_text_doc_edit();
    assert_eq!(wired_text(&h.app), "Committed linked words");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app
        .dispatch(&h.ctx, atlas_commands::CommandId("app.undo"), None);
    assert_eq!(wired_text(&h.app), "old output");
    let loaded: slate_doc::scene::Node =
        serde_json::from_value(serde_json::to_value(h.app.doc().scene.node(id).unwrap()).unwrap())
            .unwrap();
    h.app.patch_nodes(&[id], |node| *node = loaded.clone());
    assert_eq!(wired_text(&h.app), "old output");
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .shown_text
            .as_mut()
            .unwrap()
            .response
            .through = 999
    });
    assert!(h.app.resolved_selected_text_output(id).is_err());
    assert!(
        h.app.export_agent_replies().is_err(),
        "export must not serialize a UI unavailable-source placeholder"
    );
    assert!(h.app.agent_note_reply(id).unwrap().contains("unavailable"));
    assert_eq!(h.app.agent_reply(id).as_deref(), Some("latest output"));
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
fn answer_navigation_same_session_uses_exact_recorded_manifest() {
    let (mut h, id) = generator_with_output("same-session-origin");
    let origin = h.base.join("recorded-custom-bundle");
    std::fs::create_dir_all(&origin).unwrap();
    let manifest = origin.join("session.json");
    let archived = AgentSession {
        turns: vec![atlas_agent::AgentTurn {
            role: "assistant".into(),
            text: "Recorded answer".into(),
            at: 0,
        }],
        ..Default::default()
    };
    std::fs::write(&manifest, serde_json::to_vec(&archived).unwrap()).unwrap();
    let active = AgentSession {
        turns: vec![atlas_agent::AgentTurn {
            role: "assistant".into(),
            text: "Different active answer".into(),
            at: 0,
        }],
        ..Default::default()
    };
    h.app
        .agents
        .sessions
        .insert(id, std::sync::Arc::new(active));
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().bundle =
            Some(slate_doc::scene::SourceUri {
                locator: "another-custom-bundle/session.json".into(),
            });
    });
    let before = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .clone();
    let target = slate_doc::text_answers::TextOutputRef {
        review_id: None,
        source: slate_doc::scene::SourceUri {
            locator: manifest.to_string_lossy().into_owned(),
        },
        response: slate_doc::text_answers::TextAnswerRef {
            session: before.session.clone(),
            through: 1,
        },
    };
    assert!(!h.app.text_output_is_current(id, &target));
    h.app.agents.flow.text_answer_navigation = Some((h.app.tab().id, id, target));
    for _ in 0..400 {
        h.app.apply_text_answer_navigation();
        if h.app.agents.flow.text_answer_navigation.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.app.agents.flow.text_answer_navigation.is_none());
    assert_eq!(
        h.app
            .generator_chrome_data(id)
            .entries()
            .get(0)
            .unwrap()
            .text,
        "Recorded answer"
    );
    assert_eq!(
        h.app.agents.sessions[&id].turns[0].text,
        "Different active answer"
    );
    let after = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(after.session, before.session);
    assert_eq!(after.bundle, before.bundle);
    assert_eq!(after.channel, before.channel);
    assert_eq!(after.generator.history_cut, before.generator.history_cut);
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
