use super::*;

fn catalog(h: &mut Harness, provider: &str) {
    h.app.set_agent_catalog_for_test(
        provider,
        vec![serde_json::from_value(serde_json::json!({
        "id":"preferred", "name":"Preferred", "is_default":true, "parameters":[
            {"id":"effort","name":"Thought level","values":[{"value":"medium","name":"Medium"}]},
            {"id":"serviceTier","name":"Speed","values":[{"value":"priority","name":"Fast"}]}
        ]
    })).unwrap()],
    );
}

fn web(h: &mut Harness) -> NodeId {
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 640.0, 360.0),
        NodeKind::Portal(PortalNode::bound_web(
            "Dashboard",
            "https://example.invalid/",
        )),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    id
}

fn source(h: &mut Harness, kind: PendingHost) -> NodeId {
    match kind {
        PendingHost::Chat => {
            h.app.place_agent_portal_at(Pos2::ZERO);
            h.app.doc().scene.nodes.last().unwrap().id
        }
        PendingHost::Artifact => web(h),
        PendingHost::Model => super::super::super::tests::place(h, "shape.obj", 800.0),
        PendingHost::Text => super::super::super::tests::place(h, "source.txt", 1600.0),
        PendingHost::Image => super::super::super::tests::place(h, "source.png", 2400.0),
    }
}

fn modalities() -> [(PendingHost, &'static str); 5] {
    [
        (PendingHost::Chat, "codex"),
        (PendingHost::Artifact, "codex"),
        (PendingHost::Model, "codex"),
        (PendingHost::Text, "openai-text"),
        (PendingHost::Image, "openai-image"),
    ]
}

#[test]
fn ordinary_sources_restore_shared_preferences_in_a_real_new_tab() {
    for (kind, provider) in modalities() {
        let mut h = prepare(&format!("pending-new-tab-{kind:?}"));
        catalog(&mut h, provider);
        let first = source(&mut h, kind);
        assert!(h.app.set_pending_generator_provider(first, provider));
        assert!(h.app.set_pending_generator_model(first, "preferred"));
        let parameters = std::collections::BTreeMap::from([("effort".into(), "medium".into())]);
        assert!(h
            .app
            .set_pending_generator_parameters(first, parameters.clone()));
        let first_tab = h.app.active_tab;
        h.app.new_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let next = source(&mut h, kind);
        assert!(
            h.app.pending_generator_choice(next).is_none(),
            "the new tab has no old draft"
        );
        let original = h.app.doc().scene.node(next).unwrap().clone();
        h.app.prepare_pending_chat_choice(next);
        let choice = h.app.pending_generator_choice(next).unwrap();
        assert_eq!((choice.0, choice.1), (provider, "preferred"));
        assert_eq!(choice.2, &parameters);
        assert_eq!(h.app.doc().scene.node(next).unwrap(), &original);
        h.app.switch_tab(first_tab);
        let choice = h.app.pending_generator_choice(first).unwrap();
        assert_eq!((choice.0, choice.1), (provider, "preferred"));
        assert_eq!(choice.2, &parameters);
    }
}

#[test]
fn late_catalog_revalidates_removed_models_without_replacing_valid_element_choices() {
    for (kind, provider) in modalities() {
        let mut h = prepare(&format!("pending-late-catalog-{kind:?}"));
        h.app.set_agent_catalog_for_test(provider, Vec::new());
        let id = source(&mut h, kind);
        h.app
            .remember_source_model(id, provider, "removed-model", &Default::default());
        h.app.prepare_pending_chat_choice(id);
        assert_eq!(
            h.app.pending_generator_choice(id).unwrap().1,
            "removed-model"
        );
        catalog(&mut h, provider);
        h.app.prepare_pending_chat_choice(id);
        let fallback = if matches!(kind, PendingHost::Chat | PendingHost::Model) {
            "preferred"
        } else {
            ""
        };
        let choice = h.app.pending_generator_choice(id).unwrap();
        assert_eq!((choice.0, choice.1), (provider, fallback));

        assert!(h.app.set_pending_generator_model(id, "preferred"));
        let parameters = std::collections::BTreeMap::from([("effort".into(), "medium".into())]);
        assert!(h
            .app
            .set_pending_generator_parameters(id, parameters.clone()));
        // A later global selection cannot rewrite a valid existing element.
        h.app
            .remember_source_model(id, provider, "different-global-model", &Default::default());
        h.app.prepare_pending_chat_choice(id);
        let choice = h.app.pending_generator_choice(id).unwrap();
        assert_eq!((choice.0, choice.1), (provider, "preferred"));
        assert_eq!(choice.2, &parameters);
    }
}

#[test]
fn late_catalog_filters_parameters_while_preserving_the_valid_model() {
    let mut h = prepare("pending-late-parameter-catalog");
    h.app.set_agent_catalog_for_test("openai-image", Vec::new());
    let id = source(&mut h, PendingHost::Image);
    let parameters = std::collections::BTreeMap::from([
        ("effort".into(), "medium".into()),
        ("serviceTier".into(), "removed-speed".into()),
    ]);
    h.app
        .remember_source_model(id, "openai-image", "preferred", &parameters);
    h.app.prepare_pending_chat_choice(id);
    assert_eq!(h.app.pending_generator_choice(id).unwrap().2, &parameters);
    catalog(&mut h, "openai-image");
    h.app.prepare_pending_chat_choice(id);
    let choice = h.app.pending_generator_choice(id).unwrap();
    assert_eq!((choice.0, choice.1), ("openai-image", "preferred"));
    assert_eq!(
        choice.2,
        &std::collections::BTreeMap::from([("effort".into(), "medium".into())])
    );
}

#[test]
fn ordinary_new_sources_restore_last_compatible_choice_after_settings_round_trip() {
    let mut h = prepare("pending-model-preference-restart");
    for provider in ["codex", "openai-text", "openai-image"] {
        catalog(&mut h, provider);
    }
    let chat = source(&mut h, PendingHost::Chat);
    assert!(h.app.set_pending_generator_provider(chat, "codex"));
    assert!(h.app.set_pending_generator_model(chat, "preferred"));
    let first = web(&mut h);
    assert!(h.app.set_pending_generator_provider(first, "codex"));
    assert!(h.app.set_pending_generator_model(first, "preferred"));
    for (parameter, value) in [("effort", "medium"), ("serviceTier", "priority")] {
        assert!(h.app.generator_set_parameter(Some(
            &serde_json::json!({
                "node":first,"parameter":parameter,"value":value
            })
            .to_string()
        )));
    }
    let text = super::super::super::tests::place(&mut h, "source.txt", 800.0);
    assert!(h.app.set_pending_generator_provider(text, "openai-text"));
    assert!(h.app.set_pending_generator_model(text, "preferred"));
    let image = super::super::super::tests::place(&mut h, "source.png", 1600.0);
    assert!(h.app.set_pending_generator_provider(image, "openai-image"));
    assert!(h.app.set_pending_generator_model(image, "preferred"));
    let persisted = serde_json::to_string(&h.app.settings).unwrap();

    let mut restarted = prepare("pending-model-preference-restored");
    restarted.app.settings = serde_json::from_str(&persisted).unwrap();
    for provider in ["codex", "openai-text", "openai-image"] {
        catalog(&mut restarted, provider);
    }
    let dashboard = web(&mut restarted);
    let model = super::super::super::tests::place(&mut restarted, "shape.obj", 800.0);
    let text = super::super::super::tests::place(&mut restarted, "next.txt", 1600.0);
    let image = super::super::super::tests::place(&mut restarted, "next.png", 2400.0);
    let chat = source(&mut restarted, PendingHost::Chat);
    for (id, provider) in [
        (dashboard, "codex"),
        (model, "codex"),
        (text, "openai-text"),
        (image, "openai-image"),
        (chat, "codex"),
    ] {
        let original = restarted.app.doc().scene.node(id).unwrap().clone();
        restarted.app.prepare_pending_chat_choice(id);
        let choice = restarted.app.pending_generator_choice(id).unwrap();
        assert_eq!((choice.0, choice.1), (provider, "preferred"));
        if provider == "codex" {
            assert_eq!(choice.2.get("effort").map(String::as_str), Some("medium"));
            assert_eq!(
                choice.2.get("serviceTier").map(String::as_str),
                Some("priority")
            );
        }
        assert_eq!(
            restarted.app.doc().scene.node(id).unwrap(),
            &original,
            "preparing defaults does not attach an agent or change the source"
        );
    }
    // A later global selection must not rewrite an already configured source.
    restarted
        .app
        .remember_flow_model(SpawnKind::Artifact, "cursor", "different");
    restarted.app.prepare_pending_chat_choice(dashboard);
    assert_eq!(
        restarted.app.pending_generator_choice(dashboard).unwrap().0,
        "codex"
    );
}

#[test]
fn stale_preferences_are_filtered_by_source_capabilities_and_advertised_parameters() {
    let mut h = prepare("pending-model-preference-validity");
    catalog(&mut h, "codex");
    let id = web(&mut h);
    let values = std::collections::BTreeMap::from([
        ("effort".into(), "medium".into()),
        ("serviceTier".into(), "removed-speed".into()),
        ("removed-parameter".into(), "old".into()),
    ]);
    h.app
        .remember_source_model(id, "codex", "preferred", &values);
    h.app.prepare_pending_chat_choice(id);
    assert_eq!(
        h.app.pending_generator_choice(id).unwrap().2,
        &std::collections::BTreeMap::from([("effort".into(), "medium".into())])
    );
    h.app.settings.agent_artifact_model = Some(("openai-image".into(), "image-only".into()));
    let model = super::super::super::tests::place(&mut h, "model.obj", 800.0);
    h.app.prepare_pending_chat_choice(model);
    let choice = h.app.pending_generator_choice(model).unwrap();
    assert_eq!(
        choice.0, "codex",
        "3D keeps its native agentic harness filter"
    );
    assert_eq!(choice.1, "preferred");
    h.app.settings.agent_artifact_model = Some(("codex".into(), "removed-model".into()));
    let next = web(&mut h);
    h.app.prepare_pending_chat_choice(next);
    assert_eq!(
        h.app.pending_generator_choice(next).unwrap().1,
        "",
        "a removed model falls back to the provider's Automatic choice"
    );
}
