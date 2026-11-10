use super::graft_review::{publish_fixture, start_prepared_fixture};
use super::*;
const QUESTION: &str = "Change 42 to 43 without changing the layout";

fn count_fixture(name: &str, preserve: bool) -> (Harness, NodeId, NodeId, ArtifactRevisionRef) {
    let (mut h, image) = board(name);
    let id = spawn(&mut h, image);
    let parent = fixture_revision(&mut h, id, "accepted-parent", b"<html>42</html>");
    let other = spawn(&mut h, image);
    h.app.patch_nodes(&[other], |node| {
        slate_doc::agent_chat::agent_mut(node).unwrap().instruction = "OTHER HOST DRAFT".into();
    });
    h.app.sync_instruction(other, "OTHER HOST DRAFT");
    h.app.patch_nodes(&[id], |node| {
        let agent = slate_doc::agent_chat::agent_mut(node).unwrap();
        agent.generator.preserve_history = Some(preserve);
        agent.generator.count = Some(2);
    });
    *h.app.agents.prompt_mut(id) = QUESTION.into();
    assert!(h.app.queue_artifact_generation(id));
    let first = start_prepared_fixture(&mut h, id);
    assert_eq!(first.request.prompt, QUESTION);
    assert_eq!(first.parent.as_ref(), Some(&parent));
    (h, id, other, parent)
}
fn stage_next(h: &mut Harness, id: NodeId) {
    h.app.agents.artifact.active_workers = 2;
    h.app.pump_artifact_generations(&h.ctx);
    h.app.agents.artifact.active_workers = 0;
    assert!(
        h.app.agents.artifact.preparing.contains_key(&id),
        "the admitted sibling remains schedulable"
    );
}
fn fail_first(h: &mut Harness, id: NodeId) {
    let plan = h.app.agents.artifact.running[&id].prepared.plan.clone();
    h.app.agents.artifact.active_workers = 2;
    h.app
        .artifact_work_tx()
        .send(WorkDone::Published(
            plan.tab,
            id,
            plan.request.id,
            Err("Synthetic first option failed return validation".into()),
        ))
        .unwrap();
    h.app.pump_artifact_generations(&h.ctx);
    h.app.agents.artifact.active_workers = 0;
    assert!(!h.app.agents.artifact.running.contains_key(&id));
}
fn assert_target_draft(h: &Harness, id: NodeId, other: NodeId, expected: &str) {
    assert_eq!(h.app.agents.prompt(id), Some(expected));
    assert_eq!(
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .instruction,
        expected
    );
    assert_eq!(h.app.agents.prompt(other), Some("OTHER HOST DRAFT"));
}
#[test]
fn second_published_count_sibling_keeps_target_draft_through_actual_editor_sync() {
    for preserve in [false, true] {
        for next in ["", "Unsubmitted next-turn draft.", QUESTION] {
            let (mut h, id, other, parent) = count_fixture("count-two-success-composer", preserve);
            *h.app.agents.prompt_mut(id) = next.into();
            h.app.track_flow_prompt_focus(id, true);
            h.app.board_sel = [other].into_iter().collect();
            let first = publish_fixture(&mut h, id);
            stage_next(&mut h, id);
            assert!(h.app.agents.artifact.preparing[&id].plan.accepted_batch);
            let final_draft = if next.is_empty() {
                String::new()
            } else if next == QUESTION {
                QUESTION.to_owned()
            } else {
                format!("{next} Edited during sibling preparation.")
            };
            *h.app.agents.prompt_mut(id) = final_draft.clone();
            let second = start_prepared_fixture(&mut h, id);
            assert_eq!(second.request.prompt, QUESTION);
            assert_eq!(second.parent.as_ref(), Some(&parent));
            assert_eq!(second.previous.as_ref(), Some(&first.revision_id));
            assert_target_draft(&h, id, other, &final_draft);
            assert_eq!(
                std::fs::read(
                    h.app.agents.artifact.running[&id]
                        .prepared
                        .run
                        .staging_output
                        .join("index.html")
                )
                .unwrap(),
                b"<html>42</html>"
            );
            h.app.board_sel = [id].into_iter().collect();
            crate::app::tests::frame_on(&mut h, id, 0.66);
            assert!(h
                .app
                .open_object_panel(crate::app::board_properties::Panel::Agent));
            h.frame();
            h.frame();
            assert_target_draft(&h, id, other, &final_draft);
            let second_ref = publish_fixture(&mut h, id);
            assert_ne!(first.revision_id, second_ref.revision_id);
            let visible = &artifact(h.app.doc().scene.node(id).unwrap())
                .unwrap()
                .revisions;
            assert!(
                visible.contains(&first.revision_id) && visible.contains(&second_ref.revision_id)
            );
            assert_eq!(
                atlas_ai::versions::load_artifact_revision(&second_ref)
                    .unwrap()
                    .provenance
                    .prompt,
                QUESTION
            );
            assert_target_draft(&h, id, other, &final_draft);
            assert_eq!(
                std::fs::read(parent.materialized_entry).unwrap(),
                b"<html>42</html>"
            );
            assert_eq!(
                h.app
                    .agents
                    .dispatched
                    .iter()
                    .filter(|(target, _)| *target == id)
                    .count(),
                2
            );
        }
    }
}
#[test]
fn accepted_failed_count_branch_does_not_drop_its_sibling_after_composer_changes() {
    for preserve in [false, true] {
        let (mut h, id, other, parent) = count_fixture("count-two-failed-composer", preserve);
        *h.app.agents.prompt_mut(id) = "Next draft after first admission".into();
        h.app.track_flow_prompt_focus(id, true);
        h.app.board_sel = [other].into_iter().collect();
        fail_first(&mut h, id);
        stage_next(&mut h, id);
        *h.app.agents.prompt_mut(id) = "Next draft edited during second preparation".into();
        let second = start_prepared_fixture(&mut h, id);
        assert_eq!(second.request.prompt, QUESTION);
        assert_eq!(second.parent.as_ref(), Some(&parent));
        assert_target_draft(&h, id, other, "Next draft edited during second preparation");
        let published = publish_fixture(&mut h, id);
        assert_eq!(
            atlas_ai::versions::load_artifact_revision(&published)
                .unwrap()
                .provenance
                .prompt,
            QUESTION
        );
        assert_target_draft(&h, id, other, "Next draft edited during second preparation");
        assert_eq!(
            h.app
                .agents
                .dispatched
                .iter()
                .filter(|(target, _)| *target == id)
                .count(),
            2
        );
    }
}
#[test]
fn accepted_count_sibling_still_rejects_changed_source_or_model_parameters() {
    for source in [false, true] {
        let (mut h, id, _, _) = count_fixture("count-two-stale-guard", true);
        fail_first(&mut h, id);
        h.app.patch_nodes(&[id], |node| {
            if source {
                let NodeKind::Portal(portal) = &mut node.kind else {
                    panic!()
                };
                portal.source = Some(SourceUri {
                    locator: "different.html".into(),
                });
            } else {
                slate_doc::agent_chat::agent_mut(node)
                    .unwrap()
                    .model_parameters
                    .insert("effort".into(), "changed".into());
            }
        });
        h.app.agents.artifact.active_workers = 2;
        h.app.pump_artifact_generations(&h.ctx);
        assert!(!h.app.agents.artifact.preparing.contains_key(&id));
        assert!(!h.app.agents.artifact.queued.contains_key(&id));
        assert_eq!(
            h.app
                .agents
                .dispatched
                .iter()
                .filter(|(target, _)| *target == id)
                .count(),
            1
        );
    }
}
