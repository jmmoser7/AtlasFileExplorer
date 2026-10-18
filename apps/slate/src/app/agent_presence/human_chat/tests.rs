//! Real native-app adapters, TLS peers and independent on-disk histories.
use super::*;
use crate::app::tests::Harness;
use std::time::{Duration, Instant};

fn participant(name: &str) -> Harness {
    let mut h = Harness::new("presence_human_delivery");
    h.app.settings.presence_identity.id = atlas_agent::request_id();
    h.app.settings.presence_identity.name = name.into();
    h.app.settings.presence_contacts.clear();
    h.app.presence.conversations =
        conversation_store::ConversationStore::at_directory(h.base.join("messages"));
    h
}
fn tick(h: &mut Harness) {
    h.app.presence.conversations.poll();
    h.app.pump_presence_people(&h.ctx);
}
fn diagnostic(h: &Harness) -> String {
    let chat = &h.app.presence.human_chat;
    format!(
        "relay={:?}, disconnected={}, online={}, store={:?}, incoming={}, pending={}, outbound={}, acks={}",
        chat.setup.status,
        chat.client.as_ref().is_none_or(Client::is_disconnected),
        chat.online.len(),
        h.app.presence.conversations.status(),
        chat.incoming.len(),
        chat.pending.len(),
        chat.outbound.len(),
        chat.acks.len()
    )
}
fn until(stage: &str, mut step: impl FnMut() -> (bool, String)) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let (complete, diagnostic) = step();
        if complete {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "native direct-message adapter timed out during {stage}: {diagnostic}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn connect(a: &mut Harness, b: &mut Harness) -> Host {
    let host = Host::start("127.0.0.1:0".parse().unwrap()).unwrap();
    a.app.presence.human_chat.client =
        Some(Client::connect(host.invite(), a.app.local_chat_profile()).unwrap());
    b.app.presence.human_chat.client =
        Some(Client::connect(host.invite(), b.app.local_chat_profile()).unwrap());
    let mut server_failures = Vec::new();
    until("authenticated peer discovery and history load", || {
        tick(a);
        tick(b);
        server_failures.extend(host.errors().try_iter().take(16 - server_failures.len()));
        let complete = a
            .app
            .presence
            .human_chat
            .online
            .contains(&b.app.settings.presence_identity.id)
            && b.app
                .presence
                .human_chat
                .online
                .contains(&a.app.settings.presence_identity.id)
            && a.app.presence.conversations.status().is_none()
            && b.app.presence.conversations.status().is_none();
        (
            complete,
            format!(
                "Alice: {}; Bob: {}; server={server_failures:?}",
                diagnostic(a),
                diagnostic(b)
            ),
        )
    });
    host
}
fn status(h: &Harness, thread: &str) -> String {
    h.app
        .presence_conversation(thread)
        .and_then(|c| c.turns.last())
        .map(|t| t.status.clone())
        .unwrap_or_default()
}

#[test]
fn actual_two_app_delivery_waits_for_recipient_persistence_and_survives_reload() {
    let mut alice = participant("Alice");
    let mut bob = participant("Bob");
    let _host = connect(&mut alice, &mut bob);
    let outgoing = format!("human:{}", bob.app.settings.presence_identity.id);
    let incoming = format!("human:{}", alice.app.settings.presence_identity.id);
    alice
        .app
        .presence_send_chat(&outgoing, "Keep this message")
        .unwrap();

    // Bob's network client is live, but his app has not consumed or persisted
    // the incoming event. Relay acceptance must never become delivery here.
    until("sender persistence and relay acceptance", || {
        tick(&mut alice);
        (status(&alice, &outgoing) == "accepted", diagnostic(&alice))
    });
    assert_ne!(status(&alice, &outgoing), "delivered");
    assert!(bob.app.presence_conversation(&incoming).is_none());
    until("recipient persistence and delivery acknowledgement", || {
        tick(&mut alice);
        tick(&mut bob);
        (
            status(&alice, &outgoing) == "delivered",
            format!("Alice: {}; Bob: {}", diagnostic(&alice), diagnostic(&bob)),
        )
    });
    let received = bob
        .app
        .presence_conversation(&incoming)
        .unwrap()
        .turns
        .clone();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].text, "Keep this message");
    assert_eq!(received[0].role, "peer");
    assert_eq!(received[0].author, "Alice");

    // The Ack above is sufficient evidence that the independently loaded disk
    // history already contains the received row, without flushing on shutdown.
    let mut reopened =
        conversation_store::ConversationStore::at_directory(bob.base.join("messages"));
    until("independent recipient history reload", || {
        reopened.poll();
        (
            reopened.get(&incoming).is_some(),
            format!("store={:?}", reopened.status()),
        )
    });
    assert_eq!(reopened.get(&incoming).unwrap().turns, received);
    alice
        .app
        .presence_save_draft(&outgoing, "Do not discard this draft")
        .unwrap();
    alice.app.disconnect_people();
    assert!(alice
        .app
        .presence_send_chat(&outgoing, "Cannot send offline")
        .is_err());
    assert_eq!(
        alice.app.presence_conversation(&outgoing).unwrap().draft,
        "Do not discard this draft"
    );
    assert_eq!(
        alice
            .app
            .presence_conversation(&outgoing)
            .unwrap()
            .turns
            .len(),
        1
    );
}

#[test]
fn recipient_disk_failure_never_acknowledges_and_disconnect_restores_sender_draft() {
    let mut alice = participant("Alice");
    let mut bob = participant("Bob");
    let _host = connect(&mut alice, &mut bob);
    let blocked = bob.base.join("blocked-store");
    std::fs::write(
        &blocked,
        b"A file prevents creating the transcript directory",
    )
    .unwrap();
    bob.app.presence.conversations = conversation_store::ConversationStore::at_directory(blocked);
    until("blocked recipient store initial load", || {
        bob.app.presence.conversations.poll();
        (
            bob.app.presence.conversations.edit("probe").is_ok(),
            diagnostic(&bob),
        )
    });
    let outgoing = format!("human:{}", bob.app.settings.presence_identity.id);
    alice
        .app
        .presence_send_chat(&outgoing, "Retry after reconnecting")
        .unwrap();
    until("recipient write failure without acknowledgement", || {
        tick(&mut alice);
        tick(&mut bob);
        let complete = status(&alice, &outgoing) == "accepted"
            && bob
                .app
                .presence
                .conversations
                .status()
                .is_some_and(|s| s.contains("Could not save"))
            && bob.app.presence.human_chat.pending.is_empty();
        (
            complete,
            format!("Alice: {}; Bob: {}", diagnostic(&alice), diagnostic(&bob)),
        )
    });
    assert_ne!(status(&alice, &outgoing), "delivered");
    assert!(bob.app.presence.human_chat.acks.is_empty());
    let request = alice.app.presence_conversation(&outgoing).unwrap().turns[0]
        .request
        .clone();
    alice.app.disconnect_people();
    assert_eq!(status(&alice, &outgoing), "failed");
    let c = alice.app.presence_conversation(&outgoing).unwrap();
    assert_eq!(c.draft, "Retry after reconnecting");
    assert_eq!(c.turns[0].request, request);
}
