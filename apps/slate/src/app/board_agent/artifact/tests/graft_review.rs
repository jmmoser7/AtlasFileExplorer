//! Capability review regressions. Provider output is explicitly synthetic.
use super::*;

fn start_prepared_fixture(h: &mut Harness, id: NodeId) -> Plan {
    let mut pending = h
        .app
        .agents
        .artifact
        .preparing
        .remove(&id)
        .or_else(|| {
            h.app
                .agents
                .artifact
                .queued
                .get_mut(&id)?
                .pop_front()
                .map(|(plan, inputs)| Preparing {
                    plan,
                    inputs,
                    started: false,
                })
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !h
        .app
        .poll_agent_input_preparation(&mut pending.inputs)
        .unwrap()
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    pending.plan.request.inputs = pending.inputs.inputs.clone();
    let prepared = super::super::generation::prepare(pending.plan.clone()).unwrap();
    let plan = prepared.plan.clone();
    pending.plan = plan.clone();
    pending.started = true;
    h.app.agents.artifact.preparing.insert(id, pending);
    h.app
        .artifact_work_tx()
        .send(WorkDone::Prepared(
            plan.tab,
            id,
            plan.request.id.clone(),
            Box::new(Ok(prepared)),
        ))
        .unwrap();
    h.app.pump_artifact_generations(&h.ctx);
    assert!(
        h.app.agents.artifact.running.contains_key(&id),
        "prepared fixture must start"
    );
    plan
}

fn publish_fixture(h: &mut Harness, id: NodeId) -> ArtifactRevisionRef {
    let running = &h.app.agents.artifact.running[&id];
    let plan = running.prepared.plan.clone();
    let run = running.prepared.run.clone();
    std::fs::write(
        run.staging_output.join("index.html"),
        b"<html>Synthetic generated output</html>",
    )
    .unwrap();
    let provenance = ArtifactProvenance {
        prompt: plan.request.prompt.clone(),
        provider: plan.provider.clone(),
        model: "fixture".into(),
        checkpoint: ArtifactCheckpoint {
            conversation: plan.session.clone(),
            through: 2,
            turns: vec![
                AgentTurn {
                    role: "user".into(),
                    text: plan.request.prompt.clone(),
                    at: 1,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: "Synthetic completed output".into(),
                    at: 2,
                },
            ],
        },
        inputs: running.prepared.input_provenance.clone(),
    };
    h.app.agents.awaiting.remove(&id);
    h.app.set_agent_session_for_test(
        id,
        AgentSession {
            status: AgentStatus::Idle,
            request: plan.request.id.clone(),
            turns: provenance.checkpoint.turns.clone(),
            ..Default::default()
        },
    );
    let (reference, revision) =
        atlas_ai::versions::publish_artifact_revision(&run, provenance).unwrap();
    h.app
        .artifact_work_tx()
        .send(WorkDone::Published(
            plan.tab,
            id,
            plan.request.id.clone(),
            Ok(Pending {
                tab: plan.tab,
                previous: plan.previous,
                reference: reference.clone(),
                revision,
            }),
        ))
        .unwrap();
    h.app.pump_artifact_generations(&h.ctx);
    reference
}

fn wire_prompt(h: &mut Harness, target: NodeId, text: &str) {
    let source = crate::app::board_agent::test_support::note(h, Pos2::new(-300.0, 0.0), text);
    let port = agent_inputs::input_ports(&h.app.doc().scene, target)
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
            node: target,
            side: Side::Left,
            t: port,
        },
    );
    h.app.add_nodes(vec![wire]);
}

#[test]
fn gr1_wired_only_grafted_count_survives_prepare_start_and_publish_without_authoring_prompt() {
    let (mut h, image) = board("gr1-wired-only-count-graft");
    let id = spawn(&mut h, image);
    wire_prompt(&mut h, id, "Prompt A");
    wire_prompt(&mut h, id, "Prompt B");
    h.app.agents.prompts.remove(&id);
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.instruction.clear();
        agent.generator.count = Some(2);
        agent.generator.grafted_slots.insert("prompt".into());
    });
    assert!(h.app.queue_artifact_generation(id));
    for expected in ["Prompt A", "Prompt A", "Prompt B", "Prompt B"] {
        let plan = start_prepared_fixture(&mut h, id);
        assert_eq!(plan.request.prompt, expected);
        assert_eq!(plan.authored_prompt, "");
        assert_eq!(
            slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .instruction,
            ""
        );
        let result = publish_fixture(&mut h, id);
        assert_eq!(
            artifact(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .revision
                .as_deref(),
            Some(result.revision_id.as_str())
        );
    }
}

#[test]
fn gr1_actual_running_prompt_edit_rejects_publication() {
    let (mut h, image) = board("gr1-running-prompt-edit");
    let id = spawn(&mut h, image);
    wire_prompt(&mut h, id, "Original wired instruction");
    h.app.agents.prompts.remove(&id);
    assert!(h.app.queue_artifact_generation(id));
    start_prepared_fixture(&mut h, id);
    *h.app.agents.prompt_mut(id) = "A later authored instruction".into();
    publish_fixture(&mut h, id);
    assert!(artifact(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .revision
        .is_none());
    assert!(!h.app.agents.artifact.running.contains_key(&id));
}

fn pick_fixture(h: &mut Harness, id: NodeId, reference: &ArtifactRevisionRef) {
    let previous = artifact(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .revision
        .clone();
    let revision = atlas_ai::versions::load_artifact_revision(reference).unwrap();
    let tab = h.app.tab().id;
    h.app.agents.artifact.pending.insert(
        id,
        Pending {
            tab,
            previous,
            reference: reference.clone(),
            revision,
        },
    );
    assert!(h.app.accept_artifact_output(id));
}

#[test]
fn gr3_browsing_old_content_never_reactivates_cut_memory_and_fresh_memory_stays_separate() {
    let (mut h, image) = board("gr3-browse-cut-memory");
    let parent = spawn(&mut h, image);
    fixture_revision(&mut h, parent, "parent-r1", b"<html>parent</html>");
    let id = spawn(&mut h, parent);
    let old = fixture_revision(&mut h, id, "precut-r1", b"<html>precut</html>");
    assert!(h.app.artifact_disconnect_memory(id));
    pick_fixture(&mut h, id, &old);
    assert!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .generator
            .history_cut
    );
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(true)
    });
    *h.app.agents.prompt_mut(id) = "A genuinely fresh request".into();
    assert!(h.app.queue_artifact_generation(id));
    assert!(h.app.agents.artifact.preparing[&id].plan.memory.is_none());
    start_prepared_fixture(&mut h, id);
    let fresh = publish_fixture(&mut h, id);
    let fresh_node = h.app.doc().scene.node(id).unwrap().clone();
    pick_fixture(&mut h, id, &old);
    let selected_old = h.app.doc().scene.node(id).unwrap().clone();
    let memory = artifact(&selected_old)
        .unwrap()
        .active_memory
        .as_ref()
        .unwrap();
    assert_eq!(memory.revision, fresh.revision_id);
    assert_eq!(
        artifact(&selected_old).unwrap().revision.as_deref(),
        Some(old.revision_id.as_str())
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &fresh_node);
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &selected_old);
    *h.app.agents.prompt_mut(id) = "Continue fresh history".into();
    assert!(h.app.queue_artifact_generation(id));
    assert_eq!(
        h.app.agents.artifact.preparing[&id]
            .plan
            .memory
            .as_ref()
            .unwrap()
            .0
            .revision_id,
        fresh.revision_id
    );
}

#[test]
fn gr4_cut_invalidates_artifact_preparation_and_ignores_its_late_completion() {
    let (mut h, image) = board("gr4-late-artifact-preparation");
    let parent = spawn(&mut h, image);
    fixture_revision(&mut h, parent, "parent", b"<html>parent</html>");
    let id = spawn(&mut h, parent);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .count = Some(2)
    });
    let prepared = prepare_fixture_run(&mut h, id);
    let plan = prepared.plan.clone();
    let inputs = h
        .app
        .begin_agent_input_preparation(Default::default())
        .unwrap();
    h.app.agents.artifact.preparing.insert(
        id,
        Preparing {
            plan: plan.clone(),
            inputs,
            started: true,
        },
    );
    assert!(h.app.artifact_disconnect_memory(id));
    assert!(!h.app.agents.artifact.preparing.contains_key(&id));
    assert!(!h.app.agents.artifact.queued.contains_key(&id));
    let cut = h.app.doc().scene.node(id).unwrap().clone();
    h.app
        .artifact_work_tx()
        .send(WorkDone::Prepared(
            plan.tab,
            id,
            plan.request.id,
            Box::new(Ok(prepared)),
        ))
        .unwrap();
    h.app.pump_artifact_generations(&h.ctx);
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &cut);
    assert!(!h.app.artifact_is_busy(id));
    assert!(h.app.agents.dispatched.is_empty());
}

#[test]
fn di17_active_artifact_cut_cancels_count_queue_and_rejects_delayed_publication() {
    let (mut h, image) = board("di17-active-artifact-cut");
    let parent = spawn(&mut h, image);
    fixture_revision(&mut h, parent, "source-r1", b"<html>source</html>");
    let id = spawn(&mut h, parent);
    let accepted = fixture_revision(&mut h, id, "accepted-r1", b"<html>retained</html>");
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .count = Some(2)
    });
    *h.app.agents.prompt_mut(id) = "Delayed alternatives".into();
    assert!(h.app.queue_artifact_generation(id));
    let plan = start_prepared_fixture(&mut h, id);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    assert!(h.app.artifact_disconnect_memory(id));
    let cut = h.app.doc().scene.node(id).unwrap().clone();
    assert!(!h.app.agents.artifact.running.contains_key(&id));
    assert!(!h.app.agents.artifact.queued.contains_key(&id));
    assert_eq!(
        artifact(&cut).unwrap().revision.as_deref(),
        Some(accepted.revision_id.as_str())
    );
    assert_eq!(
        h.app.agents.cancelled_generations.get(&id),
        Some(&plan.request.id)
    );
    let revision = atlas_ai::versions::load_artifact_revision(&accepted).unwrap();
    h.app
        .artifact_work_tx()
        .send(WorkDone::Published(
            plan.tab,
            id,
            plan.request.id,
            Ok(Pending {
                tab: plan.tab,
                previous: plan.previous,
                reference: accepted,
                revision,
            }),
        ))
        .unwrap();
    h.app.pump_artifact_generations(&h.ctx);
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &cut);
    assert!(artifact(&cut).unwrap().active_memory.is_none());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    assert!(!h.app.artifact_is_busy(id));
}

#[test]
fn di17_delete_refuses_interior_history_but_deleting_its_target_keeps_normal_semantics() {
    let (mut h, image) = board("di17-delete-interior-target");
    let parent = spawn(&mut h, image);
    fixture_revision(&mut h, parent, "parent", b"<html>parent</html>");
    let middle = spawn(&mut h, parent);
    fixture_revision(&mut h, middle, "middle", b"<html>middle</html>");
    let _last = spawn(&mut h, middle);
    let memory = h.app.doc().scene.nodes.iter().find(|node| matches!(&node.kind,
        NodeKind::Connector(conn) if agent_inputs::generator_memory_target(&h.app.doc().scene, conn) == Some(middle))).unwrap().id;
    let before = h.app.doc().scene.node(middle).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    h.app.board_sel = [memory].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.delete"), None));
    assert!(h.app.doc().scene.node(memory).is_some());
    assert_eq!(h.app.doc().scene.node(middle).unwrap(), &before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.board_sel = [middle, memory].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.delete"), None));
    assert!(h.app.doc().scene.node(middle).is_none());
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(middle).unwrap(), &before);
    assert!(h.app.doc().scene.node(memory).is_some());
}

#[test]
fn di17_delete_refuses_to_cut_a_locked_surviving_generator_before_canceling_work() {
    let (mut h, image) = board("di17-delete-locked-target");
    let parent = spawn(&mut h, image);
    fixture_revision(&mut h, parent, "parent", b"<html>parent</html>");
    let id = spawn(&mut h, parent);
    let memory = h.app.doc().scene.nodes.iter().find(|node| matches!(&node.kind,
        NodeKind::Connector(conn) if agent_inputs::generator_memory_target(&h.app.doc().scene, conn) == Some(id))).unwrap().id;
    h.app.patch_nodes(&[id], |node| node.locked = true);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    h.app
        .agents
        .requests
        .insert(id, "locked-active-request".into());
    h.app.agents.awaiting.insert(
        id,
        AgentAwait::Sent {
            at: Instant::now(),
            req_at: 1,
        },
    );
    let depth = h.app.tab().journal.undo_depth();
    assert!(!h.app.doc().scene.node(memory).unwrap().locked);
    h.app.board_sel = [memory].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.delete"), None));
    assert!(h.app.doc().scene.node(memory).is_some());
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(
        h.app.agents.requests.get(&id).map(String::as_str),
        Some("locked-active-request")
    );
    assert!(h.app.agent_generation_dispatched(id));
    assert!(!h.app.agents.cancelled_generations.contains_key(&id));
    assert!(h.app.toasts.last().unwrap().0.contains("Unlock"));
    // Explicit whole-target deletion continues through the unchanged deletion
    // planner rather than the surviving-target cut preparation owner.
    h.app.board_sel = [id, memory].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.delete"), None));
    assert!(h.app.doc().scene.node(id).is_none());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
}
