use super::*;

#[test]
fn memory_is_bounded_as_a_train_rail_and_ordinary_delete_keeps_content() {
    let (mut h, image) = board("artifact-memory-bounds-delete");
    let first = spawn(&mut h, image);
    fixture_revision(&mut h, first, "r1", b"<html><body>chart</body></html>");
    let child = spawn(&mut h, first);
    let memory = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|node| {
            matches!(&node.kind, NodeKind::Connector(conn) if conn.binding.as_ref()
            .is_some_and(|binding| binding.kind == InputKind::Memory))
        })
        .unwrap()
        .clone();
    let from = h.app.doc().scene.node(first).unwrap().rect;
    let to = h.app.doc().scene.node(child).unwrap().rect;
    assert_eq!(memory.rect, slate_doc::agent_chat::rail(from, to).aabb());
    let NodeKind::Connector(conn) = &memory.kind else {
        unreachable!()
    };
    assert_eq!(conn.color, Some(h.app.chat_wire_color()));
    let content = agent_inputs::artifact_content_parent(&h.app.doc().scene, child);
    h.app.delete_board_nodes(&[memory.id]);
    assert!(h.app.doc().scene.node(memory.id).is_none());
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        content
    );
    h.app.board_undo();
    assert!(h.app.doc().scene.node(memory.id).is_some());
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        content
    );
}

fn source_session(h: &mut Harness, source: NodeId, turns: Vec<AgentTurn>) {
    let mut session: AgentSession = serde_json::from_value(serde_json::json!({
        "provider":"codex", "status":"idle", "conversation":"fixture-conversation"
    }))
    .unwrap();
    session.turns = turns;
    let dir = h.app.agent_link_dir(source, &h.base).unwrap();
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &session).unwrap();
    h.app.agents.sessions.insert(source, session.into());
}

#[test]
fn mainline_memory_reuses_the_session_at_its_exact_visible_checkpoint() {
    let (mut h, image) = board("artifact-mainline-memory");
    let first = spawn(&mut h, image);
    let reference = fixture_revision(&mut h, first, "fixture-r1", b"<html><body>r1</body></html>");
    let revision = atlas_ai::versions::load_artifact_revision(&reference).unwrap();
    source_session(&mut h, first, revision.provenance.checkpoint.turns);
    let source_session = h.app.agent_session_for(first).unwrap().0;
    let child = spawn(&mut h, first);
    let prepared = prepare_fixture_run(&mut h, child);
    assert!(prepared.plan.resume);
    assert_eq!(prepared.plan.session, source_session);
    assert!(
        prepared.plan.request.history.is_empty(),
        "the live session already owns this prefix"
    );
    assert_ne!(prepared.run.source_root, reference.source_root);
    assert_ne!(
        prepared.run.family_id,
        artifact(h.app.doc().scene.node(first).unwrap())
            .unwrap()
            .family
    );
    assert_eq!(prepared.run.parent, Some(reference));
}

#[test]
fn an_earlier_branch_replays_only_its_pinned_prefix_and_exact_source() {
    let (mut h, image) = board("artifact-earlier-checkpoint");
    let first = spawn(&mut h, image);
    let r1 = b"<html><body>exact earlier chart</body></html>";
    let reference = fixture_revision(&mut h, first, "fixture-r1", r1);
    let checkpoint = atlas_ai::versions::load_artifact_revision(&reference)
        .unwrap()
        .provenance
        .checkpoint;
    let child = spawn(&mut h, first);
    fixture_revision(
        &mut h,
        first,
        "fixture-r2",
        b"<html><body>later source must not enter earlier fork</body></html>",
    );
    let mut later = checkpoint.turns.clone();
    later.push(AgentTurn {
        role: "user".into(),
        text: "future-feedback-sentinel".into(),
        at: 3,
    });
    later.push(AgentTurn {
        role: "assistant".into(),
        text: "future-reply-sentinel".into(),
        at: 4,
    });
    source_session(&mut h, first, later);
    let original_session = h.app.agent_session_for(first).unwrap().0;
    let prepared = prepare_fixture_run(&mut h, child);
    assert!(!prepared.plan.resume);
    assert_ne!(prepared.plan.session, original_session);
    assert_eq!(prepared.plan.request.history, checkpoint.turns);
    assert_eq!(prepared.plan.request.history.len(), 2);
    assert!(!prepared
        .plan
        .request
        .input_text()
        .contains("future-feedback-sentinel"));
    assert_eq!(
        std::fs::read(prepared.run.staging_output.join("index.html")).unwrap(),
        r1
    );
    assert_eq!(prepared.run.parent, Some(reference.clone()));
    assert_ne!(prepared.run.source_root, reference.source_root);
    assert_eq!(std::fs::read(reference.materialized_entry).unwrap(), r1);
}

#[test]
fn cancellation_and_stale_or_read_only_apply_leave_the_accepted_revision() {
    let (mut h, image) = board("artifact-stale-apply");
    let node = spawn(&mut h, image);
    *h.app.agents.prompt_mut(node) = "Make this interactive".into();
    assert!(h.app.queue_artifact_generation(node));
    assert!(h.app.cancel_artifact_generation(node));
    assert!(!h.app.artifact_is_busy(node));
    assert!(h.app.agents.dispatched.is_empty());
    let r1 = fixture_revision(
        &mut h,
        node,
        "fixture-r1",
        b"<html><body>first</body></html>",
    );
    fixture_revision(
        &mut h,
        node,
        "fixture-r2",
        b"<html><body>accepted second</body></html>",
    );
    let revision = atlas_ai::versions::load_artifact_revision(&r1).unwrap();
    let accepted = h.app.doc().scene.node(node).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let tab = h.app.tab().id;
    h.app.agents.artifact.pending.insert(
        node,
        Pending {
            tab,
            previous: Some("fixture-r1".into()),
            reference: r1.clone(),
            revision: revision.clone(),
        },
    );
    assert!(!h.app.accept_artifact_output(node));
    assert_eq!(h.app.doc().scene.node(node).unwrap(), &accepted);
    h.app.agents.artifact.pending.insert(
        node,
        Pending {
            tab,
            previous: Some("fixture-r2".into()),
            reference: r1,
            revision,
        },
    );
    h.app.tab_mut().read_only = true;
    assert!(!h.app.accept_artifact_output(node));
    assert_eq!(h.app.doc().scene.node(node).unwrap(), &accepted);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn an_explicit_child_harness_and_model_take_precedence_over_its_parent() {
    let (mut h, image) = board("artifact-explicit-child-harness");
    let first = spawn(&mut h, image);
    fixture_revision(
        &mut h,
        first,
        "fixture-r1",
        b"<html><body>parent</body></html>",
    );
    h.app.patch_nodes(&[first], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().model = Some("parent-codex-model".into());
    });
    h.app.board_sel = [first].into_iter().collect();
    assert!(h.app.dispatch(&h.ctx, atlas_commands::CommandId("portal.agent.spawn"), Some(
        serde_json::json!({"source":first.0,"kind":"artifact","provider":"cursor","model":"explicit-cursor-model"}).to_string()
    )));
    let child = *h.app.board_sel.iter().next().unwrap();
    let agent = slate_doc::agent_chat::agent(h.app.doc().scene.node(child).unwrap()).unwrap();
    assert_eq!(agent.provider, "cursor");
    assert_eq!(agent.model.as_deref(), Some("explicit-cursor-model"));
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        Some((first, "fixture-r1".into()))
    );
    assert_eq!(
        agent_inputs::artifact_memory_parent(&h.app.doc().scene, child),
        Some((first, 2))
    );
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn a_late_preparation_cannot_dispatch_or_rebind_a_changed_harness_or_model() {
    for changed in ["provider", "model"] {
        let (mut h, image) = board(&format!("artifact-late-{changed}"));
        let node = spawn(&mut h, image);
        fixture_revision(
            &mut h,
            node,
            "fixture-r1",
            b"<html><body>accepted</body></html>",
        );
        h.app.patch_nodes(&[node], |node| {
            slate_doc::agent_chat::agent_mut(node).unwrap().model = Some("captured-model".into());
        });
        *h.app.agents.prompt_mut(node) = "Change this chart".into();
        assert!(h.app.queue_artifact_generation(node));
        let mut pending = h.app.agents.artifact.preparing.remove(&node).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !h
            .app
            .poll_agent_input_preparation(&mut pending.inputs)
            .unwrap()
        {
            assert!(Instant::now() < until, "input preparation timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
        pending.plan.request.inputs = pending.inputs.inputs.clone();
        let prepared = super::super::generation::prepare(pending.plan.clone()).unwrap();
        let tab = pending.plan.tab;
        let request = pending.plan.request.id.clone();
        pending.started = true;
        h.app.agents.artifact.preparing.insert(node, pending);
        h.app.patch_nodes(&[node], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            if changed == "provider" {
                agent.provider = "cursor".into();
            } else {
                agent.model = Some("newly-selected-model".into());
            }
        });
        let expected = h.app.doc().scene.node(node).unwrap().clone();
        let expected_session = h.app.agent_session_for(node).unwrap();
        let depth = h.app.tab().journal.undo_depth();
        let tx = h.app.artifact_work_tx();
        h.app.agents.artifact.active_workers = 1;
        assert!(tx
            .send(WorkDone::Prepared(
                tab,
                node,
                request,
                Box::new(Ok(prepared))
            ))
            .is_ok());
        h.app.pump_artifact_generations(&h.ctx);
        assert!(
            h.app.agents.dispatched.is_empty(),
            "{changed} mismatch must not run"
        );
        assert_eq!(h.app.doc().scene.node(node).unwrap(), &expected);
        assert_eq!(h.app.agent_session_for(node).unwrap(), expected_session);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(!h.app.artifact_is_busy(node));
    }
}
