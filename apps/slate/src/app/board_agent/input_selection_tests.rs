//! Selected text flows through the same asynchronous resolver as its display.
use super::super::*;

fn selected_note(tag: &str) -> (crate::app::tests::Harness, NodeId, NodeId) {
    let (mut h, target, source) =
        super::super::test_support::generator_with_note(tag, "Authored source text");
    h.app.tab_mut().path = Some(h.base.join("Board.slate"));
    let mut agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(target).unwrap())
        .unwrap()
        .clone();
    agent.view = atlas_agent::PortalView::Text;
    agent.session = "source-session".into();
    agent.bundle = Some(slate_doc::SourceUri {
        locator: "history/session.json".into(),
    });
    agent.generator.shown_text = Some(slate_doc::text_answers::TextOutputRef {
        review_id: None,
        source: agent.bundle.clone().unwrap(),
        response: slate_doc::text_answers::TextAnswerRef {
            session: agent.session.clone(),
            through: 2,
        },
    });
    h.app.patch_nodes(&[source], |node| {
        if let NodeKind::Text(text) = &mut node.kind {
            text.agent = Some(Box::new(agent.clone()));
        }
    });
    h.app.agents.sessions.insert(
        source,
        std::sync::Arc::new(AgentSession {
            turns: [
                ("user", "first"),
                ("assistant", "Selected old output"),
                ("user", "second"),
                ("assistant", "Newest output"),
            ]
            .into_iter()
            .map(|(role, text)| AgentTurn {
                role: role.into(),
                text: text.into(),
                at: 0,
            })
            .collect(),
            ..Default::default()
        }),
    );
    (h, target, source)
}

#[test]
fn selected_historical_text_overrides_authored_source_for_plain_and_grafted_inputs() {
    let (mut h, target, source) = selected_note("selected-text-input");
    assert_eq!(
        h.app.displayed_text_output(source).as_deref(),
        Some("Selected old output")
    );
    let plain = h.app.agent_input_snapshot(target).unwrap();
    assert_eq!(plain.wired[0].text, "Selected old output");
    h.app.patch_nodes(&[target], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .grafted_slots
            .insert("prompt".into());
    });
    let jobs = slate_doc::agent_inputs::plan_grafted_inputs(
        &h.app.agent_input_snapshot(target).unwrap(),
        &std::collections::BTreeSet::from(["prompt".into()]),
        1,
    )
    .unwrap();
    assert_eq!(jobs[0].inputs.wired[0].text, "Selected old output");
    assert_eq!(h.app.agent_reply(source).as_deref(), Some("Newest output"));
}

#[test]
fn missing_or_invalid_explicit_text_selection_errors_instead_of_using_latest() {
    let (mut h, target, source) = selected_note("missing-selected-input");
    h.app.patch_nodes(&[source], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .shown_text
            .as_mut()
            .unwrap()
            .response
            .through = 1
    });
    assert!(h
        .app
        .agent_input_snapshot(target)
        .unwrap_err()
        .contains("checkpoint"));
    h.app.agents.sessions.remove(&source);
    assert!(h
        .app
        .agent_input_snapshot(target)
        .unwrap_err()
        .contains("source"));
    h.app.patch_nodes(&[source], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .shown_text = None
    });
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Authored source text"
    );
}

#[test]
fn archived_selected_text_reloads_through_existing_source_cache() {
    let (mut h, target, source) = selected_note("archived-selected-input");
    let archived = h.app.agents.sessions.remove(&source).unwrap();
    let dir = h.base.join("history");
    std::fs::create_dir_all(&dir).unwrap();
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), archived.as_ref()).unwrap();
    h.app.patch_nodes(&[source], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.session = "fresh-session".into();
        agent.bundle = Some(slate_doc::SourceUri {
            locator: "fresh/session.json".into(),
        });
        *agent = serde_json::from_value(serde_json::to_value(&*agent).unwrap()).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(3);
    while h.app.agents.sources.snapshot(&dir).is_none() {
        h.app.agents.sources.poll(&dir, None);
        assert!(Instant::now() < deadline, "archived source did not load");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        h.app.agent_input_snapshot(target).unwrap().wired[0].text,
        "Selected old output"
    );
}

#[test]
fn gr4_cut_invalidates_pending_count_capture_without_removing_outputs_or_restarting_on_undo() {
    let (mut h, source) =
        crate::app::board_flow::tests::generator_with_output("gr4-pending-count-cut");
    h.app.set_agent_session_for_test(
        source,
        AgentSession {
            turns: vec![AgentTurn {
                role: "assistant".into(),
                text: "Severed context".into(),
                at: 1,
            }],
            ..Default::default()
        },
    );
    let kind = h.app.doc().scene.node(source).unwrap().kind.clone();
    let mut child = h
        .app
        .doc_mut()
        .scene
        .build_node(slate_doc::WorldRect::new(600.0, 0.0, 480.0, 320.0), kind);
    let agent = slate_doc::agent_chat::agent_mut(&mut child).unwrap();
    agent.session = slate_doc::scene::new_agent_session_id();
    agent.view = atlas_agent::PortalView::Text;
    agent.generator.count = Some(2);
    agent.generator.preserve_history = Some(true);
    let id = child.id;
    let memory = h.app.build_generator_memory_wire(source, &child).unwrap();
    h.app.add_nodes(vec![child, memory]);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let images = h.app.agent_images(id);
    let request = AgentRequest {
        id: "pending-old-context".into(),
        prompt: "Two alternatives".into(),
        ..Default::default()
    };
    let mut prep = h
        .app
        .begin_agent_input_preparation(request.inputs.clone())
        .unwrap();
    prep.bake_dependencies
        .push(crate::app::board_image_layers::LayerBakeReadiness::new());
    assert!(h.app.enqueue_prepared_generation_as(
        id,
        request,
        prep,
        slate_doc::scene::CmdAuthor::Human
    ));
    assert_eq!(h.app.agents.comfy_queue[&id].len(), 2);
    assert!(h.app.agents.comfy_queue[&id]
        .iter()
        .all(|job| job.request.history[0].text == "Severed context"));
    assert!(h.app.artifact_disconnect_memory(id));
    assert!(!h.app.agents.comfy_queue.contains_key(&id));
    assert_eq!(h.app.agent_images(id).as_ref(), images.as_ref());
    h.app.pump_comfy_queue();
    assert!(h.app.agents.dispatched.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    h.app.pump_comfy_queue();
    assert!(h.app.agents.dispatched.is_empty());
    assert!(!h.app.agents.comfy_queue.contains_key(&id));
    for view in [
        atlas_agent::PortalView::Images,
        atlas_agent::PortalView::Text,
    ] {
        h.app.patch_nodes(&[id], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.view = view;
            agent.live = true;
        });
        let active_before = h.app.doc().scene.node(id).unwrap().clone();
        let request = format!("active-delayed-{view:?}");
        h.app.agents.requests.insert(id, request.clone());
        h.app.agents.awaiting.insert(
            id,
            AgentAwait::Sent {
                at: Instant::now(),
                req_at: 1,
            },
        );
        let memory = h.app.doc().scene.nodes.iter().find(|node| matches!(&node.kind,
            NodeKind::Connector(conn) if slate_doc::agent_inputs::generator_memory_target(&h.app.doc().scene, conn) == Some(id))).unwrap().id;
        let extra = super::super::test_support::note(
            &mut h,
            Pos2::new(900.0, 600.0),
            "ordinary selected node",
        );
        h.app.board_sel = [memory, extra].into_iter().collect();
        let depth = h.app.tab().journal.undo_depth();
        assert!(h
            .app
            .dispatch(&h.ctx, atlas_commands::CommandId("board.delete"), None));
        assert!(h.app.doc().scene.node(memory).is_none());
        assert!(h.app.doc().scene.node(extra).is_none());
        assert_eq!(
            h.app.tab().journal.undo_depth(),
            depth + 1,
            "mixed Delete must remain one undo"
        );
        assert_eq!(h.app.agents.cancelled_generations.get(&id), Some(&request));
        assert!(!h.app.agents.requests.contains_key(&id));
        assert!(
            !slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .live
        );
        let delayed = AgentSession {
            request: request.clone(),
            turns: vec![AgentTurn {
                role: "assistant".into(),
                text: "Canceled late response".into(),
                at: 2,
            }],
            ..Default::default()
        };
        assert!(!h.app.generator_session_is_current(id, &delayed));
        assert_eq!(h.app.agent_images(id).as_ref(), images.as_ref());
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &active_before);
        assert!(h.app.doc().scene.node(memory).is_some());
        assert!(h.app.doc().scene.node(extra).is_some());
        assert!(!h.app.generator_session_is_current(id, &delayed));
        h.app.pump_comfy_queue();
        assert!(h.app.agents.dispatched.is_empty());
    }
}

#[test]
fn di17_cut_does_not_cancel_a_shared_runtime_siblings_current_request() {
    let (mut h, source) =
        crate::app::board_flow::tests::generator_with_output("di17-shared-runtime");
    let kind = h.app.doc().scene.node(source).unwrap().kind.clone();
    let mut child = h
        .app
        .doc_mut()
        .scene
        .build_node(slate_doc::WorldRect::new(600.0, 0.0, 480.0, 320.0), kind);
    let id = child.id;
    slate_doc::agent_chat::agent_mut(&mut child)
        .unwrap()
        .generator
        .preserve_history = Some(true);
    let memory = h.app.build_generator_memory_wire(source, &child).unwrap();
    h.app.add_nodes(vec![child, memory]);
    for (node, request) in [(source, "sibling-current"), (id, "target-old")] {
        h.app.agents.requests.insert(node, request.into());
        h.app.agents.awaiting.insert(
            node,
            AgentAwait::Sent {
                at: Instant::now(),
                req_at: 1,
            },
        );
    }
    assert!(h.app.artifact_disconnect_memory(id));
    assert_eq!(
        h.app.agents.requests.get(&source).map(String::as_str),
        Some("sibling-current")
    );
    assert!(h.app.agent_generation_dispatched(source));
    let sibling = AgentSession {
        request: "sibling-current".into(),
        ..Default::default()
    };
    let target = AgentSession {
        request: "target-old".into(),
        ..Default::default()
    };
    assert!(h.app.generator_session_is_current(source, &sibling));
    assert!(!h.app.generator_session_is_current(id, &target));
    h.app.board_undo();
    assert!(!h.app.generator_session_is_current(id, &target));
    assert!(!h.app.generator_session_is_current(id, &sibling));
    assert!(h.app.generator_session_is_current(source, &sibling));
}

#[test]
fn di17_cut_revokes_headless_ticket_before_delayed_model_completion_can_publish() {
    use crate::app::application_connector::native::{
        test_support, Lease, NativeOperation, NativeOutcome, NativeResult,
    };
    let (mut h, source) = crate::app::board_flow::tests::generator_with_output("di17-headless-cut");
    let (id, peer) = test_support::install(&mut h, atlas_connector::SourceMode::IsolatedDraft);
    test_support::set_surface_descriptor(
        &mut h,
        id,
        atlas_connector::SurfaceDescriptor {
            surface_id: "headless-cut-fixture".into(),
            mode: atlas_connector::SurfaceMode::Headless,
            process_ownership: atlas_connector::ProcessOwnership::LaunchedDedicated,
            native_handle: None,
            can_detach: true,
        },
    );
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(source).unwrap())
        .unwrap()
        .clone();
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.agent = Some(Box::new(agent.clone()));
        }
    });
    let target = h.app.doc().scene.node(id).unwrap().clone();
    let memory = h.app.build_generator_memory_wire(source, &target).unwrap();
    h.app.add_nodes(vec![memory]);
    let binding = h.app.native_binding(id).unwrap();
    let lease = Lease::new();
    let ticket = h
        .app
        .native_submit_control(id, &binding, NativeOperation::Inspect, lease.clone())
        .unwrap_or_else(|_| panic!("fixture request refused"));
    let request = peer.next().unwrap();
    assert!(request.lease.claim());
    assert!(h.app.artifact_disconnect_memory(id));
    let cut = h.app.doc().scene.node(id).unwrap().clone();
    assert!(!lease.live());
    peer.finish_admitted(
        request,
        NativeOutcome::Completed {
            result: NativeResult::Model {
                path: h.base.join("late-canceled.3dm"),
                size: 1,
                mtime: 0,
                report: serde_json::json!({}),
            },
        },
        "late-revision",
    );
    let completion = h.app.native_poll(id, &ticket);
    assert!(!lease.publish());
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &cut);
    drop(completion); // Existing native caller retains/abandons its receipt.
}
