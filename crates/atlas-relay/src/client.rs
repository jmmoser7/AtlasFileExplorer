use crate::{
    channel_error, tls, validate_message, wire, Event, Invite, Profile, RelayError, QUEUE,
};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

/// A UI-safe handle. Connect, TLS, WebSocket and all reads/writes run on one
/// worker. Dropping/disconnecting never joins or blocks the UI thread.
pub struct Client {
    commands: Sender<wire::Command>,
    events: Receiver<Event>,
    stopped: Arc<AtomicBool>,
}

impl Client {
    pub fn connect(invite: Invite, profile: Profile) -> Result<Self, RelayError> {
        invite.validate()?;
        profile.validate()?;
        let (commands, receiver) = bounded(QUEUE);
        let (sender, events) = bounded(QUEUE);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        std::thread::Builder::new()
            .name("atlas-relay-client".into())
            .spawn(move || {
                if let Err(error) = run(invite, profile, receiver, &sender, &stop) {
                    let _ = sender.try_send(Event::Error {
                        message: error.to_string(),
                    });
                }
                let _ = sender.try_send(Event::Disconnected);
                stop.store(true, Ordering::Release);
            })?;
        Ok(Self {
            commands,
            events,
            stopped,
        })
    }

    pub fn events(&self) -> &Receiver<Event> {
        &self.events
    }
    pub fn is_disconnected(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    pub fn send(&self, recipient: &str, message_id: &str, text: &str) -> Result<(), RelayError> {
        validate_message(recipient, message_id, text)?;
        self.enqueue(wire::Command::Send {
            recipient: recipient.into(),
            message_id: message_id.into(),
            text: text.into(),
        })
    }

    /// Call only after the incoming message has reached the app's durable
    /// history. A crash before persistence must never produce delivery UI.
    pub fn ack(&self, message_id: &str) -> Result<(), RelayError> {
        if !crate::valid_id(message_id) {
            return Err(RelayError::Invalid("Invalid message identifier"));
        }
        self.enqueue(wire::Command::Ack {
            message_id: message_id.into(),
        })
    }

    pub fn update_profile(&self, profile: Profile) -> Result<(), RelayError> {
        profile.validate()?;
        self.enqueue(wire::Command::UpdateProfile { profile })
    }

    pub fn disconnect(&self) {
        self.stopped.store(true, Ordering::Release);
    }

    fn enqueue(&self, command: wire::Command) -> Result<(), RelayError> {
        if self.is_disconnected() {
            return Err(RelayError::Disconnected);
        }
        self.commands.try_send(command).map_err(channel_error)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.disconnect();
    }
}

type Socket = WebSocket<rustls::StreamOwned<rustls::ClientConnection, crate::transport::Transport>>;

pub(crate) fn open(invite: &Invite) -> Result<Socket, RelayError> {
    let addr = invite.validate()?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(3))
        .map_err(|error| crate::transport::io_failure("Client TCP connect", &error))?;
    let mut stream = rustls::StreamOwned::new(
        tls::client(invite)?,
        crate::transport::Transport::handshake(tcp)?,
    );
    let deadline = Instant::now() + Duration::from_secs(4);
    while stream.conn.is_handshaking() {
        if Instant::now() > deadline {
            return Err(RelayError::Invalid("TLS handshake timed out"));
        }
        stream
            .conn
            .complete_io(&mut stream.sock)
            .map_err(|error| crate::transport::io_failure("Client TLS handshake", &error))?;
    }
    let (mut socket, _) = tungstenite::client::client_with_config(
        invite.endpoint.as_str(),
        stream,
        Some(wire::config()),
    )
    .map_err(|error| crate::transport::websocket_failure("Client WebSocket handshake", error))?;
    socket.get_mut().sock.polling()?;
    Ok(socket)
}

fn run(
    invite: Invite,
    profile: Profile,
    commands: Receiver<wire::Command>,
    events: &Sender<Event>,
    stop: &AtomicBool,
) -> Result<(), RelayError> {
    let mut socket = open(&invite)?;
    let hello = wire::Command::Hello {
        version: 1,
        room: invite.room,
        credential: invite.credential,
        profile,
    };
    write(&mut socket, &hello)?;
    let mut authenticated = false;
    let started = Instant::now();
    let mut seen = Instant::now();
    let mut pinged = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if (!authenticated && started.elapsed() > Duration::from_secs(5))
            || seen.elapsed() > Duration::from_secs(20)
        {
            return Err(RelayError::Invalid("Relay did not respond"));
        }
        if authenticated {
            for command in commands.try_iter().take(8) {
                write(&mut socket, &command)?;
            }
        }
        if pinged.elapsed() > Duration::from_secs(5) {
            socket
                .send(Message::Ping(Vec::new().into()))
                .map_err(ws_error)?;
            pinged = Instant::now();
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                seen = Instant::now();
                let event: Event = serde_json::from_str(&text)
                    .map_err(|_| RelayError::Invalid("Invalid relay response"))?;
                if matches!(event, Event::Connected { .. }) {
                    authenticated = true;
                }
                let terminal = matches!(event, Event::Disconnected | Event::Error { .. });
                // A stalled UI disconnects instead of silently dropping message
                // events and accidentally acknowledging undelivered content.
                events.try_send(event).map_err(channel_error)?;
                if terminal {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {
                seen = Instant::now();
            }
            Err(err) if wire::retryable(&err) => {}
            Err(err) => return Err(ws_error(err)),
        }
        socket.flush().map_err(ws_error)?;
    }
    let _ = socket.close(None);
    Ok(())
}

fn write(socket: &mut Socket, command: &wire::Command) -> Result<(), RelayError> {
    let text = serde_json::to_string(command)
        .map_err(|_| RelayError::Invalid("Could not encode relay command"))?;
    socket.send(Message::Text(text.into())).map_err(ws_error)
}

fn ws_error(error: tungstenite::Error) -> RelayError {
    RelayError::Transport(error.to_string())
}
