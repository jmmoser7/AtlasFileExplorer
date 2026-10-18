//! The same prepared-action owner reserves real targets for every entry point.
use super::*;

#[test]
fn simultaneous_same_target_ghosts_do_not_reserve_but_only_one_accepted_actor_can_handle_it() {
    let (mut h, first, target) = setup();
    let peer = add(&mut h, "Tess");
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let first_call = begin_command(
        &mut h,
        &first,
        "board.objects.move",
        &[target],
        json!({"dx":220,"dy":20}),
    );
    let peer_call = begin_command(
        &mut h,
        &peer,
        "board.objects.move",
        &[target],
        json!({"dx":-120,"dy":70}),
    );
    assert!(
        h.app.presence.agents.iter().all(|a| a
            .work
            .as_ref()
            .is_some_and(|w| w.purpose == witness::WitnessPurpose::ProposalPreview)),
        "active ghosts do not reserve"
    );
    for _ in 0..160 {
        tick(&mut h);
        assert_eq!(h.app.doc().scene, before);
        assert!(!h.app.presence.holds_node(target));
        if h.app.presence.agents.iter().all(|a| a.work.is_none()) {
            break;
        }
    }
    let find = |call: &BoardCall| {
        h.app
            .presence
            .proposals
            .iter()
            .find(|(_, p)| p.call.id == call.id)
            .unwrap()
            .0
            .clone()
    };
    let first_id = find(&first_call);
    let peer_id = find(&peer_call);
    assert_eq!(h.app.presence_proposal_rows(&first).len(), 1);
    assert_eq!(h.app.presence_proposal_rows(&peer).len(), 1);
    h.app.accept_stage_proposal(&first_id);
    h.app.accept_stage_proposal(&peer_id);
    assert!(h.app.presence.proposals[&first_id].approval.is_some());
    assert!(h.app.presence.proposals[&peer_id].approval.is_none());
    assert!(h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == peer)
        .unwrap()
        .work
        .is_none());
    assert!(h
        .app
        .toasts
        .last()
        .unwrap()
        .0
        .contains("Another collaborator is handling this target"));
    assert_eq!(h.app.doc().scene, before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    finish(&mut h, &first);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        slate_doc::stage::read_result(&h.base, &peer_id).unwrap(),
        None
    );
    assert!(h.app.presence.proposals[&peer_id].approval.is_none());
}

#[test]
fn accepted_actions_on_distinct_targets_can_run_in_parallel_and_commit_independently() {
    let (mut h, first, target) = setup();
    let peer = add(&mut h, "Tess");
    let other = add_filled_rect(&mut h.app, 500.0, 100.0, 80.0, 60.0);
    let first_id = stage_command(
        &mut h,
        &first,
        "board.objects.move",
        &[target],
        json!({"dx":220,"dy":20}),
    );
    let peer_id = stage_command(
        &mut h,
        &peer,
        "board.objects.move",
        &[other],
        json!({"dx":-120,"dy":70}),
    );
    let first_before = h.app.doc().scene.node(target).unwrap().rect;
    let other_before = h.app.doc().scene.node(other).unwrap().rect;
    let depth = h.app.tab().journal.undo_depth();
    h.app.accept_stage_proposal(&first_id);
    h.app.accept_stage_proposal(&peer_id);
    assert!(h.app.presence.agents.iter().all(|a| a.work.is_some()));
    assert!(h
        .app
        .presence
        .proposals
        .values()
        .all(|p| p.approval.is_some()));
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let mut simultaneous = false;
    for _ in 0..160 {
        tick(&mut h);
        simultaneous |=
            h.app.presence_preview(target).is_some() && h.app.presence_preview(other).is_some();
        if h.app.presence.agents.iter().all(|a| a.work.is_none()) {
            break;
        }
    }
    assert!(
        simultaneous,
        "both actual replacement previews can be visible together"
    );
    assert_eq!(
        h.app.doc().scene.node(target).unwrap().rect.x,
        first_before.x + 220.0
    );
    assert_eq!(
        h.app.doc().scene.node(other).unwrap().rect.x,
        other_before.x - 120.0
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);
    for id in [first_id, peer_id] {
        assert_eq!(
            slate_doc::stage::read_result(&h.base, &id).unwrap(),
            Some(slate_doc::stage::ProposalResult::Accepted)
        );
    }
}

#[test]
fn ordinary_admission_uses_the_same_real_target_reservation() {
    let (mut h, first, target) = setup();
    let peer = add(&mut h, "Tess");
    let proposal = stage_command(
        &mut h,
        &first,
        "board.objects.move",
        &[target],
        json!({"dx":220,"dy":20}),
    );
    h.app.accept_stage_proposal(&proposal);
    let mut agent = h.app.presence.agents.remove(1);
    agent.request = atlas_agent::request_id();
    h.app
        .presence
        .roster
        .session_mut(&agent.motion)
        .unwrap()
        .begin_request(id(&agent.request).unwrap())
        .unwrap();
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: agent.binding.clone(),
        request_id: agent.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "board.objects.move".into(),
            targets: vec![target.0.to_string()],
            arguments: json!({"dx":10,"dy":10}),
        },
    };
    let reply = h.app.presence_call(&mut agent, &call).unwrap();
    assert!(!reply.success && agent.work.is_none());
    assert!(agent
        .status
        .contains("Another collaborator is handling this target"));
    h.app.presence.agents.insert(1, agent);
    assert_eq!(h.app.presence.agents[1].profile.id, peer);
}
