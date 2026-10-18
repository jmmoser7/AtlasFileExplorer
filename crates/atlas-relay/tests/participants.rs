use atlas_relay::{Client, Event, Host, Invite, Profile, MAX_TEXT_BYTES};
use std::time::{Duration, Instant};

fn profile(id: &str) -> Profile {
    Profile::new(id, format!("Person {id}"))
}
fn start_host() -> Host {
    Host::start("127.0.0.1:0".parse().unwrap()).unwrap()
}
fn connect(host: &Host, id: &str) -> Client {
    let c = Client::connect(host.invite(), profile(id)).unwrap();
    wait_with_host(&c, Some(host), |e| matches!(e, Event::Connected { .. }));
    c
}
fn wait(client: &Client, predicate: impl Fn(&Event) -> bool) -> Event {
    wait_with_host(client, None, predicate)
}
fn host_failures(host: Option<&Host>) -> Vec<String> {
    let Some(host) = host else {
        return Vec::new();
    };
    let mut errors: Vec<_> = host.errors().try_iter().take(16).collect();
    if errors.is_empty() {
        if let Ok(error) = host.errors().recv_timeout(Duration::from_millis(50)) {
            errors.push(error);
        }
    }
    errors
}
fn wait_with_host(
    client: &Client,
    host: Option<&Host>,
    predicate: impl Fn(&Event) -> bool,
) -> Event {
    let end = Instant::now() + Duration::from_secs(8);
    while Instant::now() < end {
        if let Ok(event) = client.events().recv_timeout(Duration::from_millis(100)) {
            if predicate(&event) {
                return event;
            }
            assert!(
                !matches!(event, Event::Error { .. } | Event::Disconnected),
                "Unexpected relay event: {event:?}; server={:?}",
                host_failures(host)
            );
        }
    }
    panic!(
        "Timed out waiting for relay event; server={:?}",
        host_failures(host)
    );
}
fn ensure_no(client: &Client, predicate: impl Fn(&Event) -> bool) {
    let end = Instant::now() + Duration::from_millis(180);
    while Instant::now() < end {
        if let Ok(event) = client.events().recv_timeout(Duration::from_millis(20)) {
            assert!(!predicate(&event), "Unexpected event: {event:?}");
        }
    }
}

#[test]
fn encrypted_two_party_delivery_requires_recipient_persistence_ack_and_deduplicates() {
    let host = start_host();
    let alice = connect(&host, "alice");
    let bob = connect(&host, "bob");
    alice.send("bob", "message-1", "Hello from Alice").unwrap();
    let incoming = wait(&bob, |e| matches!(e, Event::Incoming { .. }));
    assert_eq!(
        incoming,
        Event::Incoming {
            from: profile("alice"),
            message_id: "message-1".into(),
            text: "Hello from Alice".into()
        }
    );
    wait(
        &alice,
        |e| matches!(e, Event::Accepted { message_id } if message_id == "message-1"),
    );
    ensure_no(&alice, |e| matches!(e, Event::Ack { .. }));
    // The sender cannot manufacture the recipient's persistence receipt.
    alice.ack("message-1").unwrap();
    ensure_no(&alice, |e| matches!(e, Event::Ack { .. }));
    bob.ack("message-1").unwrap();
    wait(
        &alice,
        |e| matches!(e, Event::Ack { message_id, recipient } if message_id == "message-1" && recipient == "bob"),
    );
    alice.send("bob", "message-1", "Hello from Alice").unwrap();
    wait(&alice, |e| matches!(e, Event::Ack { .. }));
    ensure_no(&bob, |e| matches!(e, Event::Incoming { .. }));
    // Message IDs are room-unique; collision does not impersonate a sender.
    bob.send("alice", "message-1", "Forged reuse").unwrap();
    wait(&bob, |e| matches!(e, Event::Rejected { .. }));
    assert!(alice
        .send("bob", "huge", &"x".repeat(MAX_TEXT_BYTES + 1))
        .is_err());
}

#[test]
fn profile_identity_is_bound_and_departure_expires_live_membership() {
    let host = start_host();
    let alice = connect(&host, "alice");
    let bob = connect(&host, "bob");
    let mut updated = Profile::new("bob", "Bobby");
    updated.avatar = Some(vec![1, 2, 3]);
    bob.update_profile(updated).unwrap();
    wait(
        &alice,
        |e| matches!(e, Event::Profile { profile } if profile.id == "bob" && profile.name == "Bobby" && profile.avatar.as_deref() == Some(&[1, 2, 3])),
    );
    alice
        .send("bob", "pending", "Persist before receipt")
        .unwrap();
    wait(&bob, |e| matches!(e, Event::Incoming { .. }));
    drop(bob);
    wait(
        &alice,
        |e| matches!(e, Event::Rejected { message_id, .. } if message_id == "pending"),
    );
    wait(
        &alice,
        |e| matches!(e, Event::Peers { peers } if peers.len() == 1 && peers[0].id == "alice"),
    );
    alice
        .send("bob", "offline", "Do not fake delivery")
        .unwrap();
    wait(
        &alice,
        |e| matches!(e, Event::Rejected { message_id, .. } if message_id == "offline"),
    );
    // A new connection can take its own prior ephemeral membership again.
    let bob = connect(&host, "bob");
    alice
        .send("bob", "pending", "Persist before receipt")
        .unwrap();
    wait(
        &bob,
        |e| matches!(e, Event::Incoming { message_id, .. } if message_id == "pending"),
    );
    bob.ack("pending").unwrap();
    wait(
        &alice,
        |e| matches!(e, Event::Ack { message_id, .. } if message_id == "pending"),
    );
}

#[test]
fn wrong_room_credential_and_wrong_tls_pin_cannot_join() {
    let host = start_host();
    let mut invite: serde_json::Value = serde_json::from_str(
        host.invite()
            .encode()
            .unwrap()
            .strip_prefix("slate-participant-v1:")
            .unwrap(),
    )
    .unwrap();
    invite["credential"] = serde_json::Value::String("0".repeat(64));
    let invite = Invite::decode(&format!("slate-participant-v1:{invite}")).unwrap();
    let bad = Client::connect(invite, profile("bad")).unwrap();
    let e = bad.events().recv_timeout(Duration::from_secs(8)).unwrap();
    assert!(matches!(e, Event::Error { .. }));
    let another = start_host();
    let wrong_pin = another.invite().with_address(host.address()).unwrap();
    let bad = Client::connect(wrong_pin, profile("bad-pin")).unwrap();
    let e = bad.events().recv_timeout(Duration::from_secs(8)).unwrap();
    assert!(matches!(e, Event::Error { .. }));
    let _valid = connect(&host, "valid");
}

#[test]
fn duplicate_live_identity_is_refused_and_debug_never_contains_invite_credential() {
    let host = start_host();
    let _alice = connect(&host, "alice");
    let duplicate = Client::connect(host.invite(), profile("alice")).unwrap();
    assert!(matches!(
        duplicate
            .events()
            .recv_timeout(Duration::from_secs(8))
            .unwrap(),
        Event::Error { .. }
    ));
    let raw = host.invite().encode().unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(raw.strip_prefix("slate-participant-v1:").unwrap()).unwrap();
    let credential = parsed["credential"].as_str().unwrap();
    assert!(!format!("{:?}", host.invite()).contains(credential));
    let mut altered = parsed;
    altered["certificate_sha256"] = "f".repeat(64).into();
    assert!(Invite::decode(&format!("slate-participant-v1:{altered}")).is_err());
}
