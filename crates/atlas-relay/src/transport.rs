//! An absolute handshake deadline, including fragmented TLS/HTTP input. A
//! per-read timeout alone lets a slow sender keep a handshake alive forever.
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

/// Diagnostics contain categories, never transport payloads or HTTP headers.
pub(crate) fn io_failure(phase: &'static str, error: &io::Error) -> crate::RelayError {
    let tls = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<rustls::Error>());
    let category = match tls {
        Some(rustls::Error::InvalidCertificate(_)) => "invalid_certificate",
        Some(rustls::Error::InvalidMessage(_)) => "invalid_tls_message",
        Some(rustls::Error::PeerIncompatible(_)) => "tls_peer_incompatible",
        Some(rustls::Error::PeerMisbehaved(_)) => "tls_peer_misbehaved",
        Some(rustls::Error::AlertReceived(_)) => "tls_alert",
        Some(rustls::Error::NoCertificatesPresented) => "missing_certificate",
        Some(_) => "tls_other",
        None => "io",
    };
    crate::RelayError::Transport(format!(
        "{phase}: {category}, kind={:?}, os={:?}",
        error.kind(),
        error.raw_os_error()
    ))
}

pub(crate) fn websocket_failure<Role: tungstenite::handshake::HandshakeRole>(
    phase: &'static str,
    error: tungstenite::HandshakeError<Role>,
) -> crate::RelayError {
    use tungstenite::{Error, HandshakeError};
    let category = match error {
        HandshakeError::Interrupted(_) => "interrupted",
        HandshakeError::Failure(Error::Io(error)) => return io_failure(phase, &error),
        HandshakeError::Failure(Error::Http(response)) => {
            return crate::RelayError::Transport(format!(
                "{phase}: HTTP status={}",
                response.status().as_u16()
            ));
        }
        HandshakeError::Failure(Error::Protocol(
            tungstenite::error::ProtocolError::HandshakeIncomplete,
        )) => "incomplete",
        HandshakeError::Failure(Error::Protocol(_)) => "protocol",
        HandshakeError::Failure(Error::Tls(_)) => "tls",
        HandshakeError::Failure(Error::Url(_)) => "url",
        HandshakeError::Failure(Error::HttpFormat(_)) => "http_format",
        HandshakeError::Failure(Error::Capacity(_)) => "capacity",
        HandshakeError::Failure(Error::AttackAttempt) => "attack_limit",
        HandshakeError::Failure(Error::Utf8(_)) => "utf8",
        HandshakeError::Failure(Error::ConnectionClosed | Error::AlreadyClosed) => "closed",
        HandshakeError::Failure(Error::WriteBufferFull(_)) => "write_buffer_full",
    };
    crate::RelayError::Transport(format!("{phase}: {category}"))
}

pub(crate) struct Transport {
    tcp: TcpStream,
    deadline: Option<Instant>,
}

impl Transport {
    pub fn handshake(tcp: TcpStream) -> io::Result<Self> {
        // Winsock accept inherits the nonblocking listener's socket mode.
        // This worker uses blocking I/O bounded by the absolute deadline.
        tcp.set_nonblocking(false)?;
        tcp.set_nodelay(true)?;
        Ok(Self {
            tcp,
            deadline: Some(Instant::now() + Duration::from_secs(4)),
        })
    }

    pub fn polling(&mut self) -> io::Result<()> {
        self.deadline = None;
        self.tcp.set_read_timeout(Some(Duration::from_millis(40)))?;
        self.tcp.set_write_timeout(Some(Duration::from_millis(400)))
    }

    fn remaining(&self) -> io::Result<Option<Duration>> {
        self.deadline
            .map(|d| {
                d.checked_duration_since(Instant::now())
                    .filter(|v| !v.is_zero())
                    .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "Handshake timed out"))
            })
            .transpose()
    }
}

impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(timeout) = self.remaining()? {
            self.tcp.set_read_timeout(Some(timeout))?;
        }
        self.tcp.read(buf).map_err(|error| {
            // Rustls/Tungstenite define WouldBlock as resumable. A socket
            // polling timeout means the same thing after the handshake.
            if self.deadline.is_none() && error.kind() == io::ErrorKind::TimedOut {
                io::Error::new(io::ErrorKind::WouldBlock, "No relay data available yet")
            } else {
                error
            }
        })
    }
}

impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(timeout) = self.remaining()? {
            self.tcp.set_write_timeout(Some(timeout))?;
        }
        self.tcp.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.tcp.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn handshake_normalizes_an_accepted_nonblocking_socket_and_keeps_its_deadline() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let _peer =
            TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(1))
                .unwrap();
        let (accepted, _) = listener.accept().unwrap();
        accepted.set_nonblocking(true).unwrap();
        let mut transport = Transport::handshake(accepted).unwrap();
        let deadline = transport.deadline.unwrap();
        transport
            .tcp
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        // A silent blocking socket times out with WSAETIMEDOUT. Without mode
        // normalization this returns WSAEWOULDBLOCK regardless of scheduling.
        let error = transport.tcp.read(&mut [0]).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(10060));
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert_eq!(transport.deadline, Some(deadline));
    }

    #[test]
    fn expired_handshake_deadline_refuses_read_and_write_even_with_queued_data() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer =
            TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(1))
                .unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let (accepted, _) = listener.accept().unwrap();
        let mut transport = Transport::handshake(accepted).unwrap();
        peer.write_all(b"r").unwrap();
        let expired = Instant::now() - Duration::from_secs(1);
        transport.deadline = Some(expired);
        assert_eq!(
            transport.read(&mut [0]).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(
            transport.write(b"s").unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(transport.deadline, Some(expired));
    }

    #[test]
    fn handshake_io_diagnostic_excludes_error_payloads() {
        let error = io::Error::new(
            io::ErrorKind::InvalidData,
            rustls::Error::General("PRIVATE_TLS_DETAILS".into()),
        );
        let summary = io_failure("Client TLS handshake", &error).to_string();
        assert!(summary.contains("tls_other, kind=InvalidData"));
        assert!(!summary.contains("PRIVATE_TLS_DETAILS"));
        assert!(summary.len() < 256);
    }

    #[test]
    fn websocket_http_diagnostic_excludes_headers_and_body() {
        type Role = tungstenite::handshake::client::ClientHandshake<io::Cursor<Vec<u8>>>;
        let response = tungstenite::http::Response::builder()
            .status(403)
            .header("authorization", "PRIVATE_HEADER")
            .body(Some(b"PRIVATE_BODY".to_vec()))
            .unwrap();
        let summary = websocket_failure::<Role>(
            "Client WebSocket handshake",
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(Box::new(response))),
        )
        .to_string();
        assert!(summary.contains("HTTP status=403"));
        assert!(!summary.contains("PRIVATE_HEADER"));
        assert!(!summary.contains("PRIVATE_BODY"));
        assert!(summary.len() < 256);
    }
}
