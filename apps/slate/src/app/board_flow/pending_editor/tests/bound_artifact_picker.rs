//! Exercise the shared menu on the already-bound Dashboard created by a wire.
use super::*;

fn models(provider: &str, artifact: bool) -> Vec<atlas_agent::AgentModel> {
    ["default", "second"]
        .into_iter()
        .map(|suffix| atlas_agent::AgentModel {
            id: format!("{provider}-{suffix}"),
            name: format!("{provider} {suffix}"),
            is_default: suffix == "default",
            capabilities: Some(atlas_agent::ModelCapabilities {
                text: true,
                artifact,
                vision: true,
                ..Default::default()
            }),
            parameters: vec![atlas_agent::ModelParameterDefinition {
                id: "effort".into(),
                name: "Thought level".into(),
                values: vec![
                    atlas_agent::ModelParameterValue {
                        value: "medium".into(),
                        name: "Medium".into(),
                    },
                    atlas_agent::ModelParameterValue {
                        value: "high".into(),
                        name: "High".into(),
                    },
                ],
                default: Some("medium".into()),
            }],
            ..Default::default()
        })
        .collect()
}

#[test]
fn wired_bound_dashboard_shared_provider_and_model_clicks_bind_and_persist_subscription_choice() {
    for provider in ["cursor-cli", "claude-code"] {
        let mut h = prepare(&format!("bound-artifact-picker-{provider}"));
        h.app
            .set_agent_programs_for_test(&["codex", "cursor-cli", "claude-code"]);
        h.app.settings.agent_artifact_model = Some(("codex".into(), "discovered-native-id".into()));
        for candidate in ["cursor-cli", "claude-code"] {
            h.app
                .set_agent_catalog_for_test(candidate, models(candidate, false));
        }
        let source = crate::app::board_flow::tests::place(&mut h, "linked-source.txt", -600.0);
        let NodeKind::Image(image) = &h.app.doc().scene.node(source).unwrap().kind else {
            panic!()
        };
        h.app.cache_snippet(
            image.item,
            Some("Owned linked words for Dashboard review.".into()),
        );
        let original = h.app.doc().scene.node(source).unwrap().clone();
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.spawn_flow_node(Some(
            &serde_json::json!({
                "source":source.0,"kind":"artifact","chooser":true,"at":[300.0,100.0]
            })
            .to_string()
        )));
        let target = h.app.doc().scene.nodes[before].id;
        assert!(
            h.app.pending_generator_host(target).is_none(),
            "wire-created Dashboard is already bound"
        );
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
                .unwrap()
                .provider,
            "codex"
        );
        let wire = h.app.doc().scene.nodes[before + 1].clone();
        let session = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
            .unwrap()
            .session
            .clone();
        h.app
            .dispatch_generator_provider(target, Some(provider.into()), None);
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
                .unwrap()
                .provider,
            "codex",
            "registered Text-only models cannot bind an Artifact target"
        );
        h.app
            .dispatch_generator_provider(target, Some("openai-text".into()), None);
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
                .unwrap()
                .provider,
            "codex"
        );
        for candidate in ["cursor-cli", "claude-code"] {
            h.app
                .set_agent_catalog_for_test(candidate, models(candidate, true));
        }
        open_actual_agent(&mut h, target);
        let output = h.frame_output(|_| {});
        click(
            &mut h,
            text_rect(&output, "Codex · Discovered native model")
                .expect("actual Dashboard model trigger")
                .center(),
        );
        let output = h.frame_output(|_| {});
        click(&mut h, text_rect(&output, "Provider").unwrap().center());
        let output = h.frame_output(|_| {});
        let provider_label = atlas_ai::agent::provider_by_id(provider).display_name;
        click(
            &mut h,
            text_rect(&output, &provider_label)
                .expect("actual eligible provider row")
                .center(),
        );
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap()).unwrap();
        assert_eq!(agent.provider, provider);
        assert_eq!(
            agent.model.as_deref(),
            Some(format!("{provider}-default").as_str())
        );
        assert_eq!(agent.session, session);
        let output = h.frame_output(|_| {});
        click(&mut h, text_rect(&output, "Model").unwrap().center());
        let output = h.frame_output(|_| {});
        let other = if provider == "cursor-cli" {
            "claude-code"
        } else {
            "cursor-cli"
        };
        for foreign in [
            format!("{other} second"),
            format!(
                "{} · {other} second",
                atlas_ai::agent::provider_by_id(other).display_name
            ),
            "Discovered native model".into(),
            "Codex · Discovered native model".into(),
        ] {
            assert!(
                text_rect(&output, &foreign).is_none(),
                "Model submenu must not include another provider: {foreign}"
            );
        }
        let selected_label = format!("{provider} second");
        click(
            &mut h,
            text_rect(&output, &selected_label)
                .expect("real catalog choice is reachable")
                .center(),
        );
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap()).unwrap();
        assert_eq!(agent.provider, provider);
        assert_eq!(
            agent.model.as_deref(),
            Some(format!("{provider}-second").as_str())
        );
        assert_eq!(agent.session, session);
        let output = h.frame_output(|_| {});
        click(&mut h, text_rect(&output, "Effort").unwrap().center());
        let output = h.frame_output(|_| {});
        click(
            &mut h,
            text_rect(&output, "High")
                .expect("selected model effort choice")
                .center(),
        );
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap()).unwrap();
        assert_eq!(
            agent.provider, provider,
            "model and parameter selection never switches provider"
        );
        assert_eq!(
            agent.model.as_deref(),
            Some(format!("{provider}-second").as_str())
        );
        assert_eq!(
            agent.model_parameters.get("effort").map(String::as_str),
            Some("high")
        );
        assert!(h.app.validate_subscription_source_choice(target).is_ok());
        assert_eq!(h.app.doc().scene.node(source), Some(&original));
        assert_eq!(h.app.doc().scene.node(wire.id), Some(&wire));
        assert!(
            h.app.agents.dispatched.is_empty(),
            "selection never submits a provider request"
        );
        let before = h.app.doc().scene.nodes.len();
        assert!(h.app.spawn_flow_node(Some(
            &serde_json::json!({
                "source":source.0,"kind":"artifact","chooser":true,"at":[1600.0,100.0]
            })
            .to_string()
        )));
        let next = h.app.doc().scene.nodes[before].id;
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(next).unwrap()).unwrap();
        assert_eq!(
            agent.provider, provider,
            "next wire-created Dashboard retains remembered provider"
        );
        assert_eq!(
            agent.model.as_deref(),
            Some(format!("{provider}-second").as_str())
        );
        assert_eq!(
            agent.model_parameters.get("effort").map(String::as_str),
            Some("high")
        );
    }
}
