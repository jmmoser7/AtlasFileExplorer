// Selected text outputs and navigation retain their recorded source.

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
