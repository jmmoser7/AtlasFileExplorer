//! Approval retains the existing replay, resource, and live-grant fences.
use super::approved_motion::setup;
use super::proposal_support::{finish, stage_command, tick};
use super::*;
use slate_doc::stage::{ProposalResult, ProposalStatus};
use std::time::Duration;

#[test]
fn replay_or_recovery_decisions_before_start_or_before_commit_never_apply_edits() {
    for (at_start, decision) in [
        (true, ProposalResult::Applying),
        (false, ProposalResult::Applying),
        (true, ProposalResult::Accepted),
        (false, ProposalResult::Rejected),
    ] {
        let (mut h, who, target) = setup();
        let proposal = stage_command(
            &mut h,
            &who,
            "board.objects.move",
            &[target],
            json!({"dx":200,"dy":0}),
        );
        let before = h.app.doc().scene.clone();
        let depth = h.app.tab().journal.undo_depth();
        if !at_start {
            h.app.accept_stage_proposal(&proposal);
            tick(&mut h);
        }
        slate_doc::stage::write_result(&h.base, &proposal, &decision).unwrap();
        if at_start {
            h.app.accept_stage_proposal(&proposal);
        }
        finish(&mut h, &who);
        assert_eq!(h.app.doc().scene, before);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(!h
            .app
            .presence
            .proposals
            .get(&proposal)
            .is_some_and(|p| p.approval.is_some()));
        assert_eq!(
            slate_doc::stage::read_result(&h.base, &proposal).unwrap(),
            Some(decision.clone())
        );
        if decision == ProposalResult::Applying {
            assert_eq!(
                h.app.pending_stage_proposals()[0].status,
                ProposalStatus::RecoveryRequired
            );
        }
    }
}

#[test]
fn retained_source_metadata_and_live_source_grant_are_rechecked_at_approved_commit() {
    for revoked in [false, true] {
        let (mut h, who, source) = setup();
        // File input grants are scoped to an existing canonical workbook.
        std::fs::write(h.base.join("approved-board.slate"), b"{}").unwrap();
        h.app
            .apply_presence_action(
                &h.ctx,
                chrome::PresenceAction::SetFileInputs {
                    id: who.clone(),
                    enabled: true,
                },
            )
            .unwrap();
        let proposal = stage_command(
            &mut h,
            &who,
            "board.objects.place_images",
            &[source],
            json!({"x":600,"y":100}),
        );
        let before = h.app.doc().scene.clone();
        let depth = h.app.tab().journal.undo_depth();
        h.app.accept_stage_proposal(&proposal);
        for _ in 0..3 {
            tick(&mut h);
        }
        if revoked {
            h.app.presence.agents[0]
                .grant
                .as_mut()
                .unwrap()
                .readable_sources
                .clear();
        } else {
            let item = match &h.app.doc().scene.node(source).unwrap().kind {
                slate_doc::NodeKind::Image(image) => image.item,
                _ => unreachable!(),
            };
            let replacement = h.base.join("different-unopened-image.png");
            h.app
                .doc_mut()
                .items
                .iter_mut()
                .find(|i| i.id == item)
                .unwrap()
                .path = replacement;
        }
        finish(&mut h, &who);
        assert_eq!(h.app.doc().scene, before);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert_eq!(
            slate_doc::stage::read_result(&h.base, &proposal).unwrap(),
            None
        );
        assert!(!h.app.presence.proposals[&proposal].approval.is_some());
    }
}

#[test]
fn accepted_created_chats_gain_only_explicit_live_own_chat_scope() {
    for allow_own in [false, true] {
        let (mut h, who, _) = setup();
        h.app.presence.agents[0]
            .grant
            .as_mut()
            .unwrap()
            .conversation_ids
            .push("unrelated-explicit-chat".into());
        if allow_own {
            h.app.presence_set_own_chats(&who, true).unwrap();
        }
        let proposal = stage_command(
            &mut h,
            &who,
            "board.objects.create_train",
            &[],
            json!({"x":600,"y":100,"width":640,"height":420,"provider":"local"}),
        );
        let created = h.app.presence.proposals[&proposal].prepared.created_ids();
        assert_eq!(created.len(), 1);
        let depth = h.app.tab().journal.undo_depth();
        h.app.accept_stage_proposal(&proposal);
        assert!(h.app.doc().scene.node(created[0]).is_none());
        finish(&mut h, &who);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        let session = &slate_doc::agent_chat::agent(h.app.doc().scene.node(created[0]).unwrap())
            .unwrap()
            .session;
        let a = &h.app.presence.agents[0];
        let grant = a.grant.as_ref().unwrap();
        assert_eq!(grant.conversation_ids.contains(session), allow_own);
        assert_eq!(a.owned_conversations.contains(session), allow_own);
        assert!(grant
            .conversation_ids
            .contains(&"unrelated-explicit-chat".into()));
        assert!(!grant.mutation_autonomy);
        assert!(!grant
            .readable_sources
            .iter()
            .any(|s| s == "board-file-inputs"));
        assert!(
            a.launched.is_empty(),
            "creating a train never launches a provider"
        );
    }
}

#[test]
fn fresh_idle_provider_snapshot_cannot_cancel_the_humans_already_approved_action() {
    let (mut h, who, target) = setup();
    let proposal = stage_command(
        &mut h,
        &who,
        "board.objects.move",
        &[target],
        json!({"dx":200,"dy":0}),
    );
    let before = h.app.doc().scene.node(target).unwrap().rect;
    h.app.presence.agents[0].seen_running = true;
    h.app.presence.agents[0].inflight = true;
    h.app.accept_stage_proposal(&proposal);
    let a = &h.app.presence.agents[0];
    let dir = a.dir.clone();
    let snapshot = AgentSession {
        request: a.request.clone(),
        status: atlas_agent::AgentStatus::Idle,
        turns: vec![atlas_agent::AgentTurn {
            role: "assistant".into(),
            text: "The proposed move is ready for your approval.".into(),
            at: 1,
        }],
        ..Default::default()
    };
    std::fs::create_dir_all(&dir).unwrap();
    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &snapshot).unwrap();
    h.app.presence.sources.expect(&dir, &snapshot.request);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if h.app.presence.sources.poll(&dir, None).as_deref() == Some(&snapshot) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "current-request idle snapshot was not observed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        h.app.presence.agents[0].work.is_some(),
        "the snapshot is delivered during approved work"
    );
    tick(&mut h);
    assert_eq!(
        h.app.presence.agents[0].snapshot.as_deref(),
        Some(&snapshot)
    );
    assert!(
        h.app.presence.agents[0].work.is_some(),
        "fresh Idle must not cancel this approval"
    );
    assert!(!h.app.presence.agents[0].request.is_empty());
    finish(&mut h, &who);
    assert_eq!(
        h.app.doc().scene.node(target).unwrap().rect.x,
        before.x + 200.0
    );
    assert_eq!(
        slate_doc::stage::read_result(&h.base, &proposal).unwrap(),
        Some(ProposalResult::Accepted)
    );
}

#[test]
fn every_display_status_projects_the_current_grant_without_hiding_activity_or_errors() {
    let (mut h, _, _) = setup();
    let a = &mut h.app.presence.agents[0];
    a.status = "Ready · proposal mode".into();
    assert_eq!(
        super::super::surface::presence_mode_status(a, None),
        "Ready · proposal mode"
    );
    a.grant.as_mut().unwrap().mutation_autonomy = true;
    assert_eq!(
        super::super::surface::presence_mode_status(a, None),
        "Ready · autonomous board edits"
    );
    for status in [
        "Working",
        "Listening; task suspended",
        "Provider unavailable",
    ] {
        a.status = status.into();
        assert_eq!(
            super::super::surface::presence_mode_status(a, None),
            format!("{status} · autonomous board edits")
        );
    }
    assert_eq!(
        super::super::surface::presence_mode_status(a, Some("Could not save conversation")),
        "Could not save conversation · autonomous board edits"
    );
    a.grant = None;
    assert!(super::super::surface::presence_mode_status(a, None).ends_with(" · no board access"));
}

#[test]
fn heavy_image_approval_does_not_repeat_deep_proposal_equality_during_motion_samples() {
    let (mut h, who, target) = setup();
    let mut segs = Vec::new();
    for i in 1..4096 {
        let p = i as f32 / 1024.0;
        let to = if p < 1.0 {
            [p, 0.0]
        } else if p < 2.0 {
            [1.0, p - 1.0]
        } else if p < 3.0 {
            [3.0 - p, 1.0]
        } else {
            [0.0, 4.0 - p]
        };
        segs.push(slate_doc::scene::PathSeg::Line { to });
    }
    h.app.patch_nodes(&[target], |n| {
        n.clip = Some(slate_doc::scene::PathData {
            start: [0.0, 0.0],
            segs: segs.clone(),
            closed: true,
            ..Default::default()
        })
    });
    let proposal = stage_command(
        &mut h,
        &who,
        "board.objects.move",
        &[target],
        json!({"dx":200,"dy":0}),
    );
    super::super::proposals::FULL_PROPOSAL_CHECKS.with(|checks| checks.set(0));
    h.app.accept_stage_proposal(&proposal);
    assert_eq!(
        super::super::proposals::FULL_PROPOSAL_CHECKS.with(|checks| checks.get()),
        1
    );
    let mut samples = 0;
    for _ in 0..160 {
        tick(&mut h);
        if h.app.presence.agents[0].work.is_none() {
            break;
        }
        samples += 1;
        assert_eq!(
            super::super::proposals::FULL_PROPOSAL_CHECKS.with(|checks| checks.get()),
            1,
            "the immutable heavy command must not be compared on every frame"
        );
    }
    assert!(samples >= 3);
    assert_eq!(
        super::super::proposals::FULL_PROPOSAL_CHECKS.with(|checks| checks.get()),
        2,
        "one exact check on Accept and one immediately precommit"
    );
    assert_eq!(
        slate_doc::stage::read_result(&h.base, &proposal).unwrap(),
        Some(ProposalResult::Accepted)
    );
}
