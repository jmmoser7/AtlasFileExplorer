use super::*;

fn selected_note(tag: &str) -> (crate::app::tests::Harness, NodeId) {
    let (mut h, generator, note) =
        super::super::test_support::generator_with_note(tag, "A café & tea.");
    let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(generator).unwrap())
        .unwrap()
        .clone();
    agent.view = atlas_agent::PortalView::Text;
    agent.instruction = "Explain this".into();
    h.app.patch_nodes(&[note], |n| {
        if let NodeKind::Text(t) = &mut n.kind {
            t.agent = Some(Box::new(agent.clone()));
        }
    });
    h.app
        .capture_text_answer_selection(&egui::Context::default(), note, "A café & tea.", 2..6);
    (h, note)
}

fn reply(h: &mut crate::app::tests::Harness, id: NodeId, request: &str) {
    h.app.set_agent_session_for_test(
        id,
        AgentSession {
            request: request.into(),
            status: AgentStatus::Idle,
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: "Explain this".into(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "Coffee".into(),
                    at: 2,
                },
            ],
            ..Default::default()
        },
    );
}

#[test]
fn selected_passage_uses_submitted_question_and_links_only_matching_completed_response() {
    let (mut h, id) = selected_note("selected-text-answer");
    h.app.agents.awaiting.insert(
        id,
        AgentAwait::Sent {
            at: Instant::now(),
            req_at: 1,
        },
    );
    h.app.queue_text_block(id);
    *h.app.agents.prompt_mut(id) = "Later unsent question".into();
    h.app.agents.awaiting.remove(&id);
    h.app.pump_comfy_queue();
    let sent = h.app.agents.dispatched.last().unwrap().1.clone();
    assert!(sent.prompt.contains("Explain this"));
    assert!(sent.prompt.contains("Selected passage:\ncafé"));
    assert!(!sent.prompt.contains("Later unsent"));
    reply(&mut h, id, "another-request");
    h.app.finish_text_answer_requests();
    assert_eq!(h.app.agents.flow.text_answer_requests.len(), 1);
    reply(&mut h, id, &sent.id);
    h.app.finish_text_answer_requests();
    let NodeKind::Text(text) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(text.text, "A café & tea.");
    assert_eq!(text.answers.len(), 1);
    assert_eq!(text.answers[0].quote, "café");
    assert_eq!(text.answers[0].response.through, 2);
    assert!(h.app.agents.flow.text_answer_requests.is_empty());
}

#[test]
fn changed_text_before_dispatch_never_sends_selected_query() {
    let (mut h, id) = selected_note("selected-text-changed-queued");
    h.app.agents.awaiting.insert(
        id,
        AgentAwait::Sent {
            at: Instant::now(),
            req_at: 1,
        },
    );
    h.app.queue_text_block(id);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Text(t) = &mut n.kind {
            t.text = "Changed".into();
        }
    });
    h.app.agents.awaiting.remove(&id);
    h.app.pump_comfy_queue();
    assert!(h.app.agents.dispatched.is_empty());
    assert!(h.app.agents.flow.text_answer_requests.is_empty());
}

#[test]
fn cancelled_or_edited_requests_cannot_link_late_success() {
    for cancelled in [false, true] {
        let (mut h, id) = selected_note(&format!("selected-text-late-{cancelled}"));
        h.app.queue_text_block(id);
        h.app.pump_comfy_queue();
        let sent = h.app.agents.dispatched.last().unwrap().1.clone();
        if cancelled {
            h.app
                .agents
                .cancelled_generations
                .insert(id, sent.id.clone());
        } else {
            h.app.patch_nodes(&[id], |n| {
                if let NodeKind::Text(t) = &mut n.kind {
                    t.text.push('!');
                }
            });
        }
        reply(&mut h, id, &sent.id);
        h.app.finish_text_answer_requests();
        let NodeKind::Text(text) = &h.app.doc().scene.node(id).unwrap().kind else {
            panic!()
        };
        assert!(text.answers.is_empty());
        assert!(h.app.agents.flow.text_answer_requests.is_empty());
    }
}

#[test]
fn linked_text_query_records_shared_answer_metadata_without_override_or_source_write() {
    let (mut h, id) = selected_note("linked-text-query");
    let path = h.base.join("original.txt");
    std::fs::write(&path, "A café & tea.").unwrap();
    let item = h.app.add_paths(std::slice::from_ref(&path))[0];
    let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .clone();
    agent.generator.preserve_history = Some(true);
    h.app.patch_nodes(&[id], |node| {
        let mut image = slate_doc::scene::ImageNode::new(item);
        image.agent = Some(Box::new(agent.clone()));
        node.kind = NodeKind::Image(image);
    });
    h.app.cache_snippet(item, Some("A café & tea.".into()));
    // Existing generated text uses this same slot; queries must leave it alone.
    h.app
        .agents
        .text_output
        .write_now(id, item, path.clone(), "A café & tea.".into());
    // The existing slot write is asynchronous and can briefly truncate before
    // replacing identical bytes. Finish setup before testing query immutability.
    let setup_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if h.app
            .agents
            .text_output
            .pump()
            .iter()
            .any(|(written, target)| *written == item && target == &path)
        {
            break;
        }
        assert!(
            Instant::now() < setup_deadline,
            "setup writer completion arrives"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "A café & tea.");
    h.app
        .capture_text_answer_selection(&h.ctx, id, "A café & tea.", 2..6);
    h.app.queue_text_block(id);
    h.app.pump_comfy_queue();
    let request = h.app.agents.dispatched.last().unwrap().1.clone();
    assert!(!request.oneshot, "Preserve remains enabled for the query");
    assert!(request.prompt.contains(
        "Selected passage:
café"
    ));
    reply(&mut h, id, &request.id);
    h.app.finish_text_answer_requests();
    h.app.sync_agent_text_output(id);
    h.app.pump_agent_text_outputs();
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert!(image.text_override.is_none());
    assert_eq!(image.answers.len(), 1);
    assert_eq!(image.answers[0].quote, "café");
    let revision = &h.app.doc().item(item).unwrap().cache_key;
    assert!(image.answers[0].is_valid_for_revision("A café & tea.", Some(revision)));
    assert!(!image.answers[0].is_valid_for_revision("A café & tea.", Some("refreshed-source")));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "A café & tea.");
    assert!(h.app.text_answer_owns_request(id));
    let previous_query = h.app.agents.sessions[&id].clone();
    h.app
        .agents
        .flow
        .text_selections
        .remove(&(h.app.tab().id, id));
    h.app.agents.awaiting.remove(&id);
    *h.app.agents.prompt_mut(id) = "Write the ordinary generated document".into();
    h.app.cache_snippet(item, Some("A café & tea.".into()));
    h.app.queue_text_block(id);
    let ordinary = h.app.agents.dispatched.last().unwrap().1.clone();
    assert_ne!(ordinary.id, request.id);
    assert!(
        !ordinary.oneshot,
        "ordinary output replacement does not disable provider history"
    );
    assert_eq!(ordinary.history.last().unwrap().text, "Coffee");
    assert!(!h.app.text_answer_owns_request(id));
    // An old query snapshot cannot repopulate the newly cleared ordinary run.
    h.app.agents.sessions.insert(id, previous_query.clone());
    h.app.sync_agent_text_output(id);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        h.app.pump_agent_text_outputs();
        if std::fs::read_to_string(&path).unwrap().is_empty() {
            break;
        }
        assert!(
            Instant::now() < until,
            "ordinary run must clear its previous owned output"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut completed = previous_query.as_ref().clone();
    completed.request = ordinary.id.clone();
    completed.turns.extend([
        AgentTurn {
            role: "user".into(),
            text: ordinary.prompt.clone(),
            at: 3,
        },
        AgentTurn {
            role: "assistant".into(),
            text: "Ordinary generated body.".into(),
            at: 4,
        },
    ]);
    h.app.set_agent_session_for_test(id, completed.clone());
    h.app.agents.awaiting.remove(&id);
    h.app.sync_agent_text_output(id);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        h.app.pump_agent_text_outputs();
        if std::fs::read_to_string(&path).unwrap() == "Ordinary generated body." {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    // A subsequent query keeps both an explicit authored override and source bytes.
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.text_override = Some("Authored passage remains".into());
        }
    });
    h.app
        .capture_text_answer_selection(&h.ctx, id, "Authored passage remains", 0..8);
    *h.app.agents.prompt_mut(id) = "Explain the selected words".into();
    h.app.queue_text_block(id);
    let next_query = h.app.agents.dispatched.last().unwrap().1.clone();
    assert_ne!(next_query.id, ordinary.id);
    assert!(!next_query.oneshot);
    assert_eq!(
        next_query.history.last().unwrap().text,
        "Ordinary generated body."
    );
    h.app.agents.sessions.insert(id, previous_query); // stale old completion
    h.app.sync_agent_text_output(id);
    h.app.pump_agent_text_outputs();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "Ordinary generated body."
    );
    completed.request = next_query.id.clone();
    completed.turns.extend([
        AgentTurn {
            role: "user".into(),
            text: next_query.prompt.clone(),
            at: 5,
        },
        AgentTurn {
            role: "assistant".into(),
            text: "A query answer, not source content.".into(),
            at: 6,
        },
    ]);
    h.app.set_agent_session_for_test(id, completed);
    h.app.finish_text_answer_requests();
    h.app.sync_agent_text_output(id);
    h.app.pump_agent_text_outputs();
    let NodeKind::Image(image) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(
        image.text_override.as_deref(),
        Some("Authored passage remains")
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "Ordinary generated body."
    );
    assert!(h.app.text_answer_owns_request(id));
}

#[test]
fn answer_navigation_after_cut_reopen_uses_custom_origin_without_restoring_memory() {
    let (mut h, id) = selected_note("text-answer-cut-origin");
    let workspace = h.base.join("ai");
    std::fs::create_dir_all(&workspace).unwrap();
    h.app.ai.config.workspace_dir = Some(workspace);
    let archive = h.base.join("custom-source-not-session-id");
    std::fs::create_dir_all(&archive).unwrap();
    let manifest = archive.join("session.json");
    reply(&mut h, id, "old-query");
    let old = h.app.agents.sessions.get(&id).unwrap().as_ref().clone();
    std::fs::write(&manifest, serde_json::to_vec(&old).unwrap()).unwrap();
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().bundle = Some(slate_doc::scene::SourceUri {
            locator: manifest.to_string_lossy().into_owned(),
        })
    });
    let session = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .session
        .clone();
    assert!(h.app.record_text_answer(
        id,
        "A café & tea.",
        2..7,
        slate_doc::text_answers::TextAnswerRef {
            session,
            through: 2
        },
        "Explain".into(),
        "Coffee".into()
    ));
    let cut = h.app.generator_history_cut_patch(id).unwrap();
    assert!(h.app.commit_scene(vec![cut]));
    let NodeKind::Text(text) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    let link = text.answers[0].clone();
    let new_session = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .session
        .clone();
    let doc: slate_doc::SlateDoc =
        serde_json::from_value(serde_json::to_value(h.app.doc()).unwrap()).unwrap();
    h.app.tab_mut().doc = doc;
    h.app.agents.sessions.insert(
        id,
        std::sync::Arc::new(AgentSession {
            request: "new-session".into(),
            ..Default::default()
        }),
    );
    h.app.agents.flow.text_answer_navigation = Some((
        h.app.tab().id,
        id,
        slate_doc::text_answers::TextOutputRef {
            review_id: None,
            source: link.origin.unwrap(),
            response: link.response,
        },
    ));
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
            .get(1)
            .unwrap()
            .text,
        "Coffee"
    );
    assert!(
        h.app.agents.sessions[&id].turns.is_empty(),
        "viewing archive must not repopulate inference history"
    );
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap()).unwrap();
    assert_eq!(agent.session, new_session);
    assert!(agent.generator.history_cut);
    assert_eq!(agent.generator.preserve_history, Some(false));
    assert_eq!(
        h.app
            .generator_presentation(id)
            .activity
            .unwrap()
            .focus_index,
        Some(1)
    );
}
