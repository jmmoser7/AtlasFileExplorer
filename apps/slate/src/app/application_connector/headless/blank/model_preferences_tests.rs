use super::*;

fn remembered_native_choice(h: &mut crate::app::tests::Harness, node: NodeId) {
    h.app.set_agent_catalog_for_test("codex", vec![
        atlas_agent::AgentModel {
            id: "native-codex-model".into(), name: "Native default".into(),
            is_default: true, ..Default::default()
        },
        serde_json::from_value(serde_json::json!({
            "id":"remembered-native", "name":"Remembered native", "parameters":[
                {"id":"effort","name":"Thought level","values":[{"value":"medium","name":"Medium"}]},
                {"id":"serviceTier","name":"Speed","values":[{"value":"priority","name":"Fast"}]}
            ]
        })).unwrap(),
    ]);
    h.app.remember_source_model(
        node,
        "codex",
        "remembered-native",
        &std::collections::BTreeMap::from([
            ("effort".into(), "medium".into()),
            ("serviceTier".into(), "priority".into()),
            ("removed-parameter".into(), "stale".into()),
        ]),
    );
    h.app.settings.agent_image_model = Some(("comfy".into(), "image-checkpoint".into()));
    h.app.settings.agent_text_model = Some(("openai-text".into(), "text-only".into()));
}

fn assert_remembered(h: &crate::app::tests::Harness, id: NodeId) {
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(agent.provider, "codex");
    assert_eq!(agent.model.as_deref(), Some("remembered-native"));
    assert_eq!(
        agent.model_parameters,
        std::collections::BTreeMap::from([
            ("effort".into(), "medium".into()),
            ("serviceTier".into(), "priority".into()),
        ])
    );
    assert_eq!(agent.view, atlas_agent::PortalView::Chat);
    assert!(!agent.chat.train);
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn downstream_native_model_restores_and_freezes_agentic_choice_for_model_and_image_sources() {
    for from_model in [false, true] {
        let (mut h, image) = native_chooser_fixture(&format!("native-remembered-{from_model}"));
        let model = crate::app::board_flow::tests::place(&mut h, "native.obj", 0.0);
        remembered_native_choice(&mut h, model);
        let source = if from_model { model } else { image };
        let source_before = h.app.doc().scene.node(source).unwrap().clone();
        let (_reply, rx) = crossbeam_channel::bounded(1);
        EMPTY_PREPARATION_FOR_TEST.with(|fixture| {
            *fixture.borrow_mut() =
                Some((rx, Cancellation::default(), crossbeam_channel::bounded(1).0))
        });
        assert!(h.app.spawn_flow_node(Some(
            &serde_json::json!({
                "source":source.0,"kind":"model","chooser":true
            })
            .to_string()
        )));
        let job = h.app.connectors.headless.blanks.pop().unwrap();
        // A machine preference change while the worker runs cannot rewrite
        // the model and parameters already chosen for this particular child.
        h.app.remember_flow_model(
            crate::app::board_flow::SpawnKind::Artifact,
            "codex",
            "native-codex-model",
        );
        let path = h.base.join("remembered-empty.3dm");
        std::fs::write(&path, b"prepared native model").unwrap();
        let file = super::super::super::super::super::items::ReadyFile {
            path,
            size: 21,
            mtime: 0,
        };
        let nodes = h.app.prepare_blank_model_nodes(&job, &file).unwrap();
        let child = h
            .app
            .link_prepared_ready_media(file, nodes, slate_doc::scene::CmdAuthor::Human)
            .unwrap();
        assert_remembered(&h, child);
        assert_eq!(h.app.doc().scene.node(source).unwrap(), &source_before);
        h.app.board_undo();
        assert!(h.app.doc().scene.node(child).is_none());
        h.app.board_redo();
        assert_remembered(&h, child);
        let saved = serde_json::to_vec(h.app.doc()).unwrap();
        h.app.tab_mut().doc = serde_json::from_slice(&saved).unwrap();
        assert_remembered(&h, child);
    }
}

#[test]
fn root_native_binding_restores_preferences_without_rewriting_existing_configuration() {
    let (mut h, _) = native_chooser_fixture("native-root-remembered");
    let node = crate::app::board_flow::tests::place(&mut h, "native.obj", 0.0);
    remembered_native_choice(&mut h, node);
    let rect = h.app.doc().scene.node(node).unwrap().rect;
    h.app.bind_model_agent(node).unwrap();
    assert_remembered(&h, node);
    assert_eq!(h.app.doc().scene.node(node).unwrap().rect, rect);
    h.app.remember_flow_model(
        crate::app::board_flow::SpawnKind::Artifact,
        "codex",
        "native-codex-model",
    );
    h.app.bind_model_agent(node).unwrap();
    assert_remembered(&h, node);
}

#[test]
fn unavailable_or_incompatible_native_preferences_keep_the_advertised_native_fallback() {
    for (provider, model) in [("codex", "removed-model"), ("openai-image", "image-only")] {
        let (mut h, _) = native_chooser_fixture("native-stale-remembered");
        h.app.settings.agent_artifact_model = Some((provider.into(), model.into()));
        let node = crate::app::board_flow::tests::place(&mut h, "native.obj", 0.0);
        h.app.bind_model_agent(node).unwrap();
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(node).unwrap()).unwrap();
        assert_eq!(agent.provider, "codex");
        assert_eq!(agent.model.as_deref(), Some("native-codex-model"));
        assert!(agent.model_parameters.is_empty());
        assert!(h.app.agents.dispatched.is_empty());
    }
}

fn click(h: &mut crate::app::tests::Harness, point: eframe::egui::Pos2) {
    h.frame_with(|input| input.events.push(eframe::egui::Event::PointerMoved(point)));
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(eframe::egui::Event::PointerButton {
                pos: point,
                button: eframe::egui::PointerButton::Primary,
                pressed,
                modifiers: eframe::egui::Modifiers::NONE,
            });
        });
    }
    h.frame();
    h.frame();
}

fn visible_label(output: &eframe::egui::FullOutput, label: &str) -> eframe::egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            eframe::egui::Shape::Text(text)
                if text.galley.text() == label
                    && shape.clip_rect.contains_rect(text.visual_bounding_rect()) =>
            {
                Some(text.visual_bounding_rect().center())
            }
            _ => None,
        })
        .expect("the actual model picker row is fully visible")
}

#[test]
fn bound_native_child_and_text_model_picker_applies_choices_and_remembers_parameters() {
    for host in ["model", "text", "chat-text"] {
        let (mut h, source) = native_chooser_fixture(&format!("bound-picker-command-{host}"));
        let preference_source = crate::app::board_flow::tests::place(&mut h, "native.obj", 0.0);
        remembered_native_choice(&mut h, preference_source);
        h.app.remember_flow_model(
            crate::app::board_flow::SpawnKind::Artifact,
            "codex",
            "native-codex-model",
        );
        let provider = if host == "text" {
            "codex-text"
        } else {
            "codex"
        };
        let id = if host == "model" {
            let (_reply, rx) = crossbeam_channel::bounded(1);
            EMPTY_PREPARATION_FOR_TEST.with(|fixture| {
                *fixture.borrow_mut() =
                    Some((rx, Cancellation::default(), crossbeam_channel::bounded(1).0))
            });
            assert!(h.app.spawn_flow_node(Some(
                &serde_json::json!({
                    "source":source.0,"kind":"model","chooser":true
                })
                .to_string()
            )));
            let job = h.app.connectors.headless.blanks.pop().unwrap();
            let path = h.base.join("picker-empty.3dm");
            std::fs::write(
                &path,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../crates/rhino-mesh/tests/fixtures/mesh_sphere.3dm"
                )),
            )
            .unwrap();
            let file = super::super::super::super::super::items::ReadyFile {
                size: std::fs::metadata(&path).unwrap().len(),
                path,
                mtime: 0,
            };
            let nodes = h.app.prepare_blank_model_nodes(&job, &file).unwrap();
            h.app
                .link_prepared_ready_media(file, nodes, slate_doc::scene::CmdAuthor::Human)
                .unwrap()
        } else {
            let mut agent = slate_doc::scene::AgentPortalRef::unbound(if host == "text" {
                atlas_agent::PortalView::Text
            } else {
                atlas_agent::PortalView::Chat
            });
            agent.provider = provider.into();
            agent.model = Some("native-codex-model".into());
            agent.chat.train = false;
            let text = slate_doc::scene::TextNode::agent_note(agent);
            let node = h.app.doc_mut().scene.build_node(
                WorldRect::new(-380.0, -210.0, 760.0, 420.0),
                NodeKind::Text(text),
            );
            h.app.add_nodes(vec![node])[0]
        };
        h.app.board_sel = [id].into_iter().collect();
        crate::app::tests::frame_on(&mut h, id, 0.66);
        assert!(h
            .app
            .open_object_panel(crate::app::board_properties::Panel::Agent));
        h.frame();
        h.frame();
        let trigger = h
            .ctx
            .data(|data| {
                data.get_temp::<eframe::egui::Rect>(eframe::egui::Id::new((
                    "painted-generator-model-hit",
                    id.0,
                )))
            })
            .expect("real model trigger");
        click(&mut h, trigger.center());
        let output = h.frame_output(|_| {});
        let model = visible_label(&output, "Model");
        click(&mut h, model);
        let output = h.frame_output(|_| {});
        let choice = visible_label(&output, "Remembered native");
        click(&mut h, choice);
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert_eq!(
            agent.model.as_deref(),
            Some("remembered-native"),
            "actual picker applies on {host}"
        );
        assert_eq!(agent.provider, provider);
        for (parameter, value) in [("effort", "medium"), ("serviceTier", "priority")] {
            assert!(h.app.dispatch(
                &h.ctx,
                atlas_commands::CommandId("portal.agent.model_parameter"),
                Some(
                    serde_json::json!({"node":id,"parameter":parameter,"value":value}).to_string()
                )
            ));
        }
        h.frame();
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert_eq!(agent.model.as_deref(), Some("remembered-native"));
        let slot = if host == "model" {
            &h.app.settings.agent_artifact_model
        } else {
            &h.app.settings.agent_text_model
        };
        assert_eq!(
            slot.as_ref(),
            Some(&(provider.into(), "remembered-native".into()))
        );
        assert_eq!(
            h.app.settings.agent_model_parameters[provider]["remembered-native"],
            agent.model_parameters
        );
        assert_eq!(
            agent.model_parameters.get("effort").map(String::as_str),
            Some("medium")
        );
        assert_eq!(
            agent
                .model_parameters
                .get("serviceTier")
                .map(String::as_str),
            Some("priority")
        );
        assert!(h.app.agents.dispatched.is_empty());
    }
}
