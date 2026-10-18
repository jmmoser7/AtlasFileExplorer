//! Local, single-connection Windows byte pipes. Call on worker threads only.
//!
//! The host supplies a PID from an independently trusted launch/attach action.
//! Same-user OS identity authenticates a transport peer, not its claimed pack or
//! its permission to edit a document. Core session policy owns those decisions.
//! Every retry needs a fresh address, session, and generation. Failed or timed
//! out framing closes the connection; partially consumed frames are never reused.

use atlas_connector::FrameError;
#[cfg(windows)]
use atlas_connector::{read_frame, read_raw_frame, write_frame, write_raw_frame};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

/// Cooperative cancellation shared with the worker executing an operation.
#[derive(Clone, Default, Debug)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Random rendezvous identity. It never accepts a UNC path or arbitrary pipe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipeAddress(String);

impl PipeAddress {
    /// Generate 256 bits using the operating system RNG; no predictable fallback.
    pub fn fresh() -> io::Result<Self> {
        #[cfg(windows)]
        {
            crate::pipe::fresh_token().map(Self)
        }
        #[cfg(not(windows))]
        {
            Err(unsupported())
        }
    }

    /// Reconstruct an address supplied by the trusted host's rendezvous channel.
    pub fn from_token(token: impl Into<String>) -> io::Result<Self> {
        let token = token.into();
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a 256-bit hex pipe token",
            ));
        }
        Ok(Self(token.to_ascii_lowercase()))
    }

    pub fn token(&self) -> &str {
        &self.0
    }
    pub fn pipe_name(&self) -> String {
        format!(r"\\.\pipe\slate-connector-{}", self.0)
    }
}

/// Verified OS properties, including creation time to distinguish PID reuse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity {
    pub process_id: u32,
    pub session_id: u32,
    pub user_sid: String,
    pub created_at_100ns: u64,
}

/// Single-use endpoint with a protected, explicit current-user-only DACL.
pub struct PipeListener {
    address: PipeAddress,
    #[cfg(windows)]
    inner: crate::pipe::Listener,
}

impl PipeListener {
    /// Bind before publishing the token. First-instance and one-instance guards
    /// fail closed if another server already owns this name. No process launches.
    pub fn bind(address: PipeAddress, expected_peer_pid: u32) -> io::Result<Self> {
        #[cfg(windows)]
        {
            let inner = crate::pipe::Listener::bind(&address, expected_peer_pid)?;
            Ok(Self { address, inner })
        }
        #[cfg(not(windows))]
        {
            let _ = (address, expected_peer_pid);
            Err(unsupported())
        }
    }

    pub fn address(&self) -> &PipeAddress {
        &self.address
    }

    /// Consume the endpoint even on failure; never reconnect an old identity.
    pub fn accept(self, timeout: Duration, cancel: &Cancellation) -> io::Result<PipeConnection> {
        #[cfg(windows)]
        {
            let inner = self.inner.accept(timeout, cancel)?;
            Ok(PipeConnection {
                address: self.address,
                inner,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (timeout, cancel);
            Err(unsupported())
        }
    }
}

pub struct PipeConnection {
    address: PipeAddress,
    #[cfg(windows)]
    inner: crate::pipe::Connection,
}

impl PipeConnection {
    /// Connect only to the trusted expected server PID, same OS user/session.
    /// Client security QoS prevents the server impersonating this process.
    pub fn connect(
        address: PipeAddress,
        expected_server_pid: u32,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> io::Result<Self> {
        #[cfg(windows)]
        {
            let inner =
                crate::pipe::Connection::connect(&address, expected_server_pid, timeout, cancel)?;
            Ok(Self { address, inner })
        }
        #[cfg(not(windows))]
        {
            let _ = (address, expected_server_pid, timeout, cancel);
            Err(unsupported())
        }
    }

    pub fn address(&self) -> &PipeAddress {
        &self.address
    }

    pub fn peer(&self) -> &PeerIdentity {
        #[cfg(windows)]
        {
            self.inner.peer()
        }
        #[cfg(not(windows))]
        {
            unreachable!("a Windows pipe connection cannot be created on this platform")
        }
    }

    /// One deadline covers the entire header + body, including partial writes.
    pub fn send<T: Serialize>(
        &mut self,
        value: &T,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<(), FrameError> {
        #[cfg(windows)]
        {
            let result = self
                .inner
                .stream(timeout, cancel)
                .map_err(FrameError::Io)
                .and_then(|mut stream| write_frame(&mut stream, value));
            if result.is_err() {
                self.close();
            }
            result
        }
        #[cfg(not(windows))]
        {
            let _ = (value, timeout, cancel);
            Err(FrameError::Io(unsupported()))
        }
    }

    /// Clean peer EOF yields None. Partial EOF, invalid JSON, oversize, timeout,
    /// and cancellation close this connection before returning the core error.
    pub fn recv<T: DeserializeOwned>(
        &mut self,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<Option<T>, FrameError> {
        #[cfg(windows)]
        {
            let result = self
                .inner
                .stream(timeout, cancel)
                .map_err(FrameError::Io)
                .and_then(|mut stream| read_frame(&mut stream));
            if !matches!(&result, Ok(Some(_))) {
                self.close();
            }
            result
        }
        #[cfg(not(windows))]
        {
            let _ = (timeout, cancel);
            Err(FrameError::Io(unsupported()))
        }
    }

    /// Artifact chunks only: use a separate authenticated pipe, never interleave
    /// binary chunks and control JSON. Each raw frame is bounded by the core's
    /// 4 MiB ceiling; transfer assembly, hashing and content policy live above IO.
    pub fn send_bytes(
        &mut self,
        bytes: &[u8],
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<(), FrameError> {
        #[cfg(windows)]
        {
            let result = self
                .inner
                .stream(timeout, cancel)
                .map_err(FrameError::Io)
                .and_then(|mut stream| write_raw_frame(&mut stream, bytes));
            if result.is_err() {
                self.close();
            }
            result
        }
        #[cfg(not(windows))]
        {
            let _ = (bytes, timeout, cancel);
            Err(FrameError::Io(unsupported()))
        }
    }

    /// The announced artifact remainder may impose a stricter bound than 4 MiB.
    /// Reject before allocating when the next chunk exceeds that bound.
    pub fn recv_bytes(
        &mut self,
        max_len: usize,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<Option<Vec<u8>>, FrameError> {
        #[cfg(windows)]
        {
            let result = self
                .inner
                .stream(timeout, cancel)
                .map_err(FrameError::Io)
                .and_then(|mut stream| read_raw_frame(&mut stream, max_len));
            if !matches!(&result, Ok(Some(_))) {
                self.close();
            }
            result
        }
        #[cfg(not(windows))]
        {
            let _ = (max_len, timeout, cancel);
            Err(FrameError::Io(unsupported()))
        }
    }

    pub fn is_open(&self) -> bool {
        #[cfg(windows)]
        {
            self.inner.is_open()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    /// Close without FlushFileBuffers: a peer that stopped reading cannot stall
    /// teardown. Already delivered bytes belong to the receiver's normal framing.
    pub fn close(&mut self) {
        #[cfg(windows)]
        {
            self.inner.close();
        }
    }
}

#[cfg(not(windows))]
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "application connector pipes require Windows",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rendezvous_rejects_paths_and_normalizes_hex() {
        for invalid in ["", r"\\server\pipe\remote", "..", &"g".repeat(64)] {
            assert!(PipeAddress::from_token(invalid).is_err());
        }
        let address = PipeAddress::from_token("AB".repeat(32)).unwrap();
        assert_eq!(address.token(), "ab".repeat(32));
        assert!(address
            .pipe_name()
            .starts_with(r"\\.\pipe\slate-connector-"));
    }
    #[test]
    fn cancellation_is_shared() {
        let cancel = Cancellation::default();
        let worker = cancel.clone();
        cancel.cancel();
        assert!(worker.is_cancelled());
    }
    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_fails_explicitly() {
        assert_eq!(
            PipeAddress::fresh().unwrap_err().kind(),
            io::ErrorKind::Unsupported
        );
        let address = PipeAddress::from_token("a".repeat(64)).unwrap();
        assert!(
            matches!(PipeListener::bind(address, 1), Err(e) if e.kind() == io::ErrorKind::Unsupported)
        );
    }
}
