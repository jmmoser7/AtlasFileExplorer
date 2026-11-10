use super::*;

#[test]
fn wire_created_text_registered_send_binds_its_remembered_model_before_dispatch() {
    let mut h = prepare("unbound-text-registered-first-send");
    h.app.settings.agent_text_model = Some(("codex-text".into(), "discovered-native-id".into()));
    // A different Image preference must not choose this Text request's provider.
    h.app.settings.agent_image_model = Some(("comfy".into(), String::new()));
    let primary = super::super::super::tests::place(&mut h, "primary.png", -600.0);
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({
            "source":primary.0,"kind":"text","chooser":true,"at":[300.0,100.0]
        })
        .to_string()
    )));
    let id = h.app.doc().scene.nodes[before].id;
    let pending = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(pending.provider, "codex-text");
    assert_eq!(pending.model.as_deref(), Some("discovered-native-id"));
    assert!(!pending.session.is_empty());
    assert!(
        h.app.agents.dispatched.is_empty(),
        "remembering a choice never sends a request"
    );
    let prompt = super::super::super::tests::place(&mut h, "instructions.txt", -300.0);
    let NodeKind::Image(image) = &h.app.doc().scene.node(prompt).unwrap().kind else {
        panic!()
    };
    let item = image.item;
    h.app
        .cache_snippet(item, Some("Describe the connected picture.".into()));
    let t = h
        .app
        .wire_input_ports(h.app.doc().scene.node(id).unwrap())
        .iter()
        .find(|port| port.slot == atlas_agent::InputSlot::Prompt)
        .unwrap()
        .t;
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: prompt,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: id,
            side: Side::Left,
            t,
        },
    );
    h.app.add_nodes(vec![wire]);
    // Saving/reopening must not depend on the producer's transient Some("")
    // preview or writer slot. Empty files intentionally hydrate to no excerpt.
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    let output_item = image.item;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !h.app.agents.text_output.is_idle_for_test() && std::time::Instant::now() < deadline {
        h.app.pump_agent_text_outputs();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(h.app.agents.text_output.is_idle_for_test());
    assert_eq!(
        std::fs::read_to_string(h.app.item_path(output_item).unwrap()).unwrap(),
        ""
    );
    let saved = h.base.join("before-first-text-send.slate");
    h.app.save_doc_to(h.app.tab().id, saved.clone());
    h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&saved).unwrap();
    h.app.invalidate_document_card(output_item);
    h.app.agents.text_output = Default::default();
    assert!(h.app.text_answer_source(id).is_none());
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .generator
            .owned_text_output
            .is_some()
    );
    h.app.board_sel = [id].into_iter().collect();
    assert!(h.app.dispatch(
        &h.ctx.clone(),
        atlas_commands::CommandId("portal.agent.send"),
        None
    ));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.agents.dispatched.is_empty() && std::time::Instant::now() < deadline {
        h.app.pump_comfy_queue();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(h.app.agents.dispatched.len(), 1);
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(agent.provider, "codex-text");
    assert!(!agent.session.is_empty());
    assert_eq!(agent.model.as_deref(), Some("discovered-native-id"));
    assert!(
        !agent.generator.text_response_only,
        "an owned output is not an original-source review"
    );
    assert!(agent
        .bundle
        .as_ref()
        .unwrap()
        .locator
        .contains(&agent.session));
    let (target, request) = &h.app.agents.dispatched[0];
    assert_eq!(*target, id);
    assert!(request.image.is_none());
    assert_eq!(request.model.as_deref(), Some("discovered-native-id"));
    assert_eq!(
        request
            .prompt_with_history()
            .matches("Describe the connected picture.")
            .count(),
        1
    );
    assert_eq!(
        request
            .inputs
            .on(atlas_agent::InputSlot::Media)
            .next()
            .unwrap()
            .node,
        primary.0
    );
    assert!(!h.base.join(".atlas-ai/agent/request.json").exists());
    let request_id = request.id.clone();
    h.app.set_agent_session_for_test(
        id,
        atlas_agent::AgentSession {
            request: request_id,
            status: atlas_agent::AgentStatus::Idle,
            turns: vec![atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "Synthetic description of the connected picture.".into(),
                at: 1,
            }],
            ..Default::default()
        },
    );
    h.app.agent_pump(&h.ctx);
    assert!(
        !h.app.agent_is_running(id),
        "the matching Idle response clears the sent state"
    );
    h.app.sync_agent_text_output(id);
    let path = h.app.item_path(output_item).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::fs::read_to_string(&path).ok().as_deref()
        != Some("Synthetic description of the connected picture.")
        && std::time::Instant::now() < deadline
    {
        h.app.pump_agent_text_outputs();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "Synthetic description of the connected picture."
    );
    assert_eq!(
        h.app.agent_reply(id).as_deref(),
        Some("Synthetic description of the connected picture.")
    );
}

#[test]
fn imported_empty_or_missing_text_never_becomes_an_owned_output_on_first_send() {
    for (missing, changed_item) in [(false, false), (true, false), (false, true)] {
        let mut h = prepare("original-text-requires-source-witness");
        h.app.settings.agent_text_model =
            Some(("codex-text".into(), "discovered-native-id".into()));
        let id = super::super::super::tests::place(&mut h, "original.txt", 0.0);
        let source = h.base.join("original.txt");
        if missing {
            std::fs::remove_file(&source).unwrap();
        } else {
            std::fs::write(&source, "").unwrap();
        }
        let after = h.app.graph_settings_node(id).unwrap();
        h.app.patch_nodes(&[id], |node| *node = after.clone());
        if changed_item {
            // A source change cannot inherit ownership from another output.
            h.app.patch_nodes(&[id], |node| {
                slate_doc::agent_chat::agent_mut(node)
                    .unwrap()
                    .generator
                    .owned_text_output = Some(slate_doc::SourceUri {
                    locator: h
                        .base
                        .join("previous-owned-response.txt")
                        .to_string_lossy()
                        .into(),
                });
            });
        }
        h.app.prepare_pending_chat_choice(id);
        h.app.agents.flow.drafts.get_mut(&id).unwrap().prompt =
            "Review this original source.".into();
        h.app.board_sel = [id].into_iter().collect();
        assert!(h.app.dispatch(
            &h.ctx.clone(),
            atlas_commands::CommandId("portal.agent.send"),
            None
        ));
        let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
        assert!(agent.generator.text_response_only);
        assert_eq!(agent.generator.owned_text_output.is_some(), changed_item);
        assert!(h.app.agents.dispatched.is_empty());
        assert_eq!(
            h.app.agent_failure_reason(id),
            Some("Wait for the original text source to load.")
        );
        if missing {
            assert!(!source.exists());
        } else {
            assert_eq!(std::fs::read_to_string(source).unwrap(), "");
        }
    }
}

#[test]
fn ordinary_first_send_accepts_live_wired_prompts_without_authoring_a_copy() {
    for kind in [
        PendingHost::Image,
        PendingHost::Text,
        PendingHost::Artifact,
        PendingHost::Model,
    ] {
        for preserve in [false, true] {
            for authored in ["", "Keep the framing"] {
                let mut h = prepare(&format!("wired-first-{kind:?}-{preserve}-{authored}"));
                let id = if kind == PendingHost::Artifact {
                    let path = h.base.join("source.html");
                    std::fs::write(&path, "<!doctype html><p>Original page</p>").unwrap();
                    let node = h.app.doc_mut().scene.build_node(
                        slate_doc::WorldRect::new(20.0, 20.0, 640.0, 300.0),
                        NodeKind::Portal(PortalNode::bound_web("Source", path.to_string_lossy())),
                    );
                    let id = node.id;
                    h.app.add_nodes(vec![node]);
                    id
                } else {
                    let file = match kind {
                        PendingHost::Image => "source.png",
                        PendingHost::Text => "source.txt",
                        _ => "source.obj",
                    };
                    std::fs::write(h.base.join(file), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")
                        .unwrap();
                    super::super::super::tests::place(&mut h, file, 0.0)
                };
                if kind == PendingHost::Text {
                    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
                        panic!()
                    };
                    let target_item = image.item;
                    h.app
                        .cache_snippet(target_item, Some("Owned original text.".into()));
                }
                assert_eq!(h.app.pending_generator_host(id), Some(kind));
                let source = super::super::super::tests::place(&mut h, "prompt.txt", -600.0);
                let NodeKind::Image(image) = &h.app.doc().scene.node(source).unwrap().kind else {
                    panic!("linked document fixture");
                };
                let item = image.item;
                let t = h
                    .app
                    .wire_input_ports(h.app.doc().scene.node(id).unwrap())
                    .iter()
                    .find(|port| port.slot == atlas_agent::InputSlot::Prompt)
                    .unwrap()
                    .t;
                let wire = h.app.build_connector(
                    ConnectorEnd::Anchored {
                        node: source,
                        side: Side::Right,
                        t: 0.5,
                    },
                    ConnectorEnd::Anchored {
                        node: id,
                        side: Side::Left,
                        t,
                    },
                );
                h.app.add_nodes(vec![wire]);
                let mut draft = AgentDraft::new();
                draft.prompt = authored.into();
                draft.preserve_history = preserve;
                draft.choice = Some(match kind {
                    PendingHost::Image => ("comfy".into(), String::new()),
                    PendingHost::Text => ("ollama".into(), String::new()),
                    _ => ("codex".into(), "discovered-native-id".into()),
                });
                // A late preview completion must enable the first Send without
                // copying the linked words into the authored draft.
                assert_eq!(h.app.project_agent_prompt(id, &draft.prompt), authored);
                h.app.cache_snippet(item, Some("Old linked words".into()));
                assert!(h
                    .app
                    .project_agent_prompt(id, &draft.prompt)
                    .contains("Old linked words"));
                h.app
                    .cache_snippet(item, Some("Current linked words".into()));
                let expected = h.app.project_agent_prompt(id, &draft.prompt);
                assert!(expected.contains("Current linked words"));
                assert!(!expected.contains("Old linked words"));
                h.app.submit_pending_generator(&h.ctx, id, &mut draft);
                let captured = h
                    .app
                    .agents
                    .dispatched
                    .iter()
                    .rev()
                    .find(|(target, _)| *target == id)
                    .map(|(_, request)| request);
                let accepted = captured.is_some();
                if matches!(kind, PendingHost::Image | PendingHost::Text) {
                    let request = captured.expect(
                        "the real first Send captures linked-only and linked+typed requests",
                    );
                    let expected = if authored.is_empty() {
                        "Current linked words".to_owned()
                    } else {
                        format!(
                            "Current linked words{}{authored}",
                            if kind == PendingHost::Image {
                                ", "
                            } else {
                                "\n\n"
                            }
                        )
                    };
                    assert_eq!(request.prompt, expected);
                    assert_eq!(
                        request
                            .prompt_with_history()
                            .matches("Current linked words")
                            .count(),
                        1
                    );
                    assert!(!request.prompt_with_history().contains("Old linked words"));
                }
                let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                    .expect("linked-only first Send must bind the ordinary source");
                let cleared_instruction = accepted
                    && preserve
                    && matches!(kind, PendingHost::Image | PendingHost::Artifact);
                assert_eq!(
                    agent.instruction,
                    if cleared_instruction { "" } else { authored }
                );
                assert_eq!(agent.generator.preserve_history, Some(preserve));
                assert_eq!(draft.prompt, authored);
                let effective = h.app.agent_prompt_text(id);
                assert!(effective.contains("Current linked words"));
                assert!(!effective.contains("Old linked words"));
                assert_eq!(effective.matches("Current linked words").count(), 1);
                let cleared_composer =
                    accepted && (kind == PendingHost::Text || cleared_instruction);
                if cleared_composer || authored.is_empty() {
                    assert_eq!(effective, "Current linked words");
                } else {
                    assert!(effective.ends_with(authored));
                }
            }
        }
    }
}
