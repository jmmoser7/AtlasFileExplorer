//! Actual app witness/controls and local mailbox submission, no live provider.
use super::super::tests::{add, host, proposal_support::tick};
use super::super::*;
use serde_json::json;
use std::time::Duration;

fn setup() -> (crate::app::tests::Harness, String, String) {
    let mut h = host();
    h.app.ensure_work_tab();
    h.app.set_agent_programs_for_test(&[]);
    let sender = add(&mut h, "Sender");
    let recipient = add(&mut h, "Recipient");
    h.app.presence.agents[0]
        .grant
        .as_mut()
        .unwrap()
        .conversation_ids
        .push(recipient.clone());
    let motion = h.app.presence.agents[1].motion.clone();
    h.app
        .presence
        .roster
        .session_mut(&motion)
        .unwrap()
        .settle_idle_position(point(800.0, 300.0))
        .unwrap();
    (h, sender, recipient)
}
/// Use the real local submission/binding owner, then the ordinary app poller.
/// The local adapter only writes the request mailbox; no provider is launched.
fn begin(h: &mut crate::app::tests::Harness, sender: &str, recipient: &str) -> BoardCall {
    h.app
        .send_presence_message(sender, "Coordinate this layout review")
        .unwrap();
    let a = &h.app.presence.agents[0];
    let dir = a.dir.clone();
    let call = BoardCall {
        id: atlas_agent::request_id(),
        session: a.binding.clone(),
        request_id: a.request.clone(),
        sequence: 0,
        operation: BoardOperation::Invoke {
            command: "presence.send".into(),
            targets: Vec::new(),
            arguments: json!({"recipient":recipient,"text":"Please review this layout"}),
        },
    };
    pump_until(h, |_| {
        std::fs::read(dir.join("board-session.json"))
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<atlas_ai::board_link::BoardBinding>(&bytes).ok()
            })
            .is_some_and(|b| b.session == call.session && b.request_id == call.request_id)
    });
    atlas_ai::agent::atomic_write_json(
        &dir.join("board-calls").join(format!("{}.json", call.id)),
        &call,
    )
    .unwrap();
    pump_until(h, |h| h.app.presence.agents[0].work.is_some());
    let w = h.app.presence.agents[0].work.as_ref().unwrap();
    assert_eq!(
        w.call, call,
        "ordinary BoardLinks poller admitted the exact mailbox call"
    );
    assert!(matches!(w.action, witness::WitnessAction::Relay(_)));
    assert_eq!(w.purpose, witness::WitnessPurpose::Interaction);
    assert!(w.steps.is_empty() && w.targets().is_empty() && w.previews.is_empty());
    assert!(!w.purpose.replaces_objects());
    call
}
fn pump_until(
    h: &mut crate::app::tests::Harness,
    ready: impl Fn(&crate::app::tests::Harness) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready(h) {
        h.app.presence_pump(&h.ctx);
        assert!(
            Instant::now() < deadline,
            "local board transport did not respond"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn terminal_once(h: &mut crate::app::tests::Harness, call: &BoardCall, success: bool) {
    let dir = h.app.presence.agents[0].dir.join("board-replies");
    let path = dir.join(format!("{}.json", call.id));
    pump_until(h, |_| path.exists());
    let bytes = std::fs::read(&path).unwrap();
    let reply: BoardReply = serde_json::from_slice(&bytes).unwrap();
    assert!(reply.matches(call));
    assert_eq!(reply.success, success);
    if success {
        assert_eq!(reply.result["status"], "submitted");
    } else {
        assert!(reply.result["error"]
            .as_str()
            .is_some_and(|e| !e.is_empty()));
    }
    // Existing BoardLinks owns one-response dedup. A repeated terminal outcome
    // must not replace the observed success/cancellation or wake the recipient.
    h.app
        .presence
        .links
        .reply(
            &h.app.presence.agents[0].dir,
            BoardReply::error(call, "duplicate terminal must be inert"),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(225);
    while Instant::now() < deadline {
        h.app.presence_pump(&h.ctx);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "one immutable terminal response for this call"
    );
    let replies: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .collect();
    assert_eq!(
        replies.len(),
        1,
        "exactly one terminal reply in the sender's bound session"
    );
    assert_eq!(replies[0].path(), path);
}
fn no_delivery(h: &crate::app::tests::Harness, recipient: &str) {
    assert!(h
        .app
        .presence
        .conversations
        .get(recipient)
        .is_none_or(|c| c.turns.iter().all(|t| t.role != "user")));
    assert!(h.app.presence.agents[1].request.is_empty());
}
#[test]
fn relay_has_three_real_approach_samples_before_one_contact_submission_without_scene_edits() {
    let (mut h, sender, recipient) = setup();
    let scene = h.app.doc().scene.clone();
    let depth = h.app.tab().journal.undo_depth();
    let call = begin(&mut h, &sender, &recipient);
    no_delivery(&h, &recipient);
    let mut samples = Vec::new();
    for _ in 0..3 {
        tick(&mut h);
        no_delivery(&h, &recipient);
        let a = &h.app.presence.agents[0];
        let sample = h.app.presence.roster.session(&a.motion).unwrap().sample();
        assert_eq!(sample.phase, MotionPhase::Travel);
        samples.push(sample.position);
        assert!(a.work.as_ref().unwrap().previews.is_empty());
        assert_eq!(h.app.doc().scene, scene);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(h.app.presence.proposals.is_empty());
    }
    assert!(samples.windows(2).all(|p| p[0] != p[1]));
    for _ in 0..60 {
        if h.app.presence.agents[0].work.is_none() {
            break;
        }
        tick(&mut h);
    }
    assert!(h.app.presence.agents[0].work.is_none());
    let c = h.app.presence.conversations.get(&recipient).unwrap();
    assert_eq!(c.turns.iter().filter(|t| t.role == "user").count(), 1);
    assert_eq!(c.turns[0].text, "Please review this layout");
    assert_eq!(c.turns[0].author, "Sender");
    assert_eq!(h.app.presence.agents[0].relays, 1);
    for _ in 0..5 {
        tick(&mut h);
    }
    assert_eq!(
        h.app
            .presence
            .conversations
            .get(&recipient)
            .unwrap()
            .turns
            .len(),
        1
    );
    assert_eq!(h.app.doc().scene, scene);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(h.app.presence.proposals.is_empty());
    terminal_once(&mut h, &call, true);
    assert_eq!(
        h.app
            .presence
            .conversations
            .get(&recipient)
            .unwrap()
            .turns
            .iter()
            .filter(|t| t.role == "user")
            .count(),
        1
    );
    assert_eq!(h.app.presence.agents[0].relays, 1);
    assert_eq!(h.app.doc().scene, scene);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
#[test]
fn transient_hold_parks_relay_but_real_pause_cancels_before_contact() {
    let (mut h, sender, recipient) = setup();
    let call = begin(&mut h, &sender, &recipient);
    tick(&mut h);
    let a = &h.app.presence.agents[0];
    let before = a.position(&h.app.presence.roster).unwrap();
    let epoch = a.control_epoch;
    h.app
        .set_presence_paused(&sender, true, controls::PauseCause::InputHold)
        .unwrap();
    for _ in 0..5 {
        tick(&mut h);
    }
    assert_eq!(
        h.app.presence.agents[0].position(&h.app.presence.roster),
        Some(before)
    );
    assert_eq!(h.app.presence.agents[0].control_epoch, epoch);
    no_delivery(&h, &recipient);
    h.app
        .set_presence_paused(&sender, false, controls::PauseCause::InputHold)
        .unwrap();
    tick(&mut h);
    assert_ne!(
        h.app.presence.agents[0].position(&h.app.presence.roster),
        Some(before)
    );
    h.app
        .set_presence_paused(&sender, true, controls::PauseCause::Human)
        .unwrap();
    tick(&mut h);
    assert!(h.app.presence.agents[0].work.is_none());
    no_delivery(&h, &recipient);
    assert!(h.app.presence.agents[0].paused);
    terminal_once(&mut h, &call, false);
    no_delivery(&h, &recipient);
    h.app
        .set_presence_paused(&sender, false, controls::PauseCause::Human)
        .unwrap();
    for _ in 0..40 {
        tick(&mut h);
    }
    no_delivery(&h, &recipient);
}
#[test]
fn endpoint_request_grant_revoke_and_read_only_all_fence_unsent_relay() {
    for change in 0..3 {
        let (mut h, sender, recipient) = setup();
        let scene = h.app.doc().scene.clone();
        let depth = h.app.tab().journal.undo_depth();
        let call = begin(&mut h, &sender, &recipient);
        tick(&mut h);
        match change {
            0 => h.app.presence.agents[1].request = "replacement-task".into(),
            1 => h.app.presence.agents[0]
                .grant
                .as_mut()
                .unwrap()
                .conversation_ids
                .clear(),
            _ => h.app.tab_mut().read_only = true,
        }
        tick(&mut h);
        assert!(h.app.presence.agents[0].work.is_none());
        assert!(h
            .app
            .presence
            .conversations
            .get(&recipient)
            .is_none_or(|c| c.turns.iter().all(|t| t.role != "user")));
        assert_eq!(h.app.doc().scene, scene);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
        assert!(h.app.presence.proposals.is_empty());
        terminal_once(&mut h, &call, false);
        assert!(h
            .app
            .presence
            .conversations
            .get(&recipient)
            .is_none_or(|c| c.turns.iter().all(|t| t.role != "user")));
        assert_eq!(h.app.doc().scene, scene);
        assert_eq!(h.app.tab().journal.undo_depth(), depth);
    }
}
