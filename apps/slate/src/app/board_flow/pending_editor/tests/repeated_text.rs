use super::*;
fn send(h: &mut Harness, id: NodeId, question: &str) -> atlas_agent::AgentRequest {
    // The source host is already open; clicking the actual composer takes focus.
    let output = h.frame_output(|_| {});
    if let Some(rect) = ["Describe the text or the next change", "Message…"]
        .iter()
        .find_map(|hint| text_rect(&output, hint))
    {
        click(h, rect.center());
    }
    h.frame_with(|input| input.events.push(egui::Event::Text(question.into())));
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
    for _ in 0..5 {
        h.frame();
    }
    let request = h
        .app
        .agents
        .dispatched
        .iter()
        .rev()
        .find(|(node, _)| *node == id)
        .expect("actual Send reaches test transport")
        .1
        .clone();
    assert_eq!(
        request.prompt, question,
        "the transport question is the exact visible composer text"
    );
    assert_eq!(
        request
            .prompt_with_history()
            .matches("Source text (quoted document data):")
            .count(),
        1
    );
    request
}
fn complete(h: &mut Harness, id: NodeId, request: &atlas_agent::AgentRequest, answer: &str) {
    h.app.agents.sessions.insert(
        id,
        std::sync::Arc::new(atlas_agent::AgentSession {
            request: request.id.clone(),
            provider: "ollama".into(),
            status: atlas_agent::AgentStatus::Idle,
            turns: vec![
                atlas_agent::AgentTurn {
                    role: "user".into(),
                    text: request.prompt.clone(),
                    at: 0,
                },
                atlas_agent::AgentTurn {
                    role: "assistant".into(),
                    text: answer.into(),
                    at: 0,
                },
            ],
            ..Default::default()
        }),
    );
    h.app.finish_text_answer_requests();
    h.frame();
}
fn plain(h: &mut Harness) -> NodeId {
    let text = slate_doc::scene::TextNode {
        text: "Source café.".into(),
        answers: Vec::new(),
        style: Default::default(),
        family: slate_doc::scene::Typeface::Sans,
        size: 16.0,
        color: slate_doc::scene::STICKY_INK,
        align: Default::default(),
        fill: None,
        stroke: Default::default(),
        agent: None,
    };
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20.0, 20.0, 650.0, 390.0),
        NodeKind::Text(text),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    open_actual_agent(h, id);
    id
}
fn toggle(h: &mut Harness, id: NodeId, expected: bool) {
    h.frame();
    let hit = h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new("painted-generator-options-hit")))
        .unwrap();
    click(h, hit.center());
    let output = h.frame_output(|_| {});
    click(
        h,
        text_rect(&output, "Preserve model history")
            .unwrap()
            .center(),
    );
    assert_eq!(
        h.app
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .and_then(|agent| agent.generator.preserve_history)
            .or_else(|| h.app.pending_generator_preserve(id)),
        Some(expected)
    );
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
}
#[test]
fn enabling_preserve_during_current_off_run_can_admit_it_without_resurrecting_completed_off_turns()
{
    let mut h = prepare("text-active-off-opt-in");
    let id = plain(&mut h);
    let old = send(&mut h, id, "Completed OFF");
    complete(&mut h, id, &old, "Excluded old answer");
    let active = send(&mut h, id, "Current OFF");
    assert!(active.history.is_empty());
    toggle(&mut h, id, true);
    complete(&mut h, id, &active, "Current answer");
    let next = send(&mut h, id, "Next ON");
    assert_eq!(
        next.history
            .iter()
            .map(|turn| turn.text.as_str())
            .collect::<Vec<_>>(),
        ["Current OFF", "Current answer"]
    );
    assert!(!next.prompt_with_history().contains("Excluded old answer"));
    assert_eq!(h.app.text_answer_source(id).unwrap().0, "Source café.");
}
#[test]
fn pending_count_branches_cannot_reseed_context_after_off_on_cut_or_replace_first_output() {
    let mut h = prepare("text-pending-count-cut");
    let id = plain(&mut h);
    toggle(&mut h, id, true);
    let output = h.frame_output(|_| {});
    // The Studio Count is a chevron stepper, not a popup menu. Its32px
    // actual control has13px arrow targets at either end.
    let count = text_rect(&output, "1")
        .expect("actual Count control")
        .center();
    let zoom = h.app.board_xf().z;
    click(&mut h, count + egui::vec2(9.5 * zoom, 0.0));
    let output = h.frame_output(|_| {});
    assert!(
        text_rect(&output, "2").is_some(),
        "actual increment paints Count2"
    );
    assert_eq!(h.app.agents.flow.drafts[&id].count, 2);
    let first = send(&mut h, id, "Two options");
    toggle(&mut h, id, false);
    toggle(&mut h, id, true);
    complete(&mut h, id, &first, "First option");
    for _ in 0..3 {
        h.frame();
    }
    let second = h
        .app
        .agents
        .dispatched
        .iter()
        .rev()
        .find(|(node, _)| *node == id)
        .unwrap()
        .1
        .clone();
    assert_ne!(first.id, second.id);
    complete(&mut h, id, &second, "Second option");
    let generator = &slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .generator;
    assert_eq!(generator.text_reviews.len(), 2);
    assert_eq!(generator.shown_review, Some(0));
    assert_eq!(generator.text_reviews[0].text, "First option");
    assert_eq!(
        generator.text_reviews[0].group,
        generator.text_reviews[1].group
    );
    assert!(
        generator.text_context.is_empty(),
        "pre-cut options never become future context"
    );
    complete(&mut h, id, &first, "Late first option");
    let generator = &slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .generator;
    assert_eq!(generator.text_reviews.len(), 2);
    assert!(generator.text_context.is_empty());
    assert_eq!(h.app.text_answer_source(id).unwrap().0, "Source café.");
}
#[test]
fn plain_no_agent_text_first_and_repeat_sends_respect_history_cut_and_saved_response_identity() {
    let mut h = prepare("plain-text-repeat-history");
    let source = "A café & tea. Keep these authored words.";
    let text = slate_doc::scene::TextNode {
        text: source.into(),
        answers: Vec::new(),
        style: Default::default(),
        family: slate_doc::scene::Typeface::Sans,
        size: 16.0,
        color: slate_doc::scene::STICKY_INK,
        align: Default::default(),
        fill: None,
        stroke: Default::default(),
        agent: None,
    };
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(20.0, 20.0, 650.0, 390.0),
        NodeKind::Text(text),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    let original = h.app.doc().scene.node(id).unwrap().clone();
    let count = h.app.doc().scene.nodes.len();
    open_actual_agent(&mut h, id);
    assert_eq!(h.app.doc().scene.node(id), Some(&original));
    let first = send(&mut h, id, "  Question one  ");
    assert!(first.history.is_empty());
    assert_eq!(first.text_source.as_deref(), Some(source));
    complete(&mut h, id, &first, "Answer one");
    let second = send(&mut h, id, "Question two");
    assert!(second.history.is_empty());
    assert_ne!(first.id, second.id);
    complete(&mut h, id, &second, "Answer two");
    let reviews = &slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .generator
        .text_reviews;
    assert_eq!(reviews.len(), 2);
    let first_review = reviews[0].id.clone();
    let through = reviews[0].output.response.through;
    assert!(
        h.app.prepare_text_output_review(id, through, None).is_err(),
        "one-shot checkpoints require saved response identity"
    );
    assert!(h.app.select_text_output(Some(
        &serde_json::json!({"node":id,"through":through,"review_id":first_review}).to_string()
    )));
    assert_eq!(h.app.text_review_view(id).unwrap().text, "Answer one");
    // Enable Preserve through the actual same-host More menu; old OFF turns stay cut.
    h.frame();
    let hit = h
        .ctx
        .data(|data| data.get_temp::<Rect>(Id::new("painted-generator-options-hit")))
        .unwrap();
    click(&mut h, hit.center());
    let output = h.frame_output(|_| {});
    click(
        &mut h,
        text_rect(&output, "Preserve model history")
            .unwrap()
            .center(),
    );
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    let third = send(&mut h, id, "Question three");
    assert!(third.history.is_empty());
    complete(&mut h, id, &third, "Answer three");
    let fourth = send(&mut h, id, "Question four");
    assert_eq!(
        fourth
            .history
            .iter()
            .map(|turn| turn.text.as_str())
            .collect::<Vec<_>>(),
        ["Question three", "Answer three"]
    );
    assert_eq!(h.app.text_answer_source(id).unwrap().0, source);
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, original.rect);
}
