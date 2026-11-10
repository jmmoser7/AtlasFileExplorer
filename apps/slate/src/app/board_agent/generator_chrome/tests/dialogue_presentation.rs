use super::*;

fn painted_text(shape: &egui::Shape, expected: &str) -> bool {
    match shape {
        egui::Shape::Text(text) => text.galley.text().contains(expected),
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| painted_text(shape, expected)),
        _ => false,
    }
}

#[test]
fn legacy_bound_history_presentation_matches_visible_on_and_preserves_manual_collapse() {
    let (mut h, id) = generator_with_output("legacy-bound-dialogue-default");
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = None;
    });
    assert_eq!(
        h.app.generator_activity_state(id, None).fold,
        activity::CardFold::Partial
    );
    let collapsed = activity::ActivityState::default();
    assert_eq!(
        h.app.generator_activity_state(id, Some(collapsed)).fold,
        activity::CardFold::Collapsed
    );
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .generator
            .preserve_history
            .is_none(),
        "effective presentation never writes a runtime history policy"
    );
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(false);
    });
    assert_eq!(
        h.app.generator_activity_state(id, None).fold,
        activity::CardFold::Collapsed
    );
}

#[test]
fn unsupported_owned_model_send_displays_the_real_preflight_failure_without_dispatch() {
    let (mut h, generator) = generator_with_output("owned-model-preflight-status");
    let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(generator).unwrap())
        .unwrap()
        .clone();
    h.app.delete_board_nodes(&[generator]);
    h.app.set_agent_programs_for_test(&["codex"]);
    h.app.set_agent_catalog_for_test(
        "codex",
        vec![atlas_agent::AgentModel {
            id: "native-model".into(),
            name: "Native model".into(),
            is_default: true,
            ..Default::default()
        }],
    );
    agent.provider = "codex".into();
    agent.model = Some("native-model".into());
    agent.view = atlas_agent::PortalView::Chat;
    agent.generator.preserve_history = Some(false);
    let id = crate::app::board_flow::tests::place(&mut h, "owned-tetrahedron.obj", 0.0);
    std::fs::write(
        h.base.join("owned-tetrahedron.obj"),
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 2 3\nf 1 2 4\nf 1 3 4\nf 2 3 4\n",
    )
    .unwrap();
    h.app.patch_nodes(&[id], |node| {
        node.rect = slate_doc::WorldRect::new(-380.0, -210.0, 760.0, 420.0);
        if let NodeKind::Image(image) = &mut node.kind {
            image.agent = Some(Box::new(agent.clone()));
        }
    });
    h.app.sync_instruction(id, "Create one box at the origin");
    h.app.board_sel = [id].into_iter().collect();
    crate::app::tests::frame_on(&mut h, id, 0.66);
    assert!(h.app.send_agent_prompt(id).is_none());
    let failure = "Set up this model's supported headless binding before sending";
    assert_eq!(h.app.agent_failure_reason(id), Some(failure));
    assert!(
        h.app.agents.dispatched.is_empty(),
        "unsupported binding must never invoke a provider"
    );
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Agent));
    h.frame();
    assert_eq!(h.app.agent_failure_reason(id), Some(failure));
    let output = h.frame_output(|_| {});
    assert_eq!(h.app.agent_failure_reason(id), Some(failure));
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| painted_text(&shape.shape, failure)),
        "the real attached editor renders its preflight error using the shared status row"
    );
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn retained_dashboard_dialogue_is_visible_and_explicit_collapse_remains_respected() {
    let (mut h, id) = generator_with_output("dashboard-retained-dialogue-presentation");
    let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .clone();
    agent.provider = "codex".into();
    agent.view = atlas_agent::PortalView::Chat;
    agent.instruction = "Create a dashboard".into();
    agent.generator.preserve_history = Some(true);
    let mut portal =
        slate_doc::scene::PortalNode::bound_web("Dashboard", "https://example.invalid/dashboard");
    portal.agent = Some(Box::new(agent));
    portal.web.as_mut().unwrap().artifact = Some(Default::default());
    h.app.patch_nodes(&[id], |node| {
        node.kind = NodeKind::Portal(portal.clone());
        node.rect = slate_doc::WorldRect::new(-380.0, -210.0, 760.0, 420.0);
    });
    h.app.set_agent_session_for_test(
        id,
        atlas_agent::AgentSession {
            request: "dashboard-complete".into(),
            provider: "codex".into(),
            status: atlas_agent::AgentStatus::Idle,
            turns: vec![
                atlas_agent::AgentTurn {
                    role: "user".into(),
                    text: "Create a dashboard".into(),
                    at: 0,
                },
                atlas_agent::AgentTurn {
                    role: "assistant".into(),
                    text: "DASHBOARD_RESPONSE_VISIBLE".into(),
                    at: 1,
                },
            ],
            ..Default::default()
        },
    );
    h.app.board_sel = [id].into_iter().collect();
    crate::app::tests::frame_on(&mut h, id, 0.66);
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Agent));
    h.frame();
    let output = h.frame_output(|_| {});
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| painted_text(&shape.shape, "DASHBOARD_RESPONSE_VISIBLE")),
        "real attached dashboard editor must render retained assistant text"
    );
    let mut presentation = h.app.generator_presentation(id);
    let mut state = presentation.activity.unwrap();
    state.fold = activity::CardFold::Collapsed;
    presentation.activity = Some(state);
    h.app.restore_generator_presentation(id, presentation);
    let output = h.frame_output(|_| {});
    assert!(!output
        .shapes
        .iter()
        .any(|shape| painted_text(&shape.shape, "DASHBOARD_RESPONSE_VISIBLE")));
    h.app
        .dispatch_generator_settings(&h.ctx, id, None, Some(true));
    assert_eq!(
        h.app.generator_presentation(id).activity.unwrap().fold,
        activity::CardFold::Partial
    );
    let output = h.frame_output(|_| {});
    assert!(output
        .shapes
        .iter()
        .any(|shape| painted_text(&shape.shape, "DASHBOARD_RESPONSE_VISIBLE")));
}
