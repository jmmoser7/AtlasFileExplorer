use super::graft_review::{publish_fixture, start_prepared_fixture};
use super::*;

fn instruction(h: &Harness, id: NodeId) -> &str {
    &slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
        .unwrap()
        .instruction
}

#[test]
fn ordinary_dashboard_first_send_clears_accepted_dialogue_and_reopens_empty() {
    let (mut h, _) = board("ordinary-dashboard-composer-reset");
    let path = h.base.join("original.html");
    std::fs::write(&path, "<html>Original dashboard</html>").unwrap();
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(-380.0, -210.0, 760.0, 420.0),
        NodeKind::Portal(slate_doc::scene::PortalNode::bound_web(
            "Original",
            path.to_string_lossy(),
        )),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = [id].into_iter().collect();
    crate::app::tests::frame_on(&mut h, id, 0.66);
    assert!(h
        .app
        .open_object_panel(crate::app::board_properties::Panel::Agent));
    h.frame();
    assert!(h.app.set_pending_generator_settings(id, None, Some(true)));
    let question = "Create one white dashboard with a teal card";
    h.frame_with(|input| input.events.push(egui::Event::Text(question.into())));
    h.frame_with(|input| {
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
    });
    assert!(
        h.app.is_artifact_node(id),
        "the real pending editor binds the ordinary Web source"
    );
    let plan = start_prepared_fixture(&mut h, id);
    assert_eq!(plan.request.prompt, question);
    assert_eq!(h.app.agents.dispatched.last().unwrap().1.prompt, question);
    assert_eq!(h.app.agents.prompt(id), Some(""));
    assert_eq!(instruction(&h, id), "");
    let reference = publish_fixture(&mut h, id);
    let revision = atlas_ai::versions::load_artifact_revision(&reference).unwrap();
    assert_eq!(revision.provenance.checkpoint.turns[0].text, question);
    assert_eq!(revision.provenance.checkpoint.turns.len(), 2);
    assert_eq!(h.app.agents.prompt(id), Some(""));
    assert_eq!(instruction(&h, id), "");
    let saved = h.base.join("accepted-dashboard.slate");
    h.app.doc().save_to(&saved).unwrap();
    let reopened = slate_doc::SlateDoc::load_from(&saved).unwrap();
    assert_eq!(
        slate_doc::agent_chat::agent(reopened.scene.node(id).unwrap())
            .unwrap()
            .instruction,
        ""
    );
    let followup = "Make the teal card wider";
    *h.app.agents.prompt_mut(id) = followup.into();
    assert!(h.app.queue_artifact_generation(id));
    let next = start_prepared_fixture(&mut h, id);
    assert_eq!(next.request.prompt, followup);
    assert_eq!(h.app.agents.dispatched.last().unwrap().1.prompt, followup);
    assert_eq!(h.app.agents.prompt(id), Some(""));
    assert_eq!(instruction(&h, id), "");
    assert_eq!(
        atlas_ai::versions::load_artifact_revision(&reference)
            .unwrap()
            .provenance
            .checkpoint
            .turns[0]
            .text,
        question
    );
}

#[test]
fn bound_dashboard_clear_follows_visible_preservation_and_never_overwrites_new_running_feedback() {
    for preserve in [None, Some(false), Some(true)] {
        let (mut h, image) = board("bound-dashboard-composer-reset");
        let id = spawn(&mut h, image);
        h.app.patch_nodes(&[id], |node| {
            slate_doc::agent_chat::agent_mut(node)
                .unwrap()
                .generator
                .preserve_history = preserve;
        });
        let question = "Make the supplied pie interactive";
        h.app.sync_instruction(id, "");
        *h.app.agents.prompt_mut(id) = question.into();
        assert!(h.app.queue_artifact_generation(id));
        let plan = start_prepared_fixture(&mut h, id);
        assert_eq!(plan.request.prompt, question);
        let expected = if preserve.unwrap_or(true) {
            ""
        } else {
            question
        };
        assert_eq!(h.app.agents.prompt(id), Some(expected));
        assert_eq!(instruction(&h, id), expected);
        let guard = h.app.agents.artifact.running[&id]
            .prepared
            .plan
            .guard
            .clone();
        *h.app.agents.prompt_mut(id) = "New feedback typed while the accepted job runs".into();
        h.app.track_flow_prompt_focus(id, true);
        assert_eq!(
            h.app.artifact_batch_guard(id).as_deref(),
            Some(guard.as_str()),
            "editing the next composer does not change the captured job"
        );
        let reference = publish_fixture(&mut h, id);
        assert_eq!(
            h.app.agents.prompt(id),
            Some("New feedback typed while the accepted job runs")
        );
        let revision = atlas_ai::versions::load_artifact_revision(&reference).unwrap();
        assert_eq!(revision.provenance.checkpoint.turns[0].text, question);
        assert!(h
            .app
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::scene::artifact)
            .unwrap()
            .revision
            .is_some());
    }
}

#[test]
fn rejected_dashboard_preflight_retains_its_editable_question() {
    let (mut h, image) = board("rejected-dashboard-composer-reset");
    let id = spawn(&mut h, image);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::agent_chat::agent_mut(node)
            .unwrap()
            .generator
            .preserve_history = Some(true);
    });
    let question = "Keep this editable after preflight fails";
    *h.app.agents.prompt_mut(id) = question.into();
    h.app.ai.config.workspace_dir = None;
    assert!(!h.app.queue_artifact_generation(id));
    assert_eq!(h.app.agents.prompt(id), Some(question));
    assert!(h.app.agents.dispatched.is_empty());
}
