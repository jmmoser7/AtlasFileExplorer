use super::*;

#[test]
fn subscription_artifact_picker_and_bound_admission_require_exact_task_capability() {
    for provider in ["cursor-cli", "claude-code"] {
        let (mut h, _) = subscription_fixture(provider);
        let source = super::super::super::tests::place(&mut h, "artifact-reference.png", 1200.0);
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.spawn_flow_node(Some(
            &serde_json::json!({
                "source":source.0,"kind":"artifact","chooser":true,"at":[1500.0,100.0]
            })
            .to_string()
        )));
        let target = h.app.doc().scene.nodes[before].id;
        assert!(h.app.source_model_options(target, provider).is_empty());
        for (artifact, vision, accepted) in [
            (true, false, false),
            (true, true, true),
            (false, true, false),
        ] {
            h.app.set_agent_catalog_for_test(
                provider,
                vec![atlas_agent::AgentModel {
                    id: String::new(),
                    name: "Verified task model".into(),
                    is_default: true,
                    capabilities: Some(atlas_agent::ModelCapabilities {
                        text: true,
                        artifact,
                        vision,
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
            );
            assert_eq!(
                !h.app.source_model_options(target, provider).is_empty(),
                accepted
            );
            if accepted {
                assert!(h.app.set_pending_generator_provider(target, provider));
                let mut draft = h.app.agents.flow.drafts[&target].clone();
                draft.prompt = "Build an interactive view of the connected reference.".into();
                assert!(h.app.pending_submission_choice(target, &draft).is_some());
                assert!(h.app.bind_pending_generator(
                    target,
                    PendingHost::Artifact,
                    &draft,
                    provider,
                    ""
                ));
                assert!(h.app.validate_subscription_source_choice(target).is_ok());
            } else if h
                .app
                .doc()
                .scene
                .node(target)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.provider == provider)
            {
                assert!(h.app.validate_subscription_source_choice(target).is_err());
            }
        }
    }
}

fn subscription_fixture(provider: &str) -> (Harness, NodeId) {
    let mut h = prepare(&format!("subscription-{provider}"));
    h.app
        .set_agent_programs_for_test(&["codex", "cursor-cli", "claude-code"]);
    h.app.set_agent_catalog_for_test(
        provider,
        vec![atlas_agent::AgentModel {
            id: String::new(),
            name: "Account default".into(),
            is_default: true,
            capabilities: Some(atlas_agent::ModelCapabilities::text_only()),
            ..Default::default()
        }],
    );
    let source = super::super::super::tests::place(&mut h, "plain.txt", -600.0);
    let NodeKind::Image(image) = &h.app.doc().scene.node(source).unwrap().kind else {
        panic!()
    };
    let item = image.item;
    h.app.cache_snippet(
        item,
        Some("Owned linked source for subscription review.".into()),
    );
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({
            "source":source.0,"kind":"text","chooser":true,"at":[300.0,100.0]
        })
        .to_string()
    )));
    let target = h.app.doc().scene.nodes[before].id;
    assert!(h.app.set_pending_generator_provider(target, provider));
    h.app.agents.flow.drafts.get_mut(&target).unwrap().prompt =
        "Summarize the supplied text.".into();
    (h, target)
}

#[test]
fn subscription_text_uses_shared_first_send_and_projects_linked_source_once() {
    for provider in ["cursor-cli", "claude-code"] {
        let (mut h, target) = subscription_fixture(provider);
        h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId("portal.agent.send"),
            Some(serde_json::json!({"node":target}).to_string()),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.app.agents.dispatched.is_empty() && std::time::Instant::now() < deadline {
            h.app.pump_comfy_queue();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(h.app.agents.dispatched.len(), 1);
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap()).unwrap();
        assert_eq!(agent.provider, provider);
        assert!(!agent.session.is_empty());
        let request = &h.app.agents.dispatched[0].1;
        assert_eq!(
            request
                .prompt_with_history()
                .matches("Owned linked source for subscription review.")
                .count(),
            1
        );
        assert!(
            request.image.is_none() && request.artifact.is_none() && request.collaborator.is_none()
        );
    }
}

#[test]
fn subscription_text_rejects_stale_choices_and_never_offers_other_output_tasks() {
    for provider in ["cursor-cli", "claude-code"] {
        let (mut h, target) = subscription_fixture(provider);
        {
            let draft = h.app.agents.flow.drafts.get_mut(&target).unwrap();
            draft.choice = Some((provider.into(), "unadvertised-model".into()));
        }
        h.app.prepare_pending_chat_choice(target);
        let mut draft = h.app.agents.flow.drafts[&target].clone();
        assert_eq!(draft.choice.as_ref().unwrap().1, "unadvertised-model");
        assert!(h.app.pending_submission_choice(target, &draft).is_none());
        draft.choice = Some((provider.into(), String::new()));
        draft
            .model_parameters
            .insert("invented-speed".into(), "fast".into());
        assert!(h.app.pending_submission_choice(target, &draft).is_none());
        assert!(h.app.agents.dispatched.is_empty());
        assert!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
                .unwrap()
                .provider
                .is_empty()
        );
        let picture = super::super::super::tests::place(&mut h, "visual.png", 1200.0);
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.spawn_flow_node(Some(
            &serde_json::json!({
                "source":picture.0,"kind":"text","chooser":true,"at":[1500.0,100.0]
            })
            .to_string()
        )));
        let visual_text = h.app.doc().scene.nodes[before].id;
        assert!(!h
            .app
            .pending_provider_options(visual_text)
            .iter()
            .any(|p| p.id == provider));
        for file in ["source.png", "shape.obj"] {
            let other = super::super::super::tests::place(&mut h, file, 1000.0);
            assert!(!h
                .app
                .pending_provider_options(other)
                .iter()
                .any(|p| p.id == provider));
        }
        let web = h.app.doc_mut().scene.build_node(
            slate_doc::WorldRect::new(0.0, 0.0, 640.0, 360.0),
            NodeKind::Portal(PortalNode::bound_web(
                "Dashboard",
                "https://example.invalid/",
            )),
        );
        let id = web.id;
        h.app.add_nodes(vec![web]);
        assert!(!h
            .app
            .pending_provider_options(id)
            .iter()
            .any(|p| p.id == provider));
    }
}

#[test]
fn subscription_visual_input_uses_real_vision_capability_independently_of_image_output() {
    for provider in ["cursor-cli", "claude-code"] {
        let (mut h, _) = subscription_fixture(provider);
        let picture = super::super::super::tests::place(&mut h, "visual.png", 1200.0);
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.spawn_flow_node(Some(&serde_json::json!({"source":picture.0,"kind":"text","chooser":true,"at":[1500.0,100.0]}).to_string())));
        let target = h.app.doc().scene.nodes[before].id;
        assert!(h.app.source_model_options(target, provider).is_empty());
        h.app.set_agent_catalog_for_test(
            provider,
            vec![atlas_agent::AgentModel {
                id: String::new(),
                name: "Vision account default".into(),
                is_default: true,
                capabilities: Some(atlas_agent::ModelCapabilities {
                    text: true,
                    vision: true,
                    ..Default::default()
                }),
                ..Default::default()
            }],
        );
        assert!(h
            .app
            .pending_provider_options(target)
            .iter()
            .any(|entry| entry.id == provider));
        assert_eq!(h.app.source_model_options(target, provider).len(), 1);
        assert!(
            !h.app
                .pending_provider_options(picture)
                .iter()
                .any(|entry| entry.id == provider),
            "vision input grants no image generation"
        );
        assert!(h.app.set_pending_generator_provider(target, provider));
        let mut draft = h.app.agents.flow.drafts[&target].clone();
        draft.prompt = "Describe the supplied image".into();
        assert!(h.app.pending_submission_choice(target, &draft).is_some());
        assert!(h
            .app
            .bind_pending_generator(target, PendingHost::Text, &draft, provider, ""));
        assert!(h.app.validate_subscription_source_choice(target).is_ok());
        h.app.set_agent_catalog_for_test(
            provider,
            vec![atlas_agent::AgentModel {
                id: String::new(),
                name: "Text only".into(),
                is_default: true,
                capabilities: Some(atlas_agent::ModelCapabilities::text_only()),
                ..Default::default()
            }],
        );
        assert!(
            h.app.validate_subscription_source_choice(target).is_err(),
            "Send rechecks changed input capabilities"
        );
    }
}

#[test]
fn subscription_native_model_choices_defaults_and_send_revalidate_model_tools_and_parameters() {
    for provider in ["cursor-cli", "claude-code"] {
        let (mut h, _) = subscription_fixture(provider);
        let model = super::super::super::tests::place(&mut h, "native-model.obj", 2000.0);
        assert!(h.app.source_model_options(model, provider).is_empty());
        h.app.set_agent_catalog_for_test(
            provider,
            vec![atlas_agent::AgentModel {
                id: "native".into(),
                name: "Native model tools".into(),
                is_default: true,
                capabilities: Some(atlas_agent::ModelCapabilities {
                    text: true,
                    model_tools: true,
                    ..Default::default()
                }),
                ..Default::default()
            }],
        );
        assert_eq!(h.app.source_model_options(model, provider).len(), 1);
        assert!(h.app.set_pending_generator_provider(model, provider));
        assert!(h.app.set_pending_generator_model(model, "native"));
        assert_eq!(
            h.app.default_source_model(model),
            Some((provider.into(), "native".into()))
        );
        let mut draft = h.app.agents.flow.drafts[&model].clone();
        draft.prompt = "Make the native model".into();
        assert!(h.app.pending_submission_choice(model, &draft).is_some());
        draft
            .model_parameters
            .insert("invented-speed".into(), "fast".into());
        assert!(h.app.pending_submission_choice(model, &draft).is_none());
        h.app.new_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let next = super::super::super::tests::place(&mut h, "another-model.obj", 2400.0);
        h.app.prepare_pending_chat_choice(next);
        let choice = h.app.pending_generator_choice(next).unwrap();
        assert_eq!((choice.0, choice.1), (provider, "native"));
    }
}
