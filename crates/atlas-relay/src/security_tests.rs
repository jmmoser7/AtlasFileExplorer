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

// Every wait observes attack traffic as well as its expected event.
fn wait_for(client: &crate::Client, predicate: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let event = client
            .events()
            .recv_deadline(deadline)
            .expect("Expected relay event before the absolute deadline");
        match &event {
            Event::Incoming { message_id, .. }
            | Event::Accepted { message_id }
            | Event::Ack { message_id, .. } => assert_ne!(message_id, "attack"),
            Event::Error { .. } | Event::Disconnected => {
                panic!("Healthy relay client terminated unexpectedly: {event:?}")
            }
            _ => {}
        }
        if predicate(&event) {
            return event;
        }
    }
}

fn connection_reset(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::ConnectionReset
        && (!cfg!(windows) || error.raw_os_error() == Some(10054))
}

#[test]
fn hostile_network_frame_and_forged_sender_are_rejected_by_server() {
    let host = Host::start("127.0.0.1:0".parse().unwrap()).unwrap();
    let observer =
        crate::Client::connect(host.invite(), Profile::new("observer", "Observer")).unwrap();
    wait_for(&observer, |event| matches!(event, Event::Connected { .. }));
    for (id, payload) in [
        ("oversize", "x".repeat(MAX_FRAME_BYTES + 1)),
        ("forged", r#"{"type":"send","sender":"someone-else","recipient":"observer","message_id":"attack","text":"fake"}"#.into()),
    ] {
        let invite = host.invite();
        let mut socket = client::open(&invite).unwrap();
        socket.send(Message::Text(serde_json::to_string(&hello(&invite, id)).unwrap().into())).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "Authentication timed out for {id}");
            match socket.read() {
                Ok(Message::Text(text)) if matches!(serde_json::from_str::<Event>(&text), Ok(Event::Connected { .. })) => break,
                Ok(_) => {}
                Err(e) if wire::retryable(&e) => {}
                Err(e) => panic!("Unexpected {id} authentication read error: {e:?}"),
            }
        }
        wait_for(&observer, |event| {
            matches!(event, Event::Peers { peers } if peers.iter().any(|peer| peer.id == id))
        });
        // The server can reject the frame before its client-side write completes.
        match socket.send(Message::Text(payload.into())) {
            Ok(()) => {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    assert!(Instant::now() < deadline, "Host left invalid {id} connection alive during rejection read");
                    match socket.read() {
                        Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => break,
                        Err(tungstenite::Error::Io(error)) if connection_reset(&error) => break,
                        Err(tungstenite::Error::Io(error))
                            if error.kind() == std::io::ErrorKind::UnexpectedEof
                                && error.to_string().starts_with(
                                    "peer closed connection without sending TLS close_notify: ",
                                ) => break,
                        Err(tungstenite::Error::Protocol(
                            tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
                        )) => break,
                        Ok(Message::Text(text)) => {
                            let event: Event = serde_json::from_str(&text).unwrap();
                            assert!(
                                matches!(event, Event::Peers { .. } | Event::Profile { .. } | Event::Error { .. }),
                                "Invalid {id} connection emitted unexpected rejection event: {event:?}"
                            );
                        }
                        Ok(Message::Ping(_) | Message::Pong(_)) => {}
                        Ok(other) => panic!("Unexpected {id} hostile-connection read frame: {other:?}"),
                        Err(error) if wire::retryable(&error) => {}
                        Err(error) => panic!("Unexpected {id} hostile-connection read error: {error:?}"),
                    }
                }
            }
            Err(tungstenite::Error::Io(error)) if connection_reset(&error) => {}
            Err(error) => panic!("Unexpected {id} hostile-frame write error: {error:?}"),
        }
        wait_for(&observer, |event| {
            matches!(event, Event::Peers { peers } if !peers.iter().any(|peer| peer.id == id))
        });

        // Rejection must leave this same host able to deliver and acknowledge.
        let profile = Profile::new(format!("canary-{id}"), format!("Canary {id}"));
        let canary = crate::Client::connect(host.invite(), profile.clone()).unwrap();
        let connected = wait_for(&canary, |event| matches!(event, Event::Connected { .. }));
        assert!(matches!(connected, Event::Connected { peers } if peers.iter().any(|peer| peer.id == "observer")));
        let message_id = format!("verified-{id}");
        let text = format!("Host remains live after {id}");
        canary.send("observer", &message_id, &text).unwrap();
        let received = wait_for(&observer, |event| matches!(event, Event::Incoming { .. }));
        assert_eq!(received, Event::Incoming {
            from: profile,
            message_id: message_id.clone(),
            text,
        });
        observer.ack(&message_id).unwrap();
        let ack = wait_for(&canary, |event| matches!(event, Event::Ack { .. }));
        assert_eq!(ack, Event::Ack {
            message_id,
            recipient: "observer".into(),
        });
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
