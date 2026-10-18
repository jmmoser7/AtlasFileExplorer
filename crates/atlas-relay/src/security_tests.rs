use crate::{client, wire, Event, Host, Profile, MAX_FRAME_BYTES};
use std::time::{Duration, Instant};
use tungstenite::Message;

fn hello(invite: &crate::Invite, id: &str) -> wire::Command {
    wire::Command::Hello {
        version: 1,
        room: invite.room.clone(),
        credential: invite.credential.clone(),
        profile: Profile::new(id, id),
    }
}

#[test]
fn hostile_network_frame_and_forged_sender_are_rejected_by_server() {
    let host = Host::start("127.0.0.1:0".parse().unwrap()).unwrap();
    for (id, payload) in [
        ("oversize", "x".repeat(MAX_FRAME_BYTES + 1)),
        ("forged", r#"{"type":"send","sender":"someone-else","recipient":"forged","message_id":"attack","text":"fake"}"#.into()),
    ] {
        let invite = host.invite();
        let mut socket = client::open(&invite).unwrap();
        socket.send(Message::Text(serde_json::to_string(&hello(&invite, id)).unwrap().into())).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "Authentication timed out");
            match socket.read() {
                Ok(Message::Text(text)) if matches!(serde_json::from_str::<Event>(&text), Ok(Event::Connected { .. })) => break,
                Ok(_) => {}
                Err(e) if wire::retryable(&e) => {}
                Err(e) => panic!("Unexpected transport error: {e}"),
            }
        }
        socket.send(Message::Text(payload.into())).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "Host left an invalid connection alive");
            match socket.read() {
                Ok(Message::Close(_)) => break,
                Ok(_) => {}
                Err(e) if wire::retryable(&e) => {}
                Err(_) => break,
            }
        }
    }
}

#[test]
fn slow_handshake_has_an_absolute_deadline() {
    use std::{io::Read, net::TcpStream};
    let host = Host::start("127.0.0.1:0".parse().unwrap()).unwrap();
    let mut socket = TcpStream::connect(host.address()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    let start = Instant::now();
    let result = socket.read(&mut [0]);
    assert!(
        matches!(result, Ok(0))
            || result
                .as_ref()
                .is_err_and(|e| e.kind() != std::io::ErrorKind::TimedOut)
    );
    assert!(start.elapsed() < Duration::from_secs(6));
}

#[test]
fn maximum_portraits_and_escaped_names_fit_a_complete_room_frame() {
    let peers: Vec<_> = (0..crate::MAX_PEERS)
        .map(|i| {
            // Longest allowed ID/name, worst JSON escaping and byte-array width.
            let mut profile = Profile::new(format!("p{i:0>127}"), "\0".repeat(160));
            profile.avatar = Some(vec![255; crate::MAX_AVATAR_BYTES]);
            profile.validate().unwrap();
            profile
        })
        .collect();
    for event in [
        Event::Connected {
            peers: peers.clone(),
        },
        Event::Peers {
            peers: peers.clone(),
        },
    ] {
        assert!(serde_json::to_vec(&event).unwrap().len() < crate::MAX_FRAME_BYTES);
    }
    let incoming = Event::Incoming {
        from: peers[0].clone(),
        message_id: "m".repeat(128),
        text: "\0".repeat(crate::MAX_TEXT_BYTES),
    };
    assert!(serde_json::to_vec(&incoming).unwrap().len() < crate::MAX_FRAME_BYTES);
    let mut oversized = peers[0].clone();
    oversized.avatar.as_mut().unwrap().push(255);
    assert!(oversized.validate().is_err());
    let legacy: Profile =
        serde_json::from_str(r#"{"id":"legacy","name":"Legacy profile"}"#).unwrap();
    assert!(legacy.avatar.is_none());
    assert!(!serde_json::to_string(&legacy).unwrap().contains("avatar"));
}
