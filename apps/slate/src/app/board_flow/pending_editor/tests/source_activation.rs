#[test]
fn ordinary_no_agent_sources_open_the_real_agent_tool_without_binding_or_image_leakage() {
    for (file, kind) in [
        ("ordinary.obj", PendingHost::Model),
        ("ordinary.3dm", PendingHost::Model),
        ("ordinary.txt", PendingHost::Text),
        ("ordinary.png", PendingHost::Image),
    ] {
        let mut h = prepare(&format!("ordinary-first-use-{file}"));
        let id = super::super::tests::place(&mut h, file, 0.0);
        if file.ends_with(".obj") {
            std::fs::write(h.base.join(file), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        }
        if file.ends_with(".3dm") {
            std::fs::write(
                h.base.join(file),
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../crates/rhino-mesh/tests/fixtures/mesh_sphere.3dm"
                )),
            )
            .unwrap();
        }
        h.app.patch_nodes(&[id], |node| {
            node.rect = slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0)
        });
        if kind == PendingHost::Text {
            let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
                panic!()
            };
            h.app
                .cache_snippet(image.item, Some("A café & tea. Original source.".into()));
        }
        h.frame();
        assert!(slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).is_none());
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let sibling = super::super::tests::place(&mut h, "unactivated-sibling.png", 2200.0);
        let output = h
            .app
            .wire_input_host(&before)
            .anchor(Side::Right, agent_inputs::OUTPUT_T);
        assert_eq!(
            output,
            [
                before.rect.x + before.rect.w,
                before.rect.y + before.rect.h * 0.5
            ]
        );
        assert!(!h.app.source_agent_inputs_visible(id));
        assert!(h
            .app
            .presented_wire_ports(&before)
            .iter()
            .all(|port| port.side == Side::Right));
        let nodes = h.app.doc().scene.nodes.len();
        let undo = h.app.tab().journal.undo_depth();
        open_actual_agent(&mut h, id);
        assert!(h.app.source_agent_inputs_visible(id));
        assert!(!h.app.source_agent_inputs_visible(sibling));
        assert_eq!(
            h.app
                .wire_input_host(&before)
                .anchor(Side::Right, agent_inputs::OUTPUT_T),
            output
        );

        assert_eq!(h.app.pending_generator_host(id), Some(kind));
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
        assert_eq!(h.app.doc().scene.nodes.len(), nodes);
        assert_eq!(h.app.tab().journal.undo_depth(), undo);
        let output = h.frame_output(|_| {});
        let trigger = match kind {
            PendingHost::Model => "Codex · Discovered native model",
            PendingHost::Text => "Ollama · Automatic",
            PendingHost::Image => "ComfyUI · Automatic",
            _ => unreachable!(),
        };
        let trigger_rect = if kind == PendingHost::Model {
            let rect = h
                .ctx
                .data(|data| data.get_temp::<Rect>(Id::new(("painted-generator-model-hit", id.0))))
                .expect("stable production model trigger response");
            assert!(rect.width() <= 120.0 * h.app.board_xf().z + 0.01);
            assert_eq!(
                h.app
                    .pending_generator_choice(id)
                    .map(|(provider, model, _)| (provider, model)),
                Some(("codex", "discovered-native-id"))
            );
            assert!(h
                .app
                .source_model_options(id, "codex")
                .iter()
                .any(|(_, _, name)| name == "Discovered native model"));
            rect
        } else {
            text_rect(&output, trigger).expect("actual compact model trigger")
        };
        click(&mut h, trigger_rect.center());
        let root = h.frame_output(|_| {});
        for label in ["Provider", "Model", "Effort", "Speed"] {
            assert!(
                text_rect(&root, label).is_some(),
                "missing {label} for {file}"
            );
        }
        assert!(text_rect(&root, "Full access · all agents").is_none());
        assert!(text_rect(&root, "Agent · chat train").is_none());
        click(&mut h, text_rect(&root, "Provider").unwrap().center());
        let providers = h.frame_output(|_| {});
        if kind == PendingHost::Model {
            for forbidden in ["OpenAI", "GPT Image", "ComfyUI", "Cursor"] {
                assert!(
                    text_rect(&providers, forbidden).is_none(),
                    "{forbidden} is not an admitted native harness"
                );
            }
            assert!(text_rect(&providers, "Codex").is_some());
        }
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
    }
}

#[test]
fn authored_text_stays_a_plain_sheet_until_its_own_robot_is_opened() {
    let mut h = prepare("authored-text-simple-before-robot");
    let mut text = slate_doc::scene::TextNode::agent_note(
        slate_doc::scene::AgentPortalRef::unbound(PortalView::Text),
    );
    text.agent = None;
    text.text = "build a better version of this".into();
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20., 20., 640., 300.),
        NodeKind::Text(text),
    );
    let id = h.app.add_nodes(vec![node])[0];
    crate::app::tests::frame_on(&mut h, id, 1.0);
    h.app.board_sel = [id].into_iter().collect();
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().clone();
    assert!(!h.app.source_agent_inputs_visible(id));
    let output = h.frame_output(|_| {});
    for forbidden in [
        "Run",
        "Wire a note to Prompt, or type here",
        "Style image",
        "Prompt",
    ] {
        assert!(
            text_rect(&output, forbidden).is_none(),
            "simple Text has no {forbidden}"
        );
    }
    assert!(text_rect(&output, "build a better version of this").is_some());
    open_actual_agent(&mut h, id);
    assert!(h.app.source_agent_inputs_visible(id));
    assert_eq!(h.app.doc().scene.node(id), Some(&before));
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn real_input_wires_remain_inspectable_and_visibility_tracks_undo_and_redo() {
    let mut h = prepare("simple-source-authored-input-visibility");
    let source = super::super::tests::place(&mut h, "brief.txt", 0.);
    let target = super::super::tests::place(&mut h, "target.png", 800.);
    assert!(!h.app.source_agent_inputs_visible(target));
    h.app.add_connector(
        ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: agent_inputs::OUTPUT_T,
        },
        ConnectorEnd::Anchored {
            node: target,
            side: Side::Left,
            t: agent_inputs::GENERATOR_PORTS[1].t,
        },
    );
    assert!(h.app.source_agent_inputs_visible(target));
    assert!(h
        .app
        .source_agent_input_visible(target, atlas_agent::InputSlot::Prompt));
    assert!(h
        .app
        .source_agent_input_visible(target, atlas_agent::InputSlot::Media));
    assert!(h
        .app
        .source_agent_input_visible(target, atlas_agent::InputSlot::Style));
    let target_node = h.app.doc().scene.node(target).unwrap();
    let actual = h.app.presented_wire_ports(target_node);
    assert_eq!(
        actual.len(),
        4,
        "an authored input implicitly activates every applicable grip plus output"
    );
    assert_eq!(actual[0].t, agent_inputs::GENERATOR_PORTS[0].t);
    assert_eq!(
        actual[0].point,
        h.app
            .wire_input_host(target_node)
            .anchor(Side::Left, actual[0].t),
        "remaining ports keep canonical positions"
    );
    assert!(
        !h.app.source_agent_inputs_visible(source),
        "an output wire does not open that source's Agent inputs"
    );
    h.app.board_undo();
    assert!(!h.app.source_agent_inputs_visible(target));
    h.app.board_redo();
    assert!(h.app.source_agent_inputs_visible(target));
    let saved = serde_json::to_vec(h.app.doc()).unwrap();
    h.app.tab_mut().doc = serde_json::from_slice(&saved).unwrap();
    assert!(h
        .app
        .source_agent_input_visible(target, atlas_agent::InputSlot::Style));
    assert!(!h.app.source_agent_inputs_visible(source));
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn actual_wire_snap_rejects_hidden_inputs_then_accepts_connected_and_expanded_slots() {
    let mut h = prepare("simple-source-real-snap-policy");
    h.app.tab_mut().cam.z = 1.0;
    let words = super::super::tests::place(&mut h, "brief.txt", 0.);
    let picture = super::super::tests::place(&mut h, "reference.png", -500.);
    let target = super::super::tests::place(&mut h, "target.png", 800.);
    let node = h.app.doc().scene.node(target).unwrap().clone();
    let host = h.app.wire_input_host(&node);
    let prompt_t = agent_inputs::GENERATOR_PORTS[1].t;
    let prompt = Pos2::from(host.anchor(Side::Left, prompt_t));
    let media_t = agent_inputs::GENERATOR_PORTS[0].t;
    let media = Pos2::from(host.anchor(Side::Left, media_t));
    let words_from = Some((words, Side::Right, agent_inputs::OUTPUT_T));
    let picture_from = Some((picture, Side::Right, agent_inputs::OUTPUT_T));
    assert_eq!(
        h.app.wire_snap_target(prompt, words_from),
        None,
        "a hidden Prompt cannot catch a wire through the arbitrary edge fallback"
    );
    assert_eq!(h.app.wire_snap_target(media, picture_from), None);
    h.app
        .add_connector(
            ConnectorEnd::Anchored {
                node: words,
                side: Side::Right,
                t: agent_inputs::OUTPUT_T,
            },
            ConnectorEnd::Anchored {
                node: target,
                side: Side::Left,
                t: prompt_t,
            },
        )
        .unwrap();
    assert_eq!(
        h.app.wire_snap_target(prompt, words_from),
        Some((target, Side::Left, prompt_t))
    );
    assert_eq!(
        h.app.wire_snap_target(media, picture_from),
        Some((target, Side::Left, media_t)),
        "one typed Prompt input implicitly activates the unused Media grip"
    );
    h.app.board_sel = [target].into_iter().collect();
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Agent));
    assert_eq!(
        h.app.wire_snap_target(media, picture_from),
        Some((target, Side::Left, media_t))
    );
    assert_eq!(
        h.app.wire_snap_target(prompt, words_from),
        Some((target, Side::Left, prompt_t))
    );
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn history_and_geometry_wires_do_not_implicitly_activate_generation_inputs() {
    let mut h = prepare("simple-source-nongeneration-wires");
    let source = super::super::tests::place(&mut h, "brief.txt", 0.);
    let target = super::super::tests::place(&mut h, "target.png", 800.);
    let wire = h
        .app
        .add_connector(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Right,
                t: agent_inputs::OUTPUT_T,
            },
            ConnectorEnd::Anchored {
                node: target,
                side: Side::Left,
                t: agent_inputs::GENERATOR_PORTS[1].t,
            },
        )
        .unwrap();
    assert!(h
        .app
        .source_agent_input_visible(target, atlas_agent::InputSlot::Style));
    h.app.patch_nodes(&[wire], |node| {
        let NodeKind::Connector(connector) = &mut node.kind else {
            panic!()
        };
        let binding = connector.binding.as_mut().unwrap();
        binding.kind = agent_inputs::InputKind::Memory;
        binding.slot = Some("generator_memory".into());
    });
    assert!(!h.app.source_agent_inputs_visible(target));
    h.app.board_undo();
    assert!(h
        .app
        .source_agent_input_visible(target, atlas_agent::InputSlot::Style));
    h.app.board_redo();
    assert!(!h.app.source_agent_inputs_visible(target));
    h.app.patch_nodes(&[wire], |node| {
        let NodeKind::Connector(connector) = &mut node.kind else {
            panic!()
        };
        connector.binding = None;
    });
    assert!(!h.app.source_agent_inputs_visible(target));
    assert!(!h.app.source_agent_inputs_visible(source));
    assert!(h.app.agents.dispatched.is_empty());
}
