//! Real registered result actions, offline materialization, and normal proposal ownership.
use super::*;
use slate_doc::scene::{CmdAuthor, HiddenOutputRef, NodeId};

fn setup(tag: &str) -> (Harness, String, NodeId, HiddenOutputRef, HiddenOutputRef) {
    let (mut h, host, first, last) = SlateApp::saved_result_test_board(tag);
    h.app.tab_mut().path = Some(h.base.join("results.slate"));
    // Fixtures use absolute saved references before the board is assigned a path.
    h.app.board_sel.clear();
    let who = add(&mut h, "Dot");
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_sources
        .push("board-file-inputs".into());
    let family = slate_doc::scene::artifact(h.app.doc().scene.node(host).unwrap())
        .unwrap()
        .family
        .clone();
    (
        h,
        who,
        host,
        HiddenOutputRef::Artifact {
            family: family.clone(),
            revision: first.revision_id,
        },
        HiddenOutputRef::Artifact {
            family,
            revision: last.revision_id,
        },
    )
}
fn drive(h: &mut Harness, done: impl Fn(&Harness) -> bool) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while !done(h) {
        h.app.pump_artifact_generations(&h.ctx);
        proposal_support::tick(h);
        assert!(
            Instant::now() < deadline,
            "saved result action did not reach bounded completion"
        );
        std::thread::yield_now();
    }
}
fn start(
    h: &mut Harness,
    who: &str,
    host: NodeId,
    command: &str,
    output: &HiddenOutputRef,
) -> BoardCall {
    proposal_support::begin_command(h, who, command, &[host], json!({"output":output}))
}
#[test]
fn registered_artifact_use_and_active_remove_stage_then_commit_as_actual_author_once() {
    for command in ["portal.agent.result.use", "portal.agent.result.remove"] {
        let (mut h, who, host, first, last) = setup(&format!("registered-artifact-{command}"));
        let output = if command.ends_with(".use") {
            first
        } else {
            last
        };
        let before = h.app.doc().scene.node(host).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let prepared = h
            .app
            .prepare_targeted_command(command, &[host.0.to_string()], &json!({"output":output}))
            .unwrap();
        assert!(prepared.pending_artifact_pick().is_some());
        assert!(prepared.cmds.is_empty());
        assert!(prepared.requires_linked_inputs());
        assert!(h.app.dispatch(
            &h.ctx,
            atlas_commands::CommandId(command),
            Some(json!({"tab":h.app.tab().id,"host":host,"binding":h.app.result_binding(host).unwrap(),
                "outputs":[match &output { HiddenOutputRef::Artifact {revision,..} => revision, _ => unreachable!() }],
                "references":[output],"target":match &output { HiddenOutputRef::Artifact {revision,..} => revision, _ => unreachable!() },"witness":before}).to_string())
        ));
        drive(&mut h, |h| !h.app.artifact_revision_loading(host));
        let expected = h.app.doc().scene.node(host).unwrap().clone();
        assert_ne!(expected, before);
        assert_eq!(h.app.tab().journal.last_author(), Some(&CmdAuthor::Human));
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(host), Some(&before));
        let call = start(&mut h, &who, host, command, &output);
        drive(&mut h, |h| h.app.presence.agents[0].work.is_none());
        assert_eq!(h.app.doc().scene.node(host), Some(&before));
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        let proposal = h
            .app
            .presence
            .proposals
            .iter()
            .find(|(_, p)| p.call.id == call.id)
            .unwrap()
            .0
            .clone();
        assert!(h.app.presence.proposals[&proposal]
            .prepared
            .requires_linked_inputs());
        h.app.accept_stage_proposal(&proposal);
        proposal_support::finish(&mut h, &who);
        assert_eq!(h.app.doc().scene.node(host), Some(&expected));
        assert_eq!(
            h.app.tab().journal.last_author(),
            Some(&CmdAuthor::Agent("Dot".into()))
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(host), Some(&before));
    }
}
#[test]
fn pending_registered_artifact_pick_rechecks_actor_grant_source_and_cancel_lease() {
    for mode in [
        "grant", "linked", "epoch", "request", "author", "readonly", "lock", "source", "node",
        "tab", "cancel", "stop", "revoke",
    ] {
        let (mut h, who, host, first, _) = setup(&format!("artifact-pick-fence-{mode}"));
        start(&mut h, &who, host, "portal.agent.result.use", &first);
        drive(&mut h, |h| {
            h.app.presence.agents[0]
                .work
                .as_ref()
                .is_some_and(|work| work.board_prepared().unwrap().artifact_pick_waiting())
        });
        match mode {
            "grant" => h.app.presence.agents[0].grant = None,
            "linked" => h.app.presence.agents[0]
                .grant
                .as_mut()
                .unwrap()
                .readable_sources
                .clear(),
            "epoch" => h.app.presence.agents[0].control_epoch += 1,
            "request" => h.app.presence.agents[0].request.push_str("-new"),
            "author" => h.app.presence.agents[0].profile.name.push_str("-new"),
            "readonly" => h.app.tab_mut().read_only = true,
            "lock" => {
                h.app.patch_nodes(&[host], |node| node.locked = true);
            }
            "source" => {
                h.app.patch_nodes(&[host], |node| {
                    if let slate_doc::scene::NodeKind::Portal(portal) = &mut node.kind {
                        portal.source.as_mut().unwrap().locator.push_str("-changed");
                    }
                });
            }
            "node" => {
                h.app.patch_nodes(&[host], |node| node.opacity = 0.75);
            }
            "tab" => {
                h.app.tab_mut().id += 1;
            }
            "stop" | "revoke" => {
                h.app.stop_presence(&who, mode == "revoke").unwrap();
            }
            _ => {
                assert!(h.app.cancel_artifact_generation(host));
            }
        }
        let expected = h.app.doc().scene.node(host).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        drive(&mut h, |h| {
            !h.app.artifact_revision_loading(host) && h.app.presence.agents[0].work.is_none()
        });
        assert_eq!(h.app.doc().scene.node(host), Some(&expected), "{mode}");
        assert_eq!(h.app.tab().journal.undo_depth(), depth, "{mode}");
        assert!(h.app.presence.proposals.is_empty(), "{mode}");
        assert!(h.app.presence.agents[0].work.is_none(), "{mode}");
    }
}

#[test]
fn explicitly_autonomous_registered_artifact_choice_commits_as_collaborator_after_materialization()
{
    let (mut h, _, host, first, _) = setup("artifact-pick-autonomous");
    let before = h.app.doc().scene.node(host).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let mut actor = h.app.presence.agents.remove(0);
    actor.grant.as_mut().unwrap().mutation_autonomy = true;
    actor.request = atlas_agent::request_id();
    h.app
        .presence
        .roster
        .session_mut(&actor.motion)
        .unwrap()
        .begin_request(id(&actor.request).unwrap())
        .unwrap();
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: actor.binding.clone(),
        request_id: actor.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "portal.agent.result.use".into(),
            targets: vec![host.0.to_string()],
            arguments: json!({"output":first}),
        },
    };
    let reply = h.app.presence_call(&mut actor, &call);
    assert!(reply.is_none(), "actual registered admission: {reply:?}");
    h.app.presence.agents.push(actor);
    drive(&mut h, |h| h.app.presence.agents[0].work.is_none());
    assert_ne!(h.app.doc().scene.node(host), Some(&before));
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(h.app.presence.proposals.is_empty());
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(host), Some(&before));
}

#[test]
fn materialized_artifact_proposal_rechecks_linked_grant_before_acceptance() {
    let (mut h, who, host, first, _) = setup("artifact-proposal-revoke");
    let before = h.app.doc().scene.node(host).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let call = start(&mut h, &who, host, "portal.agent.result.use", &first);
    drive(&mut h, |h| h.app.presence.agents[0].work.is_none());
    let proposal = h
        .app
        .presence
        .proposals
        .iter()
        .find(|(_, p)| p.call.id == call.id)
        .unwrap()
        .0
        .clone();
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .readable_sources
        .clear();
    h.app.accept_stage_proposal(&proposal);
    for _ in 0..10 {
        proposal_support::tick(&mut h);
    }
    assert_eq!(h.app.doc().scene.node(host), Some(&before));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.presence.agents[0].work.is_none());
}
