//! Explicit approval is a scoped visible action, never a general Resume.
use super::proposal_support::{begin_command, finish, stage_command, tick};
use super::*;
use crate::app::tests::{add_filled_rect, picture_from, primary_button};
use slate_doc::scene::CmdAuthor;
use std::time::Duration;

#[path = "proposal_reservation_tests.rs"]
mod reservations;

pub(super) fn setup() -> (Harness, String, slate_doc::NodeId) {
    let mut h = host();
    h.app.set_agent_programs_for_test(&[]);
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("approved-board.slate"));
    let target = picture_from(&mut h, "approved-image.png", -100.0);
    h.app.board_sel.clear();
    let who = add(&mut h, "Dot");
    (h, who, target)
}
fn stage_move(h: &mut Harness, who: &str, target: slate_doc::NodeId) -> String {
    stage_command(
        h,
        who,
        "board.objects.move",
        &[target],
        json!({"dx":300,"dy":40}),
    )
}

#[test]
fn accepted_images_show_three_replacement_positions_then_one_attributed_undoable_commit() {
    let (mut h, who, target) = setup();
    let before = h.app.doc().scene.node(target).unwrap().clone();
    let scene = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let proposal = stage_move(&mut h, &who, target);
    assert_eq!(h.app.doc().scene, scene);
    assert!(
        h.app.presence_preview(target).is_none(),
        "proposal never replaces original"
    );
    h.app.accept_stage_proposal(&proposal);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    let mut positions = Vec::new();
    for _ in 0..160 {
        tick(&mut h);
        if h.app.presence.agents[0].work.is_none() {
            break;
        }
        assert_eq!(h.app.doc().scene, scene);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        if let Some(node) = h.app.presence_preview(target) {
            if node.rect.x > before.rect.x && node.rect.x < before.rect.x + 300.0 {
                assert_eq!(node.id, before.id);
                assert_eq!(
                    node.kind, before.kind,
                    "the linked image itself is previewed"
                );
                positions.push(node.rect.x);
            }
        }
    }
    assert!(positions.len() >= 3 && positions.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(
        h.app.doc().scene.node(target).unwrap().rect.x,
        before.rect.x + 300.0
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    h.app.accept_stage_proposal(&proposal);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene, scene);
}

#[test]
fn proposed_ghost_primary_press_cannot_materialize_it_or_hold_authored_images() {
    let (mut h, who, target) = setup();
    h.frame();
    let before = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let proposal = stage_move(&mut h, &who, target);
    let ghost = h.app.presence.proposals[&proposal].previews[&target].clone();
    let xf = h.app.board_xf();
    let at = xf.w2s(egui::pos2(ghost.rect.x + 8.0, ghost.rect.y + 8.0));
    let p = h.app.presence.agents[0]
        .position(&h.app.presence.roster)
        .unwrap();
    let avatar = xf.w2s(egui::pos2(p.x as f32, p.y as f32));
    assert!(xf.rect_w2s(ghost.rect).contains(at) && h.app.canvas_rect.contains(at));
    assert!(
        avatar.distance(at) > (18.0 * xf.z).max(12.0),
        "staged ghost press stays outside the actual avatar target"
    );
    h.app.presence.refresh_held(h.app.tab().id);
    assert!(!h.app.presence.holds_node(target));
    assert!(h.app.presence_preview(target).is_none());
    assert!(h
        .app
        .presence_preview_additions(h.app.canvas_rect)
        .is_empty());
    h.frame_with(primary_button(at, true, false));
    assert_eq!(
        h.app.doc().scene,
        before,
        "a real board press cannot take over an unapproved ghost"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.frame_with(primary_button(at, false, false));
    assert!(h.app.presence.proposals.contains_key(&proposal));
    begin_command(
        &mut h,
        &who,
        "board.objects.move",
        &[target],
        json!({"dx":400,"dy":200}),
    );
    for _ in 0..100 {
        tick(&mut h);
        if h.app.presence.agents[0]
            .work
            .as_ref()
            .unwrap()
            .previews
            .values()
            .any(|n| n.rect != h.app.doc().scene.node(target).unwrap().rect)
        {
            break;
        }
    }
    let active = &h.app.presence.agents[0].work.as_ref().unwrap().previews[&target];
    let xf = h.app.board_xf();
    let at = xf.w2s(egui::pos2(active.rect.x + 8.0, active.rect.y + 8.0));
    let p = h.app.presence.agents[0]
        .position(&h.app.presence.roster)
        .unwrap();
    let avatar = xf.w2s(egui::pos2(p.x as f32, p.y as f32));
    assert!(xf.rect_w2s(active.rect).contains(at) && h.app.canvas_rect.contains(at));
    assert!(
        avatar.distance(at) > (18.0 * xf.z).max(12.0),
        "active ghost press stays outside the actual avatar target"
    );
    h.app.presence.refresh_held(h.app.tab().id);
    assert!(!h.app.presence.holds_node(target));
    assert!(h.app.presence_preview(target).is_none());
    assert!(h
        .app
        .presence_preview_additions(h.app.canvas_rect)
        .is_empty());
    h.frame_with(primary_button(at, true, false));
    assert_eq!(
        h.app.doc().scene,
        before,
        "active proposed motion is also unpickable"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.frame_with(primary_button(at, false, false));
}

#[test]
fn each_pending_proposal_retains_its_prepared_action_and_accept_is_not_an_approved_queue() {
    let (mut h, who, target) = setup();
    let second = add_filled_rect(&mut h.app, 500.0, 100.0, 80.0, 60.0);
    let first = stage_move(&mut h, &who, target);
    let other = stage_move(&mut h, &who, second);
    assert_eq!(h.app.presence_proposal_rows(&who).len(), 2);
    h.app.accept_stage_proposal(&first);
    tick(&mut h);
    let index = h.app.presence.agents[0].work.as_ref().unwrap().index;
    h.app.accept_stage_proposal(&first);
    assert_eq!(h.app.presence.agents[0].work.as_ref().unwrap().index, index);
    h.app.accept_stage_proposal(&other);
    assert!(h.app.presence.proposals[&other].approval.is_none());
    assert!(
        h.app
            .presence_proposal_rows(&who)
            .iter()
            .find(|r| r.id == first)
            .unwrap()
            .applying
    );
    finish(&mut h, &who);
    assert!(h.app.presence.proposals.contains_key(&other));
    assert_eq!(h.app.doc().scene.node(second).unwrap().rect.x, 500.0);
    h.app.accept_stage_proposal(&other);
    finish(&mut h, &who);
    assert_eq!(h.app.doc().scene.node(second).unwrap().rect.x, 800.0);
    assert!(h.app.presence_proposal_rows(&who).is_empty());
}

#[test]
fn listening_accept_moves_only_the_approved_action_and_leaves_queued_read_and_call_parked() {
    let (mut h, who, target) = setup();
    h.frame();
    let proposal = stage_move(&mut h, &who, target);
    let mut a = h.app.presence.agents.remove(0);
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: a.calls as u64,
        operation: BoardOperation::Read {
            node_ids: vec![target.0.to_string()],
            offset: 0,
            limit: 1,
        },
    };
    h.app.begin_presence_read(&mut a, &call).unwrap();
    let dir = a.dir.clone();
    h.app
        .presence
        .links
        .set_session(
            &dir,
            a.binding.clone(),
            a.grant.clone().unwrap(),
            a.request.clone(),
        )
        .unwrap();
    let calls = a.calls;
    h.app.presence.agents.insert(0, a);
    h.app.presence_address(&h.ctx, &who).unwrap();
    let path = dir.join("board-calls").join(format!("{}.json", call.id));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    atlas_ai::agent::atomic_write_json(&path, &call).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while path.exists() {
        assert!(
            Instant::now() < deadline,
            "loopback board worker did not queue the call"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    h.app.accept_stage_proposal(&proposal);
    assert!(h.app.presence.agents[0].paused);
    assert!(h
        .app
        .presence
        .approved_while_listening(&h.app.presence.agents[0]));
    tick(&mut h);
    assert_eq!(
        h.app.presence.avatar_activity(&h.app.presence.agents[0]),
        "moving"
    );
    assert!(h.app.presence.agents[0]
        .status
        .contains("Applying approved"));
    finish(&mut h, &who);
    let a = &h.app.presence.agents[0];
    assert!(a.paused && a.read.is_some());
    assert_eq!(a.activity, "listening");
    assert_eq!(h.app.presence.avatar_activity(a), "listening");
    assert_eq!(a.calls, calls);
    assert_eq!(h.app.presence.links.poll_limit(&dir, 1), vec![call]);
    assert!(!a.grant.as_ref().unwrap().mutation_autonomy);
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Pause {
                id: who,
                paused: false,
            },
        )
        .unwrap();
    tick(&mut h);
    assert!(!h.app.presence.agents[0].paused && h.app.presence.agents[0].read.is_none());
}

#[test]
fn later_human_pause_stops_the_scoped_exception_and_resume_is_preserved() {
    let (mut h, who, target) = setup();
    let proposal = stage_move(&mut h, &who, target);
    h.app.presence_address(&h.ctx, &who).unwrap();
    h.app.accept_stage_proposal(&proposal);
    tick(&mut h);
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Pause {
                id: who.clone(),
                paused: true,
            },
        )
        .unwrap();
    assert!(!h
        .app
        .presence
        .approved_while_listening(&h.app.presence.agents[0]));
    let sample = h
        .app
        .presence
        .roster
        .session(&h.app.presence.agents[0].motion)
        .unwrap()
        .sample();
    for _ in 0..5 {
        tick(&mut h);
    }
    assert_eq!(
        h.app
            .presence
            .roster
            .session(&h.app.presence.agents[0].motion)
            .unwrap()
            .sample()
            .position,
        sample.position
    );
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Pause {
                id: who.clone(),
                paused: false,
            },
        )
        .unwrap();
    finish(&mut h, &who);
    assert!(!h.app.presence.agents[0].paused);
    assert!(
        !h.app.presence.agents[0]
            .grant
            .as_ref()
            .unwrap()
            .mutation_autonomy
    );
}

#[test]
fn revoke_stop_retire_reject_and_request_replacement_cancel_approved_motion_without_edits() {
    for cancellation in ["revoke", "stop", "retire", "reject", "request", "grant"] {
        let (mut h, who, target) = setup();
        let proposal = stage_move(&mut h, &who, target);
        let before = h.app.doc().scene.clone();
        let depth = h.app.tab().journal.undo_depth();
        h.app.accept_stage_proposal(&proposal);
        for _ in 0..100 {
            tick(&mut h);
            if h.app
                .presence_preview(target)
                .is_some_and(|n| n.rect != before.node(target).unwrap().rect)
            {
                break;
            }
        }
        assert!(
            h.app
                .presence_preview(target)
                .is_some_and(|n| n.rect != before.node(target).unwrap().rect),
            "cancel a visible intermediate drag"
        );
        match cancellation {
            "revoke" => h.app.stop_presence(&who, true).unwrap(),
            "stop" => h.app.stop_presence(&who, false).unwrap(),
            "retire" => h.app.retire_presence(&who, "bow").unwrap(),
            "reject" => h.app.reject_stage_proposal(&proposal),
            "request" => h.app.presence.agents[0].request = atlas_agent::request_id(),
            _ => h.app.presence.agents[0]
                .grant
                .as_mut()
                .unwrap()
                .commands
                .clear(),
        }
        for _ in 0..8 {
            tick(&mut h);
        }
        assert_eq!(h.app.doc().scene, before, "{cancellation}");
        assert_eq!(h.app.tab().journal.undo_depth(), depth, "{cancellation}");
        assert!(h.app.presence.agents[0].work.is_none());
        assert!(!h
            .app
            .presence
            .proposals
            .get(&proposal)
            .is_some_and(|p| p.approval.is_some()));
    }
}

#[test]
fn staged_z_order_preserves_requested_position() {
    let (mut h, who, a) = setup();
    let b = add_filled_rect(&mut h.app, 200.0, 0.0, 100.0, 80.0);
    let c = add_filled_rect(&mut h.app, 400.0, 0.0, 100.0, 80.0);
    let original = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let proposal = stage_command(&mut h, &who, "board.objects.order", &[c], json!({"z":0}));
    assert_eq!(h.app.doc().scene, original);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    h.app.accept_stage_proposal(&proposal);
    assert_eq!(h.app.doc().scene, original);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    finish(&mut h, &who);
    assert_eq!(
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        vec![c, a, b]
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(
        h.app.tab().journal.last_author(),
        Some(&CmdAuthor::Agent("Dot".into()))
    );
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
    assert_eq!(h.app.doc().scene, original);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
