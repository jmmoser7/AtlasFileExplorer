//! The shared chat actions dispatch through the registered staging commands.
use super::*;
use slate_doc::scene::CmdAuthor;

fn setup() -> (Harness, String) {
    let mut h = host();
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("board.slate"));
    let who = add(&mut h, "Dot");
    (h, who)
}

fn stage(h: &mut Harness, who: &str) -> String {
    let agent = h
        .app
        .presence
        .agents
        .iter()
        .find(|a| a.profile.id == who)
        .unwrap();
    let author = agent.profile.name.clone();
    let session = agent.binding.session_id.clone();
    let prepared = h
        .app
        .prepare_targeted_command(
            "board.objects.create_shape",
            &[],
            &json!({"x":20,"y":30,"width":80,"height":60}),
        )
        .unwrap();
    h.app
        .finish_targeted_command(prepared, &author, false, &session)
        .unwrap()
        .strip_prefix("staged:")
        .unwrap()
        .to_owned()
}

fn decide(h: &mut Harness, who: &str, proposal: &str, accept: bool) {
    let action = if accept {
        chrome::PresenceAction::AcceptProposal {
            participant: who.into(),
            id: proposal.into(),
        }
    } else {
        chrome::PresenceAction::RejectProposal {
            participant: who.into(),
            id: proposal.into(),
        }
    };
    h.app.apply_presence_action(&h.ctx, action).unwrap();
}

#[test]
fn proposal_chat_rows_borrow_all_current_session_rows_and_registry_labels() {
    let (mut h, who) = setup();
    let first = stage(&mut h, &who);
    let mut recovered = h.app.pending_stage_proposals()[0].clone();
    recovered.id = format!("presence-{}", atlas_agent::request_id());
    recovered.status = slate_doc::ProposalStatus::RecoveryRequired;
    let second = recovered.id.clone();
    h.app.queue_stage_proposal(recovered).unwrap();
    let peer = add(&mut h, "Dot"); // Equal names must not merge independent sessions.
    let foreign = stage(&mut h, &peer);
    let before = serde_json::to_value(h.app.doc()).unwrap();
    let rows = h.app.presence_proposal_rows(&who);
    assert_eq!(
        rows.iter().map(|r| r.id).collect::<Vec<_>>(),
        [first.as_str(), second.as_str()]
    );
    let label = h
        .app
        .registry
        .iter()
        .find(|s| s.id.0 == "board.objects.create_shape")
        .unwrap()
        .name;
    assert!(rows.iter().all(|r| r.label == label));
    assert!(!rows[0].recovery_required && rows[1].recovery_required);
    assert_eq!(h.app.presence_proposal_rows(&peer)[0].id, foreign);
    assert!(h.app.presence_proposal_rows("self").is_empty());
    assert_eq!(serde_json::to_value(h.app.doc()).unwrap(), before);
    assert_eq!(h.app.pending_stage_proposals().len(), 3);
}

#[test]
fn chat_accept_routes_registered_command_to_one_attributed_commit_and_cannot_replay() {
    let (mut h, who) = setup();
    let proposal = stage(&mut h, &who);
    let depth = h.app.tab().journal.undo_depth();
    assert!(h.app.doc().scene.nodes.is_empty());
    decide(&mut h, &who, &proposal, true);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    assert_eq!(
        slate_doc::stage::read_result(&h.base, &proposal).unwrap(),
        Some(slate_doc::stage::ProposalResult::Accepted)
    );
    assert!(h
        .app
        .cmd_history
        .iter()
        .any(|entry| entry.id.0 == "presence.accept"
            && entry.author == atlas_commands::CmdAuthor::Human));
    assert!(h.app.presence_proposal_rows(&who).is_empty());
    decide(&mut h, &who, &proposal, true);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(
        !h.app.presence.agents[0]
            .grant
            .as_ref()
            .unwrap()
            .mutation_autonomy
    );
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert!(h.app.doc().scene.nodes.is_empty());
}

#[test]
fn chat_reject_records_the_decision_without_mutating_the_board() {
    let (mut h, who) = setup();
    let proposal = stage(&mut h, &who);
    let before = serde_json::to_value(h.app.doc()).unwrap();
    let depth = h.app.tab().journal.undo_depth();
    decide(&mut h, &who, &proposal, false);
    assert_eq!(serde_json::to_value(h.app.doc()).unwrap(), before);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(
        slate_doc::stage::read_result(&h.base, &proposal).unwrap(),
        Some(slate_doc::stage::ProposalResult::Rejected)
    );
    assert!(h
        .app
        .cmd_history
        .iter()
        .any(|entry| entry.id.0 == "presence.reject"));
    assert!(h.app.presence_proposal_rows(&who).is_empty());
    decide(&mut h, &who, &proposal, true);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

#[test]
fn stale_chat_actions_cannot_decide_another_agent_session_or_tab() {
    let (mut h, who) = setup();
    let proposal = stage(&mut h, &who);
    let peer = add(&mut h, "Dot");
    let depth = h.app.tab().journal.undo_depth();
    decide(&mut h, &peer, &proposal, true);
    decide(&mut h, &peer, &proposal, false);
    assert_eq!(h.app.presence_proposal_rows(&who).len(), 1);
    let original = h.app.presence.agents[0].binding.session_id.clone();
    h.app.presence.agents[0].binding.session_id = atlas_agent::request_id();
    assert!(h.app.presence_proposal_rows(&who).is_empty());
    decide(&mut h, &who, &proposal, true);
    h.app.presence.agents[0].binding.session_id = original;
    let board = h.app.active_tab;
    let path = h.app.tab().path.clone();
    h.app.tab_mut().path = Some(h.base.join("another.slate"));
    assert!(h.app.presence_proposal_rows(&who).is_empty());
    decide(&mut h, &who, &proposal, false);
    h.app.tab_mut().path = path.clone();
    h.app.new_tab();
    h.app.tab_mut().path = path;
    assert!(h.app.presence_proposal_rows(&who).is_empty());
    decide(&mut h, &who, &proposal, true);
    decide(&mut h, &who, &proposal, false);
    h.app.switch_tab(board);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert_eq!(h.app.presence_proposal_rows(&who).len(), 1);
    assert_eq!(
        slate_doc::stage::read_result(&h.base, &proposal).unwrap(),
        None
    );
}

#[test]
fn retirement_removes_chat_decisions_and_a_late_click_cannot_restore_them() {
    let (mut h, who) = setup();
    let proposal = stage(&mut h, &who);
    let depth = h.app.tab().journal.undo_depth();
    h.app.retire_presence(&who, "bow").unwrap();
    assert!(h.app.presence_proposal_rows(&who).is_empty());
    decide(&mut h, &who, &proposal, true);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.doc().scene.nodes.is_empty());
}
