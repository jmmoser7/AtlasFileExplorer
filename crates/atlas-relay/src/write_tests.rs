//! Real TLS/WebSocket encoding with deterministic socket write interruptions.
use super::*;
use std::io::{self, Cursor, Read, Write};
use tungstenite::protocol::Role;

struct InterruptedIo {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
    fault: Option<io::ErrorKind>,
    prefix: usize,
    attempts: usize,
    stop_after: Option<(usize, Arc<AtomicBool>)>,
}

impl InterruptedIo {
    fn blocked(kind: io::ErrorKind, prefix: usize) -> Self {
        Self {
            input: Cursor::new(Vec::new()),
            output: Vec::new(),
            fault: Some(kind),
            prefix,
            attempts: 0,
            stop_after: None,
        }
    }
}

impl Read for InterruptedIo {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.input.read(bytes)?;
        if count == 0 {
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            Ok(count)
        }
    }
}

impl Write for InterruptedIo {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.attempts += 1;
        if let Some((_, stop)) = self
            .stop_after
            .as_ref()
            .filter(|(limit, _)| self.attempts >= *limit)
        {
            stop.store(true, Ordering::Release);
        }
        if let Some(kind) = self.fault {
            if self.prefix == 0 {
                return Err(kind.into());
            }
            let count = self.prefix.min(bytes.len());
            self.output.extend_from_slice(&bytes[..count]);
            self.prefix -= count;
            return Ok(count);
        }
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn established_tls() -> (rustls::ClientConnection, rustls::ServerConnection) {
    let (invite, config) = tls::server("127.0.0.1:1".parse().unwrap()).unwrap();
    let mut client = tls::client(&invite).unwrap();
    let mut server = rustls::ServerConnection::new(config).unwrap();
    for _ in 0..16 {
        let mut bytes = Vec::new();
        client.write_tls(&mut bytes).unwrap();
        if !bytes.is_empty() {
            server.read_tls(&mut Cursor::new(bytes)).unwrap();
            server.process_new_packets().unwrap();
        }
        let mut bytes = Vec::new();
        server.write_tls(&mut bytes).unwrap();
        if !bytes.is_empty() {
            client.read_tls(&mut Cursor::new(bytes)).unwrap();
            client.process_new_packets().unwrap();
        }
        if !client.is_handshaking()
            && !server.is_handshaking()
            && !client.wants_write()
            && !server.wants_write()
        {
            return (client, server);
        }
    }
    panic!("In-memory TLS handshake exceeded its bounded exchanges");
}

fn resumable_write(kind: io::ErrorKind, prefix: usize) {
    let (client, server) = established_tls();
    let stream = rustls::StreamOwned::new(client, InterruptedIo::blocked(kind, prefix));
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, Some(wire::config()));
    let result = write(
        &mut socket,
        &wire::Command::Send {
            recipient: "bob".into(),
            message_id: "persist-first".into(),
            text: "exactly once".into(),
        },
    );
    assert_eq!(socket.get_ref().sock.output.len(), prefix);
    // Rustls has accepted plaintext even though encrypted output is blocked.
    // Resume the buffered write; never send the original command again.
    socket.get_mut().sock.fault = None;
    assert!(wire::flush(&mut socket).unwrap());
    write(
        &mut socket,
        &wire::Command::Ack {
            message_id: "persist-first".into(),
        },
    )
    .unwrap();
    let mut input = InterruptedIo::blocked(io::ErrorKind::WouldBlock, 0);
    input.fault = None;
    input.input = Cursor::new(std::mem::take(&mut socket.get_mut().sock.output));
    let stream = rustls::StreamOwned::new(server, input);
    let mut received = WebSocket::from_raw_socket(stream, Role::Server, Some(wire::config()));
    let Message::Text(text) = received.read().unwrap() else {
        panic!("Expected one Send frame");
    };
    assert!(matches!(
        serde_json::from_str::<wire::Command>(&text).unwrap(),
        wire::Command::Send { recipient, message_id, text }
            if recipient == "bob" && message_id == "persist-first" && text == "exactly once"
    ));
    let Message::Text(text) = received.read().unwrap() else {
        panic!("Expected one Ack frame");
    };
    assert!(matches!(
        serde_json::from_str::<wire::Command>(&text).unwrap(),
        wire::Command::Ack { message_id } if message_id == "persist-first"
    ));
    assert!(matches!(received.read(), Err(error) if wire::retryable(&error)));
    assert!(
        matches!(result, Ok(false)),
        "A buffered command must remain pending after {kind:?} and {prefix} encrypted bytes: {result:?}"
    );
}

#[test]
fn buffered_tls_write_survives_would_block_without_duplicate_send_or_ack() {
    resumable_write(io::ErrorKind::WouldBlock, 0);
}

#[test]
fn partial_tls_write_survives_would_block_without_duplicate_send_or_ack() {
    resumable_write(io::ErrorKind::WouldBlock, 7);
}

#[test]
fn buffered_tls_write_survives_poll_timeout_without_duplicate_send_or_ack() {
    resumable_write(io::ErrorKind::TimedOut, 0);
}

#[test]
fn partial_tls_write_survives_poll_timeout_without_duplicate_send_or_ack() {
    resumable_write(io::ErrorKind::TimedOut, 7);
}

#[test]
fn buffered_tls_write_still_reports_terminal_socket_failure() {
    let (client, _) = established_tls();
    let stream =
        rustls::StreamOwned::new(client, InterruptedIo::blocked(io::ErrorKind::BrokenPipe, 7));
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, Some(wire::config()));
    assert!(write(
        &mut socket,
        &wire::Command::Ack {
            message_id: "persist-first".into(),
        },
    )
    .is_err());
}

#[test]
fn host_tls_events_resume_once_after_partial_write_and_flush_interruptions() {
    for kind in [io::ErrorKind::WouldBlock, io::ErrorKind::TimedOut] {
        let (client, server) = established_tls();
        let stream = rustls::StreamOwned::new(server, InterruptedIo::blocked(kind, 7));
        let mut socket = WebSocket::from_raw_socket(stream, Role::Server, Some(wire::config()));
        let incoming = Event::Incoming {
            from: Profile::new("alice", "Alice"),
            message_id: "persist-first".into(),
            text: "exactly once".into(),
        };
        assert!(!crate::host::write(&mut socket, &incoming).unwrap());
        assert!(!wire::flush(&mut socket).unwrap());
        assert_eq!(socket.get_ref().sock.output.len(), 7);
        socket.get_mut().sock.fault = None;
        assert!(wire::flush(&mut socket).unwrap());
        let ack = Event::Ack {
            message_id: "persist-first".into(),
            recipient: "bob".into(),
        };
        assert!(crate::host::write(&mut socket, &ack).unwrap());
        let mut input = InterruptedIo::blocked(io::ErrorKind::WouldBlock, 0);
        input.fault = None;
        input.input = Cursor::new(std::mem::take(&mut socket.get_mut().sock.output));
        let stream = rustls::StreamOwned::new(client, input);
        let mut received = WebSocket::from_raw_socket(stream, Role::Client, Some(wire::config()));
        for expected in [incoming, ack] {
            let Message::Text(text) = received.read().unwrap() else {
                panic!("Expected one relay event frame");
            };
            assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), expected);
        }
        assert!(matches!(received.read(), Err(error) if wire::retryable(&error)));
    }
}

#[test]
fn stalled_host_keeps_remaining_events_in_bounded_queue_and_observes_stop() {
    for kind in [io::ErrorKind::WouldBlock, io::ErrorKind::TimedOut] {
        let (_, server) = established_tls();
        let stop = Arc::new(AtomicBool::new(false));
        let mut io = InterruptedIo::blocked(kind, 7);
        io.stop_after = Some((8, stop.clone()));
        let stream = rustls::StreamOwned::new(server, io);
        let mut socket = WebSocket::from_raw_socket(stream, Role::Server, Some(wire::config()));
        let (sender, events) = bounded(QUEUE);
        for index in 0..QUEUE {
            sender
                .try_send(Event::Ack {
                    message_id: format!("persist-{index}"),
                    recipient: "bob".into(),
                })
                .unwrap();
        }
        let remaining = events.clone();
        let (input, _room) = bounded(QUEUE);
        crate::host::exchange(&mut socket, events, &input, &stop, 1).unwrap();
        assert!(stop.load(Ordering::Acquire));
        assert_eq!(remaining.len(), QUEUE - 1);
        assert_eq!(socket.get_ref().sock.output.len(), 7);
        assert!(socket.get_ref().sock.attempts < 16);
    }
}

#[test]
fn websocket_capacity_failure_is_not_mistaken_for_an_accepted_frame() {
    let (client, _) = established_tls();
    let mut io = InterruptedIo::blocked(io::ErrorKind::WouldBlock, 0);
    io.fault = None;
    let stream = rustls::StreamOwned::new(client, io);
    let mut socket = WebSocket::from_raw_socket(stream, Role::Client, Some(wire::config()));
    assert!(wire::send(
        &mut socket,
        Message::Binary(vec![0; crate::MAX_FRAME_BYTES * 3].into())
    )
    .is_err());
    assert!(socket.get_ref().sock.output.is_empty());
}
