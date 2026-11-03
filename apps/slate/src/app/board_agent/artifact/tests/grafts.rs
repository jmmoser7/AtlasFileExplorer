//! Synthetic lifecycle regressions, independent of live provider availability.
use super::*;

#[test]
fn semantic_guard_ignores_moves_camera_and_output_selection() {
    let (mut h, image) = board("artifact-graft-visual-state");
    let id = spawn(&mut h, image);
    *h.app.agents.prompt_mut(id) = "Keep the submitted request".into();
    let guard = h.app.artifact_batch_guard(id);
    h.app.patch_nodes(&[image, id], |node| {
        node.rect.x += 100.0;
        node.rect.y += 40.0;
    });
    h.app.tab_mut().cam.z = 0.5;
    h.app.board_sel.clear();
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .shown_text = Some(slate_doc::text_answers::TextOutputRef {
            review_id: None,
            source: SourceUri {
                locator: "old/session.json".into(),
            },
            response: slate_doc::text_answers::TextAnswerRef {
                session: "archived".into(),
                through: 2,
            },
        });
    });
    assert_eq!(h.app.artifact_batch_guard(id), guard);
}

#[test]
fn editing_the_prompt_aborts_the_remaining_batch() {
    let (mut h, image) = board("artifact-graft-prompt-change");
    let id = spawn(&mut h, image);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .count = Some(2)
    });
    *h.app.agents.prompt_mut(id) = "Original prompt".into();
    assert!(h.app.queue_artifact_generation(id));
    h.app.agents.artifact.preparing.remove(&id);
    *h.app.agents.prompt_mut(id) = "Changed prompt".into();
    h.app.pump_artifact_generations(&h.ctx);
    assert!(!h.app.agents.artifact.queued.contains_key(&id));
    assert!(!h.app.agents.artifact.preparing.contains_key(&id));
}

#[test]
fn failed_branch_retains_the_last_output_and_allows_the_next_snapshot() {
    let (mut h, image) = board("artifact-graft-partial-failure");
    let id = spawn(&mut h, image);
    fixture_revision(&mut h, id, "prior-output", b"<html>Prior output</html>");
    let previous = artifact(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .revision
        .clone();
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .count = Some(2)
    });
    *h.app.agents.prompt_mut(id) = "Create independent alternatives".into();
    assert!(h.app.queue_artifact_generation(id));
    let failed = h.app.agents.artifact.preparing.remove(&id).unwrap();
    h.app
        .fail_agent_await(id, "Synthetic first-branch failure".into());
    // Keep the native worker pool occupied so this is a pure scheduling test.
    h.app.agents.artifact.active_workers = 2;
    h.app.pump_artifact_generations(&h.ctx);
    let next = &h.app.agents.artifact.preparing[&id];
    assert_ne!(next.plan.request.id, failed.plan.request.id);
    assert_eq!(next.plan.request.inputs, failed.plan.request.inputs);
    assert_eq!(
        artifact(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .revision,
        previous
    );
}

#[test]
fn count_queues_independent_plans_and_stop_discards_only_pending_work() {
    let (mut h, image) = board("artifact-graft-count-stop");
    let id = spawn(&mut h, image);
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.generator.count = Some(3);
        agent.generator.preserve_history = Some(true);
    });
    *h.app.agents.prompt_mut(id) = "Build three alternatives".into();
    assert!(h.app.queue_artifact_generation(id));
    let first = &h.app.agents.artifact.preparing[&id].plan;
    let queued = &h.app.agents.artifact.queued[&id];
    assert_eq!(queued.len(), 2);
    assert_ne!(first.session, queued[0].0.session);
    assert_eq!(first.request.inputs, queued[0].0.request.inputs);
    assert!(!first.resume && !queued[0].0.resume);
    assert!(h.app.cancel_artifact_generation(id));
    assert!(!h.app.agents.artifact.queued.contains_key(&id));
    assert!(h.app.doc().scene.node(id).is_some());
}

#[test]
fn an_intervening_source_edit_aborts_the_remaining_batch() {
    let (mut h, image) = board("artifact-graft-source-change");
    let id = spawn(&mut h, image);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .count = Some(2)
    });
    *h.app.agents.prompt_mut(id) = "Build alternatives".into();
    assert!(h.app.queue_artifact_generation(id));
    h.app.agents.artifact.preparing.remove(&id); // Simulate a failed first branch.
    h.app.patch_nodes(&[id], |node| {
        if let NodeKind::Portal(portal) = &mut node.kind {
            portal.source = Some(SourceUri {
                locator: "changed.html".into(),
            });
        }
    });
    h.app.pump_artifact_generations(&h.ctx);
    assert!(!h.app.agents.artifact.queued.contains_key(&id));
    assert!(!h.app.agents.artifact.preparing.contains_key(&id));
}

#[test]
fn terminal_cut_archives_context_reenable_stays_fresh_and_undo_restores_wire() {
    let (mut h, image) = board("artifact-cut-archive");
    let first = spawn(&mut h, image);
    fixture_revision(&mut h, first, "r1", b"<html>retained</html>");
    let child = spawn(&mut h, first);
    let before = h.app.doc().scene.node(child).unwrap().clone();
    let content = agent_inputs::artifact_content_parent(&h.app.doc().scene, child);
    assert!(h.app.artifact_disconnect_memory(child));
    let cut = h.app.doc().scene.node(child).unwrap();
    let agent = slate_doc::agent_chat::agent(cut).unwrap();
    assert!(agent.generator.history_cut);
    let session_path = h
        .app
        .agent_link_dir(child, &h.base)
        .unwrap()
        .join("session.json");
    let session: atlas_agent::AgentSession =
        serde_json::from_slice(&std::fs::read(session_path).unwrap()).unwrap();
    assert!(
        session.turns.is_empty() && session.conversation.is_empty() && session.approval.is_none()
    );
    assert_eq!(
        agent_inputs::artifact_content_parent(&h.app.doc().scene, child),
        content
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(child).unwrap(), &before);
    assert!(agent_inputs::artifact_memory_source(&h.app.doc().scene, child).is_some());
    assert!(h.app.artifact_disconnect_memory(child));
    h.app.patch_nodes(&[child], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(true)
    });
    *h.app.agents.prompt_mut(child) = "A fresh turn".into();
    assert!(h.app.queue_artifact_generation(child));
    let plan = &h.app.agents.artifact.preparing[&child].plan;
    assert!(plan.memory.is_none() && !plan.resume && plan.request.history.is_empty());
}
