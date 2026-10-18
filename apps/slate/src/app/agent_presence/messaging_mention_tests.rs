//! Real app human SceneCmd routing and the existing conversation worker/mailbox.
//! No Codex/Cursor process or external provider is launched.
use super::super::tests::{add, host};
use super::super::*;
use serde_json::json;
use std::time::Duration;

fn setup() -> (crate::app::tests::Harness, String) {
    let mut h = host();
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("mentions.slate"));
    h.app.presence.conversations =
        conversation_store::ConversationStore::at_directory(h.base.join("messages"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.app.presence.conversations.edit("self").is_err() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let who = add(&mut h, "Dot");
    (h, who)
}
fn card(h: &mut crate::app::tests::Harness, text: &str) -> Vec<slate_doc::scene::SceneCmd> {
    let p = h
        .app
        .prepare_targeted_command(
            "board.objects.create_text",
            &[],
            &json!({"x":0,"y":0,"width":250,"height":100,"text":text}),
        )
        .unwrap();
    let cmds = p.cmds;
    assert!(h.app.commit_scene(cmds.clone()));
    cmds
}
fn until(h: &mut crate::app::tests::Harness, done: impl Fn(&SlateApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.app.presence.conversations.poll();
        h.app.pump_presence_mentions(true);
        if done(&h.app) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
#[test]
fn busy_board_mention_persists_then_submits_once_without_clobbering_draft() {
    let (mut h, who) = setup();
    h.app.presence.agents[0].inflight = true;
    h.app.presence.agents[0].request = "busy-task".into();
    h.app
        .presence
        .conversations
        .edit(&who)
        .unwrap()
        .set_draft("Human unfinished draft");
    let cmds = card(&mut h, "@Dot: organize these images");
    h.app.route_presence_mentions(&cmds); // same authored card never queues twice
    let c = h.app.presence.conversations.get(&who).unwrap();
    assert_eq!(c.turns.len(), 1);
    assert_eq!(c.turns[0].status, "queued");
    assert!(c.turns[0].board_origin.is_some());
    assert!(c.replay().is_empty());
    let queued = c.turns[0].request.clone();
    for _ in 0..20 {
        h.app.presence.conversations.poll();
        h.app.pump_presence_mentions(true);
    }
    assert_eq!(h.app.presence.agents[0].request, "busy-task");
    h.app.presence.agents[0].inflight = false;
    until(&mut h, |app| app.presence.agents[0].request == queued);
    assert!(h.app.presence.agents[0].inflight);
    for _ in 0..5 {
        h.app.pump_presence_mentions(true);
    }
    let c = h.app.presence.conversations.get(&who).unwrap();
    assert_eq!(c.turns.len(), 1);
    assert_eq!(c.turns[0].status, "sent");
    assert_eq!(c.turns[0].request, queued);
    assert_eq!(c.draft, "Human unfinished draft");
}
#[test]
fn temporary_input_hold_parks_but_explicit_pause_cancels_and_late_receipt_is_owned() {
    let (mut h, who) = setup();
    card(&mut h, "@Dot read this later");
    let epoch = h.app.presence.agents[0].control_epoch;
    h.app
        .set_presence_paused(&who, true, controls::PauseCause::InputHold)
        .unwrap();
    assert_eq!(h.app.presence.agents[0].control_epoch, epoch);
    for _ in 0..20 {
        h.app.presence.conversations.poll();
        h.app.pump_presence_mentions(true);
    }
    assert!(h.app.presence.agents[0].request.is_empty());
    assert_eq!(
        h.app.presence.conversations.get(&who).unwrap().turns[0].status,
        "queued"
    );
    h.app
        .presence
        .conversations
        .edit("human:peer")
        .unwrap()
        .push("user", "Human DM", "You", None, "human-dm", "pending");
    let human = h.app.presence.conversations.save_with_receipt("human:peer");
    h.app
        .set_presence_paused(&who, true, controls::PauseCause::Human)
        .unwrap();
    until(&mut h, |app| {
        app.presence.conversations.get(&who).unwrap().turns[0].status == "not-sent"
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut receipts = Vec::new();
    while !receipts.contains(&human) {
        h.app.presence.conversations.poll();
        h.app.pump_presence_mentions(true);
        receipts.extend(h.app.presence.conversations.take_receipts());
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(receipts, vec![human]);
    h.app
        .set_presence_paused(&who, false, controls::PauseCause::Human)
        .unwrap();
    for _ in 0..20 {
        h.app.presence.conversations.poll();
        h.app.pump_presence_mentions(true);
    }
    assert!(h.app.presence.agents[0].request.is_empty());
}
#[test]
fn source_edit_and_restart_never_resurrect_queued_execution() {
    let (mut h, who) = setup();
    let cmds = card(&mut h, "@Dot first message");
    let node = match &cmds[0] {
        slate_doc::scene::SceneCmd::Add { node, .. } => node.clone(),
        _ => unreachable!(),
    };
    let mut changed = node.clone();
    if let slate_doc::scene::NodeKind::Text(t) = &mut changed.kind {
        t.text = "No recipient".into();
    }
    assert!(h.app.commit_scene(vec![slate_doc::scene::SceneCmd::Patch {
        before: Box::new(node),
        after: Box::new(changed)
    }]));
    until(&mut h, |app| {
        app.presence.conversations.get(&who).unwrap().turns[0].status == "not-sent"
    });
    assert!(h.app.presence.agents[0].request.is_empty());
    card(&mut h, "@Dot queued at shutdown");
    let c = h.app.presence.conversations.edit(&who).unwrap();
    let index = c.turns.len() - 1;
    c.set_turn_status(index, "dispatching");
    let request = c.turns[index].request.clone();
    let receipt = h.app.presence.conversations.save_with_receipt(&who);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = false;
    while !seen {
        h.app.presence.conversations.poll();
        seen = h
            .app
            .presence
            .conversations
            .take_receipts()
            .contains(&receipt);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let old = std::mem::replace(
        &mut h.app.presence.conversations,
        conversation_store::ConversationStore::default(),
    );
    drop(old);
    let mut restored = conversation_store::ConversationStore::at_directory(h.base.join("messages"));
    while restored.edit(&who).is_err() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let c = restored.get(&who).unwrap();
    let t = &c.turns[c.request_index(&request).unwrap()];
    assert_eq!(t.status, "not-sent");
    assert_eq!(t.text, "queued at shutdown");
    assert!(t.board_origin.is_some());
    assert!(c
        .turns
        .iter()
        .any(|t| t.role == "system" && t.text.contains("delivery is uncertain")));
    assert!(c.replay().iter().all(|t| t.text != "queued at shutdown"));
}

#[test]
fn stable_recipient_survives_cosmetic_rename_and_utf8_envelope_is_shared() {
    let (mut h, who) = setup();
    h.app.presence.agents[0].inflight = true;
    h.app.presence.agents[0].request = "busy-task".into();
    card(&mut h, "@Dot: keep the original stable recipient");
    h.app
        .apply_presence_action(
            &h.ctx,
            chrome::PresenceAction::Rename {
                id: who.clone(),
                name: "Renamed".into(),
            },
        )
        .unwrap();
    h.app.pump_presence_mentions(true);
    assert_eq!(
        h.app.presence.conversations.get(&who).unwrap().turns[0].status,
        "queued"
    );
    let request = h.app.presence.conversations.get(&who).unwrap().turns[0]
        .request
        .clone();
    h.app.presence.agents[0].inflight = false;
    until(&mut h, |app| app.presence.agents[0].request == request);
    assert_eq!(
        h.app.presence.conversations.get(&who).unwrap().turns.len(),
        1
    );
    let exact = "🟢".repeat(submission::MESSAGE_BYTES / 4);
    assert!(submission::validate_message_text(&exact).is_ok());
    assert!(submission::validate_message_text(&(exact + "x")).is_err());
    assert!(submission::validate_message_text(&" ".repeat(submission::MESSAGE_BYTES + 1)).is_err());
}
#[test]
fn invalid_body_and_oversize_address_card_never_enqueue_consume_hash_or_send() {
    let (mut h, who) = setup();
    let mut p = h
        .app
        .prepare_targeted_command(
            "board.objects.create_text",
            &[],
            &json!({"x":0,"y":0,"width":250,"height":100,"text":"placeholder"}),
        )
        .unwrap();
    let node = match &mut p.cmds[0] {
        slate_doc::scene::SceneCmd::Add { node, .. } => {
            if let slate_doc::scene::NodeKind::Text(t) = &mut node.kind {
                t.text = " ".repeat(submission::MESSAGE_BYTES) + "@Dot x";
            }
            node.id
        }
        _ => unreachable!(),
    };
    assert!(h.app.commit_scene(p.cmds));
    assert!(!h.app.presence.mention_hashes.contains_key(&node));
    assert!(h
        .app
        .presence
        .conversations
        .get(&who)
        .is_none_or(|c| c.turns.is_empty()));
    assert!(h.app.queue_presence_mention(&who, node, 0, "   ").is_err());
    assert!(h
        .app
        .queue_presence_mention(&who, node, 0, &"x".repeat(submission::MESSAGE_BYTES + 1))
        .is_err());
    assert!(h
        .app
        .queue_presence_mention(&who, node, 0, "valid body but oversized source")
        .is_err());
    assert!(h
        .app
        .presence
        .conversations
        .get(&who)
        .is_none_or(|c| c.turns.is_empty()));
    assert!(h.app.presence.agents[0].request.is_empty());
}

#[test]
fn admitted_card_edited_over_envelope_cancels_before_hash_or_provider_submission() {
    let (mut h, who) = setup();
    h.app.presence.agents[0].inflight = true;
    h.app.presence.agents[0].request = "busy-task".into();
    let cmds = card(&mut h, "@Dot bounded original");
    let before = match &cmds[0] {
        slate_doc::scene::SceneCmd::Add { node, .. } => node.clone(),
        _ => unreachable!(),
    };
    let mut after = before.clone();
    if let slate_doc::scene::NodeKind::Text(t) = &mut after.kind {
        t.text = " ".repeat(submission::MESSAGE_BYTES + 1) + "@Dot changed";
    }
    assert!(h.app.commit_scene(vec![slate_doc::scene::SceneCmd::Patch {
        before: Box::new(before),
        after: Box::new(after)
    }]));
    h.app.pump_presence_mentions(true);
    let c = h.app.presence.conversations.get(&who).unwrap();
    assert_eq!(c.turns.iter().filter(|t| t.role == "user").count(), 1);
    assert_eq!(c.turns[0].status, "not-sent");
    assert_eq!(c.turns[0].text, "bounded original");
    assert!(c
        .turns
        .iter()
        .any(|t| t.role == "system" && t.text.contains("exceeds 32768")));
    assert_eq!(h.app.presence.agents[0].request, "busy-task");
}

#[test]
fn actual_queue_persistence_failure_retains_not_sent_text_without_a_request() {
    let (mut h, who) = setup();
    let blocked = h.base.join("not-a-conversation-directory");
    std::fs::write(&blocked, b"Deliberately occupies the directory").unwrap();
    h.app.presence.conversations = conversation_store::ConversationStore::at_directory(blocked);
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.app.presence.conversations.edit(&who).is_err() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let mailbox = h.app.presence.agents[0].dir.join("request.json");
    assert!(!mailbox.exists());
    card(&mut h, "@Dot keep this unsent message");
    until(&mut h, |app| {
        app.presence.conversations.get(&who).unwrap().turns[0].status == "not-sent"
    });
    let c = h.app.presence.conversations.get(&who).unwrap();
    assert_eq!(c.turns.iter().filter(|t| t.role == "user").count(), 1);
    assert_eq!(c.turns[0].text, "keep this unsent message");
    assert!(c
        .turns
        .iter()
        .any(|t| t.role == "system" && t.text.contains("Could not persist")));
    let a = &h.app.presence.agents[0];
    assert!(a.request.is_empty() && !a.inflight && a.runtime.is_none() && a.cursor.is_none());
    assert!(!mailbox.exists());
    for _ in 0..10 {
        h.app.presence.conversations.poll();
        h.app.pump_presence_mentions(true);
    }
    assert!(h.app.presence.agents[0].request.is_empty());
    assert!(!mailbox.exists());
}
#[test]
fn queued_message_tab_binding_revocation_retirement_and_handoff_cancellation_table() {
    for reason in ["tab", "binding", "revoke", "retire", "handoff"] {
        let (mut h, who) = setup();
        h.app.presence.agents[0].inflight = true;
        h.app.presence.agents[0].request = "original-busy-task".into();
        card(&mut h, "@Dot never cross an authority fence");
        let queued = h.app.presence.conversations.get(&who).unwrap().turns[0]
            .request
            .clone();
        let mailbox = h.app.presence.agents[0].dir.join("request.json");
        match reason {
            "tab" => h.app.tab_mut().id += 1,
            "binding" => h.app.presence.agents[0].binding.session_id = atlas_agent::request_id(),
            "revoke" => h.app.stop_presence(&who, true).unwrap(),
            "retire" => h.app.retire_presence(&who, "bow").unwrap(),
            _ => {
                // Availability fixture only; after-task selection launches nothing.
                h.app.set_agent_programs_for_test(&["cursor"]);
                h.app
                    .presence_set_harness(&who, "cursor", "after-task")
                    .unwrap();
                assert!(h.app.presence.agents[0].pending_harness.is_some());
            }
        }
        until(&mut h, |app| {
            app.presence.conversations.get(&who).unwrap().turns[0].status == "not-sent"
        });
        let c = h.app.presence.conversations.get(&who).unwrap();
        assert_eq!(
            c.turns.iter().filter(|t| t.role == "user").count(),
            1,
            "{reason}"
        );
        assert_eq!(c.turns[0].text, "never cross an authority fence");
        let a = &h.app.presence.agents[0];
        assert_ne!(a.request, queued, "{reason}");
        assert!(a.runtime.is_none() && a.cursor.is_none(), "{reason}");
        assert!(!mailbox.exists(), "{reason}");
    }
}
