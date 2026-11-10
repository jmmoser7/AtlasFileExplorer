use super::*;
use crate::app::board_agent::test_support::{generator_with_note, note};

fn equivalent(app: &SlateApp, target: NodeId) {
    app.legacy_input_projection_for_test(true);
    let eager = app.agent_input_snapshot(target);
    app.legacy_input_projection_for_test(false);
    let lazy = app.agent_input_snapshot(target);
    assert_eq!(
        serde_json::to_value(eager).unwrap(),
        serde_json::to_value(lazy).unwrap()
    );
}

#[test]
fn ordinary_model_reference_survives_chat_metadata_and_reaches_real_capture_admission() {
    for metadata in ["none", "unbound", "bound"] {
        let (mut h, target, _) = generator_with_note(
            "model-reference-before-chat-album",
            "Describe the model view",
        );
        let model = crate::app::board_flow::tests::place(&mut h, "tetrahedron.obj", -800.0);
        let path = h.base.join("tetrahedron.obj");
        std::fs::write(
            &path,
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 2 3\nf 1 2 4\nf 1 3 4\nf 2 3 4\n",
        )
        .unwrap();
        let t = slate_doc::agent_inputs::input_ports(&h.app.doc().scene, target)
            .iter()
            .find(|port| port.slot == atlas_agent::InputSlot::Media)
            .unwrap()
            .t;
        let wire = h.app.build_connector(
            slate_doc::scene::ConnectorEnd::Anchored {
                node: model,
                side: slate_doc::scene::Side::Right,
                t: 0.5,
            },
            slate_doc::scene::ConnectorEnd::Anchored {
                node: target,
                side: slate_doc::scene::Side::Left,
                t,
            },
        );
        h.app.add_nodes(vec![wire]);
        h.app.patch_nodes(&[model], |node| {
            let NodeKind::Image(image) = &mut node.kind else {
                panic!()
            };
            image.agent = if metadata == "none" {
                None
            } else {
                let mut agent =
                    slate_doc::scene::AgentPortalRef::unbound(atlas_agent::PortalView::Chat);
                if metadata == "bound" {
                    agent.provider = "codex".into();
                    agent.session = "model-chat-with-no-image-results".into();
                }
                Some(Box::new(agent))
            };
        });
        let source_before = h.app.doc().scene.node(model).unwrap().clone();
        let saved = h.base.join("Model input.slate");
        h.app.save_doc_to(h.app.tab().id, saved.clone());
        h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&saved).unwrap();
        h.app.invalidate_generator_inputs();
        equivalent(&h.app, target);
        let mut request = h.app.generation_request(target, false).unwrap();
        let reference = request
            .inputs
            .on(atlas_agent::InputSlot::Media)
            .find(|item| item.node == model.0)
            .unwrap();
        assert_eq!(
            reference.images,
            vec![path.to_string_lossy().into_owned()],
            "metadata={metadata}"
        );
        let mut preparation = h
            .app
            .begin_agent_input_preparation(request.inputs.clone())
            .unwrap();
        assert!(h
            .app
            .poll_agent_input_preparation(&mut preparation)
            .unwrap());
        // The ordinary unit harness has no GPU. Reaching this real capture
        // error proves admission got past album readiness without pretending
        // the OBJ is a decoded image or fabricating a rendered view/depth.
        assert_eq!(
            h.app
                .capture_prepared_generator_inputs(target, &mut preparation, &mut request, false)
                .unwrap_err(),
            "3D viewports need GPU rendering (unavailable here)."
        );
        assert!(h.app.agents.dispatched.is_empty());
        assert_eq!(
            h.app.doc().scene.node(model),
            Some(&source_before),
            "source geometry/camera stays authored"
        );
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 2 3\nf 1 2 4\nf 1 3 4\nf 2 3 4\n"
        );
    }
}

#[test]
fn unwired_nodes_are_never_projected_and_snapshot_matches_eager_control() {
    let (mut h, target, _) = generator_with_note("lazy-projection-scope", "Wired instruction");
    for i in 0..80 {
        note(
            &mut h,
            Pos2::new(5000.0, i as f32 * 200.0),
            "Unrelated authored content",
        );
    }
    equivalent(&h.app, target);
    PROJECTED_NODES.with(|count| count.set(0));
    h.app.agent_input_snapshot(target).unwrap();
    assert_eq!(
        PROJECTED_NODES.with(std::cell::Cell::get),
        1,
        "only the actual wired source is projected"
    );
    h.app.legacy_input_projection_for_test(true);
    PROJECTED_NODES.with(|count| count.set(0));
    h.app.agent_input_snapshot(target).unwrap();
    let eager_nodes = PROJECTED_NODES.with(std::cell::Cell::get);
    h.app.legacy_input_projection_for_test(false);
    assert!(eager_nodes > 80, "control exercises whole-board projection");
}

#[test]
fn live_edits_and_journal_changes_are_read_without_cached_projection() {
    let (mut h, target, source) = generator_with_note("lazy-projection-edits", "Original source");
    let captured = h.app.agent_input_snapshot(target).unwrap();
    h.app.text_edit = Some((source, "Live Unicode source 🦀".into()));
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Live Unicode source 🦀"
    );
    h.app.text_edit = None;
    h.app.patch_nodes(&[source], |node| {
        if let NodeKind::Text(text) = &mut node.kind {
            text.text = "Committed change".into();
        }
    });
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Committed change"
    );
    assert_eq!(
        captured.wired[0].text, "Original source",
        "an earlier run's immutable input stays intact"
    );
}

#[test]
fn new_session_output_is_visible_without_a_scene_edit() {
    let (mut h, target, source) = generator_with_note("lazy-projection-session", "");
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
        .unwrap()
        .clone();
    h.app.patch_nodes(&[source], |node| {
        if let NodeKind::Text(text) = &mut node.kind {
            text.agent = Some(Box::new(agent.clone()));
        }
    });
    let mut session = AgentSession {
        turns: vec![AgentTurn {
            role: "assistant".into(),
            text: "First response".into(),
            at: 1,
        }],
        ..Default::default()
    };
    h.app.set_agent_session_for_test(source, session.clone());
    let scene = h.app.scene_gen;
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "First response"
    );
    session.turns[0].text = "Updated response".into();
    h.app.set_agent_session_for_test(source, session);
    assert_eq!(h.app.scene_gen, scene);
    equivalent(&h.app, target);
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Updated response"
    );
}
