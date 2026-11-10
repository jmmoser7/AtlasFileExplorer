use super::*;

fn artifact_wire(h: &mut Harness, source: NodeId, target: NodeId) {
    let mut wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: agent_inputs::OUTPUT_T,
        },
        ConnectorEnd::Anchored {
            node: target,
            side: Side::Left,
            t: agent_inputs::GENERATOR_PORTS[0].t,
        },
    );
    let NodeKind::Connector(connector) = &mut wire.kind else {
        panic!()
    };
    connector.binding = Some(agent_inputs::WireBinding {
        input_b: true,
        kind: InputKind::Artifact,
        output: h
            .app
            .doc()
            .scene
            .node(source)
            .and_then(artifact)
            .and_then(|binding| binding.revision.clone()),
        order: vec![],
        all_images: false,
        consumed: false,
        slot: Some("media".into()),
    });
    h.app.add_nodes(vec![wire]);
}

#[test]
fn self_followup_seeds_frozen_displayed_content_for_both_count_branches_independently_of_memory() {
    const CURRENT: &[u8] = b"<html><body>Current teal card: 42</body></html>";
    for preserve in [false, true] {
        let (mut h, image) = board("dashboard-self-followup-content");
        let id = spawn(&mut h, image);
        let memory = fixture_revision(&mut h, id, "older-memory", b"<html>Older content</html>");
        let current = fixture_revision(&mut h, id, "displayed-current", CURRENT);
        h.app.patch_nodes(&[id], |node| {
            let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
            agent.generator.preserve_history = Some(preserve);
            agent.generator.count = Some(2);
            artifact_mut(node).unwrap().active_memory = Some(agent_inputs::ArtifactMemoryPin {
                revision: memory.revision_id.clone(),
                through: 2,
            });
        });
        let question =
            "Change the dashboard number from 42 to 43. Keep the same layout and colors.";
        *h.app.agents.prompt_mut(id) = question.into();
        assert!(h.app.queue_artifact_generation(id));
        let first = h.app.agents.artifact.preparing[&id].plan.clone();
        let queued = &h.app.agents.artifact.queued[&id];
        assert_eq!(
            queued.len(),
            1,
            "Count 2 captures exactly two artifact jobs"
        );
        let second = queued[0].0.clone();
        assert_ne!(first.request.id, second.request.id);
        let guard = first.guard.clone();
        let mut prepared = Vec::new();
        for plan in [first, second] {
            assert_eq!(plan.request.prompt, question);
            assert_eq!(plan.parent.as_ref(), Some(&current));
            if preserve {
                assert_eq!(plan.memory.as_ref().unwrap().0, memory);
            } else {
                assert!(plan.memory.is_none());
            }
            let ready = super::super::generation::prepare(plan).unwrap();
            assert_eq!(
                ready.plan.request.history.len(),
                if preserve { 2 } else { 0 }
            );
            assert_eq!(
                std::fs::read(ready.run.staging_output.join("index.html")).unwrap(),
                CURRENT
            );
            prepared.push(ready);
        }
        assert_ne!(
            prepared[0].run.staging_output,
            prepared[1].run.staging_output
        );
        std::fs::write(
            prepared[0].run.staging_output.join("index.html"),
            b"<html>43</html>",
        )
        .unwrap();
        assert_eq!(
            std::fs::read(prepared[1].run.staging_output.join("index.html")).unwrap(),
            CURRENT
        );
        assert_eq!(std::fs::read(&current.materialized_entry).unwrap(), CURRENT);
        h.app.patch_nodes(&[id], |node| {
            artifact_mut(node).unwrap().revision = Some(memory.revision_id.clone())
        });
        assert_ne!(
            h.app.artifact_batch_guard(id).as_deref(),
            Some(guard.as_str()),
            "changing the displayed revision still invalidates stale queued work"
        );
        assert_eq!(
            prepared[0].run.parent.as_ref(),
            Some(&current),
            "captured content never follows later selection changes"
        );
    }
}

#[test]
fn explicit_artifact_content_wins_and_an_invalid_edge_is_not_hidden_by_self_fallback() {
    for valid in [false, true] {
        let (mut h, image) = board("dashboard-explicit-content-precedence");
        let id = spawn(&mut h, image);
        let own = fixture_revision(&mut h, id, "own-current", b"<html>Own 42</html>");
        let donor = spawn(&mut h, image);
        let donor_revision = valid
            .then(|| fixture_revision(&mut h, donor, "donor-current", b"<html>Explicit 77</html>"));
        h.app.patch_nodes(&[id], |node| {
            slate_doc::agent_chat::agent_mut(node)
                .unwrap()
                .generator
                .preserve_history = Some(false);
        });
        artifact_wire(&mut h, donor, id);
        *h.app.agents.prompt_mut(id) = "Revise the selected source".into();
        if valid {
            assert!(h.app.queue_artifact_generation(id));
            let plan = h.app.agents.artifact.preparing[&id].plan.clone();
            assert_eq!(plan.parent, donor_revision);
            assert_ne!(plan.parent.as_ref(), Some(&own));
            let ready = super::super::generation::prepare(plan).unwrap();
            assert_eq!(
                std::fs::read(ready.run.staging_output.join("index.html")).unwrap(),
                b"<html>Explicit 77</html>"
            );
        } else {
            assert!(!h.app.queue_artifact_generation(id));
            assert_eq!(
                h.app.agent_failure_reason(id),
                Some("The artifact has an invalid or ambiguous content/memory connection.")
            );
            assert!(h.app.agents.dispatched.is_empty());
        }
        assert_eq!(
            std::fs::read(&own.materialized_entry).unwrap(),
            b"<html>Own 42</html>"
        );
    }
}

#[test]
fn blank_first_generation_has_no_invented_self_parent() {
    let (mut h, image) = board("dashboard-blank-first-content");
    let id = spawn(&mut h, image);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(true);
    });
    *h.app.agents.prompt_mut(id) = "Create a new page".into();
    assert!(h.app.queue_artifact_generation(id));
    let plan = h.app.agents.artifact.preparing[&id].plan.clone();
    assert!(plan.parent.is_none());
    let ready = super::super::generation::prepare(plan).unwrap();
    assert!(!ready.run.staging_output.join("index.html").exists());
}
