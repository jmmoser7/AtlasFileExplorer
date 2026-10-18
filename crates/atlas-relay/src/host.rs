use crate::{room, tls, wire, Event, Invite, RelayError, MAX_PEERS, QUEUE};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket};

/// One ephemeral encrypted participant room. Keys and room credentials remain
/// only in memory. Default callers should bind `127.0.0.1:0`; remote access is
/// an explicit advertised-address choice, never an automatic firewall change.
pub struct Host {
    invite: Invite,
    stopped: Arc<AtomicBool>,
    errors: Receiver<String>,
}

impl Host {
    pub fn start(bind: SocketAddr) -> Result<Self, RelayError> {
        let listener = TcpListener::bind(bind)?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let (invite, config) = tls::server(addr)?;
        let stopped = Arc::new(AtomicBool::new(false));
        let (input, receiver) = bounded(QUEUE);
        let (errors, failures) = bounded(16);
        let stop = stopped.clone();
        std::thread::Builder::new()
            .name("atlas-relay-room".into())
            .spawn(move || room::run(receiver, stop))?;
        let stop = stopped.clone();
        let auth = invite.clone();
        let launched = std::thread::Builder::new()
            .name("atlas-relay-host".into())
            .spawn(move || {
                listen(listener, config, auth, input, errors, stop);
            });
        if let Err(error) = launched {
            stopped.store(true, Ordering::Release);
            return Err(error.into());
        }
        Ok(Self {
            invite,
            stopped,
            errors: failures,
        })
    }

    pub fn invite(&self) -> Invite {
        self.invite.clone()
    }
    /// At most16 sanitized handshake failures; no room or message contents.
    pub fn errors(&self) -> &Receiver<String> {
        &self.errors
    }
    pub fn address(&self) -> SocketAddr {
        self.invite
            .endpoint
            .trim_start_matches("wss://")
            .trim_end_matches("/participants")
            .parse()
            .expect("host address was constructed from SocketAddr")
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}

fn listen(
    listener: TcpListener,
    config: Arc<rustls::ServerConfig>,
    invite: Invite,
    input: Sender<room::Input>,
    errors: Sender<String>,
    stop: Arc<AtomicBool>,
) {
    let active = Arc::new(AtomicUsize::new(0));
    let mut next = 0_u64;
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((tcp, _)) => {
                if active.fetch_add(1, Ordering::AcqRel) >= MAX_PEERS {
                    active.fetch_sub(1, Ordering::AcqRel);
                    continue;
                }
                next += 1;
                let connection = next;
                let config = config.clone();
                let invite = invite.clone();
                let input = input.clone();
                let errors = errors.clone();
                let stop = stop.clone();
                let peers_active = active.clone();
                let launched = std::thread::Builder::new()
                    .name("atlas-relay-peer".into())
                    .spawn(move || {
                        let _ = peer(tcp, config, &invite, &input, &errors, &stop, connection);
                        // Leave is essential even under backpressure. Bounded wait
                        // keeps a dying socket from pinning live membership.
                        let _ = input.send_timeout(
                            room::Input::Leave { connection },
                            Duration::from_millis(400),
                        );
                        peers_active.fetch_sub(1, Ordering::AcqRel);
                    });
                if launched.is_err() {
                    active.fetch_sub(1, Ordering::AcqRel);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(_) => break,
        }
    }
    stop.store(true, Ordering::Release);
}

type Socket = WebSocket<rustls::StreamOwned<rustls::ServerConnection, crate::transport::Transport>>;

struct LiveConnection(Arc<AtomicBool>);
impl Drop for LiveConnection {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn open(tcp: TcpStream, config: Arc<rustls::ServerConfig>) -> Result<Socket, RelayError> {
    let connection = rustls::ServerConnection::new(config)
        .map_err(|_| RelayError::Invalid("TLS initialization failed"))?;
    let mut stream =
        rustls::StreamOwned::new(connection, crate::transport::Transport::handshake(tcp)?);
    let deadline = Instant::now() + Duration::from_secs(4);
    while stream.conn.is_handshaking() {
        if Instant::now() > deadline {
            return Err(RelayError::Invalid("TLS handshake timed out"));
        }
        stream
            .conn
            .complete_io(&mut stream.sock)
            .map_err(|error| crate::transport::io_failure("Server TLS handshake", &error))?;
    }
    tungstenite::accept_with_config(stream, Some(wire::config()))
        .map_err(|error| crate::transport::websocket_failure("Server WebSocket handshake", error))
}

fn peer(
    tcp: TcpStream,
    config: Arc<rustls::ServerConfig>,
    invite: &Invite,
    input: &Sender<room::Input>,
    errors: &Sender<String>,
    stop: &AtomicBool,
    connection: u64,
) -> Result<(), RelayError> {
    let mut socket = open(tcp, config).map_err(|error| {
        let mut summary = error.to_string();
        summary.truncate(256);
        let _ = errors.try_send(summary);
        error
    })?;
    let hello = socket.read().map_err(ws_error)?;
    let authorized = match hello {
        Message::Text(text) => match serde_json::from_str::<wire::Command>(&text) {
            Ok(wire::Command::Hello {
                version: 1,
                room,
                credential,
                profile,
            }) if room == invite.room
                && credential == invite.credential
                && profile.validate().is_ok() =>
            {
                Some(profile)
            }
            _ => None,
        },
        _ => None,
    };
    let Some(profile) = authorized else {
        write(
            &mut socket,
            &Event::Error {
                message: "Room authentication failed".into(),
            },
        )?;
        let _ = socket.close(None);
        return Ok(());
    };
    let (outgoing, events) = bounded(QUEUE);
    let live = Arc::new(AtomicBool::new(true));
    let _liveness = LiveConnection(live.clone());
    input
        .try_send(room::Input::Join {
            connection,
            profile,
            outgoing,
            live,
        })
        .map_err(crate::channel_error)?;
    socket.get_mut().sock.polling()?;
    exchange(&mut socket, events, input, stop, connection)
}

fn exchange(
    socket: &mut Socket,
    events: Receiver<Event>,
    input: &Sender<room::Input>,
    stop: &AtomicBool,
    connection: u64,
) -> Result<(), RelayError> {
    let mut rate = wire::Rate::new();
    let mut seen = Instant::now();
    let mut pinged = Instant::now();
    while !stop.load(Ordering::Acquire) && seen.elapsed() < Duration::from_secs(20) {
        for _ in 0..8 {
            match events.try_recv() {
                Ok(event) => {
                    let terminal = matches!(event, Event::Disconnected | Event::Error { .. });
                    write(socket, &event)?;
                    if terminal {
                        let _ = socket.close(None);
                        return Ok(());
                    }
                }
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => return Ok(()),
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
                if !rate.allow() {
                    return Err(RelayError::Invalid("Participant rate limit exceeded"));
                }
                let command = serde_json::from_str(&text)
                    .map_err(|_| RelayError::Invalid("Invalid participant command"))?;
                input
                    .try_send(room::Input::Data {
                        connection,
                        command,
                    })
                    .map_err(crate::channel_error)?;
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

fn write(socket: &mut Socket, event: &Event) -> Result<(), RelayError> {
    let text = serde_json::to_string(event)
        .map_err(|_| RelayError::Invalid("Could not encode relay event"))?;
    socket.send(Message::Text(text.into())).map_err(ws_error)
}

fn ws_error(error: tungstenite::Error) -> RelayError {
    RelayError::Transport(error.to_string())
}
