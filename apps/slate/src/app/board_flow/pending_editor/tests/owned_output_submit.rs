//! Actual shared composer Submit must preserve the producer's durable Text role.
use super::*;

fn key(h: &mut Harness, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            })
        });
    }
}
fn drain(h: &mut Harness) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !h.app.agents.text_output.is_idle_for_test() && std::time::Instant::now() < deadline {
        h.app.pump_agent_text_outputs();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(h.app.agents.text_output.is_idle_for_test());
}
fn fixture(name: &str) -> (Harness, NodeId, slate_doc::ItemId) {
    let mut h = prepare(name);
    h.app.settings.agent_text_model = Some(("codex-text".into(), "discovered-native-id".into()));
    image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 80, 120, 255]))
        .save(h.base.join("source.png"))
        .unwrap();
    let primary = super::super::super::tests::place(&mut h, "source.png", -600.0);
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.spawn_flow_node(Some(
        &serde_json::json!({"source":primary.0,"kind":"text","chooser":true,"at":[20.0,20.0]})
            .to_string()
    )));
    let id = h.app.doc().scene.nodes[before].id;
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    let item = image.item;
    h.app.patch_nodes(&[id], |node| {
        node.rect = slate_doc::WorldRect::new(20.0, 20.0, 650.0, 390.0)
    });
    drain(&mut h);
    assert!(h.app.is_owned_text_output(id));
    assert_eq!(
        std::fs::read_to_string(h.app.item_path(item).unwrap()).unwrap(),
        ""
    );
    (h, id, item)
}
fn reopen(h: &mut Harness, item: slate_doc::ItemId) {
    drain(h);
    let path = h.base.join("owned-text-submit.slate");
    h.app.save_doc_to(h.app.tab().id, path.clone());
    h.app.tab_mut().doc = slate_doc::SlateDoc::load_from(&path).unwrap();
    h.app.invalidate_document_card(item);
    h.app.agents.text_output = Default::default();
    h.app.agent_blur();
    h.app.shape_properties.panel = None;
}
fn submit_visible(h: &mut Harness, id: NodeId, question: &str) -> atlas_agent::AgentRequest {
    if h.app.shape_properties.panel != Some(crate::app::board_properties::Panel::Agent) {
        open_actual_agent(h, id);
    }
    let selected = h
        .app
        .agents
        .flow
        .text_selections
        .get(&(h.app.tab().id, id))
        .and_then(|(source, range)| {
            h.app
                .text_answer_source(id)
                .filter(|(current, _)| current == source)?;
            source.get(range.clone()).map(str::to_owned)
        });
    let current = h.app.agent_prompt_text(id);
    let output = h.frame_output(|_| {});
    let hit = [
        current.as_str(),
        "Describe the text or the next change",
        "Message…",
    ]
    .into_iter()
    .filter(|text| !text.is_empty())
    .find_map(|text| text_rect(&output, text))
    .expect("actual editable composer text or hint");
    click(h, hit.center());
    key(h, egui::Key::A, egui::Modifiers::CTRL);
    h.frame_with(|input| input.events.push(egui::Event::Text(question.into())));
    let previous = h.app.agents.dispatched.len();
    key(h, egui::Key::Enter, egui::Modifiers::NONE);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h.app.agents.dispatched.len() == previous && std::time::Instant::now() < deadline {
        h.app.pump_comfy_queue();
        h.frame();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let (target, request) = h
        .app
        .agents
        .dispatched
        .get(previous)
        .expect("actual shared Submit must dispatch a new request")
        .clone();
    assert_eq!(target, id);
    assert_eq!(
        request.prompt,
        selected.map_or_else(
            || question.to_owned(),
            |text| format!("{question}\n\nSelected passage:\n{text}")
        )
    );
    assert!(
        request.text_source.is_none(),
        "an owned output does not quote an immutable original source witness"
    );
    assert!(!h.app.text_response_only(id));
    request
}
fn complete(h: &mut Harness, id: NodeId, request: &atlas_agent::AgentRequest, text: &str) {
    h.app.set_agent_session_for_test(
        id,
        atlas_agent::AgentSession {
            request: request.id.clone(),
            status: atlas_agent::AgentStatus::Idle,
            turns: vec![atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: text.into(),
                at: 1,
            }],
            ..Default::default()
        },
    );
    h.app.agent_pump(&h.ctx);
    assert!(!h.app.agent_is_running(id));
    h.app.sync_agent_text_output(id);
    drain(h);
}

#[test]
fn owned_text_final_idle_flushes_before_await_cleanup_on_first_and_bound_repeat() {
    let (mut h, id, item) = fixture("owned-text-terminal-file-flush");
    let path = h.app.item_path(item).unwrap();
    let partial = "The image shows a white dashboard titled “Native dashboard test.” Below the title, a teal card with rounded corners displays the number 42";
    let full = format!("{partial} in large white text.");
    let mut previous_request = None::<String>;
    for receipt_before_final in [true, false] {
        let request = submit_visible(
            &mut h,
            id,
            "Describe the connected picture in two sentences.",
        );
        let streaming = atlas_agent::AgentSession {
            request: request.id.clone(),
            status: atlas_agent::AgentStatus::Thinking,
            turns: vec![atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: partial.into(),
                at: 1,
            }],
            ..Default::default()
        };
        h.app.set_agent_session_for_test(id, streaming.clone());
        h.app.sync_agent_text_output(id);
        if receipt_before_final {
            drain(&mut h);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), partial);
        }
        let mut terminal = streaming;
        terminal.status = atlas_agent::AgentStatus::Idle;
        terminal.turns[0].text = full.clone();
        h.app.set_agent_session_for_test(id, terminal.clone());
        assert!(h.app.agent_is_awaiting(id));
        assert!(
            h.app.agent_is_running(id),
            "the await UI has not caught up with Idle"
        );
        // This is the ordering in pump_agent_sessions: accept final Idle, sync
        // its file, then let the separate await pump remove the running marker.
        h.app.sync_agent_text_output(id);
        drain(&mut h);
        assert_eq!(std::fs::read(&path).unwrap(), full.as_bytes());
        if let Some(older) = &previous_request {
            let mut stale = terminal.clone();
            stale.request = older.clone();
            stale.turns[0].text = "An older request cannot replace the current output.".into();
            h.app.set_agent_session_for_test(id, stale);
            h.app.sync_agent_text_output(id);
            drain(&mut h);
            assert_eq!(std::fs::read(&path).unwrap(), full.as_bytes());
            h.app.set_agent_session_for_test(id, terminal);
        }
        h.app.agent_pump(&h.ctx);
        assert!(!h.app.agent_is_awaiting(id));
        h.app.cache_snippet(item, Some(full.clone()));
        previous_request = Some(request.id);
    }
}

#[test]
fn owned_text_first_and_bound_repeat_use_actual_shared_composer_after_save_reload() {
    let (mut h, id, item) = fixture("owned-text-actual-shared-submit");
    reopen(&mut h, item);
    assert!(
        h.app.text_answer_source(id).is_none(),
        "first send has no fabricated Some(empty) cache"
    );
    let first = submit_visible(&mut h, id, "Describe the connected image.");
    complete(&mut h, id, &first, "Generated description.");
    assert_eq!(
        std::fs::read_to_string(h.app.item_path(item).unwrap()).unwrap(),
        "Generated description."
    );
    assert!(h.app.is_owned_text_output(id));
    reopen(&mut h, item);
    let second = submit_visible(&mut h, id, "Write one shorter alternative.");
    assert_ne!(first.id, second.id);
    assert!(
        h.app.is_owned_text_output(id),
        "repeat generation retains the same owned output role"
    );
}

#[test]
fn saved_owned_empty_text_with_legacy_review_flag_recovers_on_actual_bound_submit() {
    let (mut h, id, item) = fixture("owned-text-saved-incorrect-review-role");
    h.app.prepare_pending_chat_choice(id);
    let draft = h.app.agents.flow.drafts.get(&id).unwrap().clone();
    assert!(h.app.bind_pending_generator(
        id,
        PendingHost::Text,
        &draft,
        "codex-text",
        "discovered-native-id"
    ));
    assert!(!h.app.text_response_only(id));
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .text_response_only = true
    });
    reopen(&mut h, item);
    assert!(h.app.text_response_only(id) && h.app.is_owned_text_output(id));
    assert!(h.app.text_answer_source(id).is_none());
    let request = submit_visible(&mut h, id, "Describe the connected image after reopening.");
    assert_eq!(request.model.as_deref(), Some("discovered-native-id"));
    assert_eq!(
        std::fs::read_to_string(h.app.item_path(item).unwrap()).unwrap(),
        ""
    );
}

#[test]
fn owned_generated_unicode_passage_query_links_answer_without_replacing_source_then_can_generate() {
    let (mut h, id, item) = fixture("owned-text-shared-submit-selected-passage");
    let first = submit_visible(&mut h, id, "Write a short generated document.");
    let source = "A café & tea. Keep these words.";
    complete(&mut h, id, &first, source);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while h
        .app
        .text_answer_source(id)
        .is_none_or(|(text, _)| text != source)
        && std::time::Instant::now() < deadline
    {
        h.frame();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(h.app.text_answer_source(id).unwrap().0, source);
    h.app
        .capture_text_answer_selection(&h.ctx, id, source, 2..6);
    let query = submit_visible(&mut h, id, "What does this word mean?");
    assert_eq!(query.prompt.matches("Selected passage:\ncafé").count(), 1);
    complete(&mut h, id, &query, "Coffee.");
    assert_eq!(
        std::fs::read_to_string(h.app.item_path(item).unwrap()).unwrap(),
        source
    );
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(image.answers.len(), 1);
    assert_eq!(image.answers[0].quote, "café");
    assert_eq!(image.answers[0].answer, "Coffee.");
    h.app
        .agents
        .flow
        .text_selections
        .remove(&(h.app.tab().id, id));
    let normal = submit_visible(&mut h, id, "Write another ordinary generated document.");
    assert_eq!(normal.prompt, "Write another ordinary generated document.");
    assert!(normal.text_source.is_none());
    assert!(!h.app.text_response_only(id));
}

#[test]
fn exact_owned_source_classifier_never_promotes_unmarked_or_replaced_originals() {
    for marker in [None, Some("another-source.txt")] {
        let mut h = prepare("text-owned-role-negative");
        let id = super::super::super::tests::place(&mut h, "original.txt", 0.0);
        std::fs::write(h.base.join("original.txt"), "").unwrap();
        h.app.prepare_pending_chat_choice(id);
        let draft = h.app.agents.flow.drafts.get(&id).unwrap().clone();
        assert!(h.app.bind_pending_generator(
            id,
            PendingHost::Text,
            &draft,
            "codex-text",
            "discovered-native-id"
        ));
        h.app.patch_nodes(&[id], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.generator.owned_text_output = marker.map(|locator| slate_doc::SourceUri {
                locator: locator.into(),
            });
            agent.generator.text_response_only = false;
        });
        assert!(!h.app.is_owned_text_output(id));
        h.app.activate_text_review_mode(id);
        assert!(h.app.text_response_only(id));
        *h.app.agents.prompt_mut(id) = "Question about original".into();
        h.app.board_sel = [id].into_iter().collect();
        assert!(h.app.dispatch(
            &h.ctx.clone(),
            atlas_commands::CommandId("portal.agent.send"),
            None
        ));
        assert!(h.app.agents.dispatched.is_empty());
        assert_eq!(
            h.app.agent_failure_reason(id),
            Some("Wait for the original text source to load."),
            "an empty original still requires the immutable-source readiness guard"
        );
        assert_eq!(
            std::fs::read_to_string(h.base.join("original.txt")).unwrap(),
            ""
        );
    }
}
