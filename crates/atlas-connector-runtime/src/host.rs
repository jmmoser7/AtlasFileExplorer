//! Worker-only host SDK. The host creates requests from its trusted action
//! dispatch; connector-provided JSON never establishes caller authority.
//!
//! Every dispatch is admitted by the pure Session owner. Its reply is validated
//! before local state advances. A broken exchange closes the channel and retains
//! an OutcomeUnknown receipt: reconnecting or replaying an edit is never implicit.

use crate::{
    deadline::Deadline,
    transport::{Cancellation, PeerIdentity, PipeAddress, PipeConnection, PipeListener},
};
use atlas_connector::{
    Begin, CallerAuthority, ErrorCode, FrameError, HostPolicy, Operation, ProtocolError, ReplyBody,
    ReplyEnvelope, RequestEnvelope, Session, SourceScope, WireFrame,
};
use std::{error::Error, fmt, io, time::Duration};

#[derive(Debug)]
pub enum HostErrorKind {
    Io(io::Error),
    Frame(FrameError),
    Protocol(ProtocolError),
    IdentityMismatch,
    UnexpectedFrame(&'static str),
    Closed,
}

/// Failures before admission have no receipt. Failures after dispatch include
/// the retained uncertain receipt, and session() exposes the same tombstone.
#[derive(Debug)]
pub struct HostError {
    kind: HostErrorKind,
    receipt: Option<ReplyEnvelope>,
}

impl HostError {
    pub fn kind(&self) -> &HostErrorKind {
        &self.kind
    }
    pub fn uncertain_receipt(&self) -> Option<&ReplyEnvelope> {
        self.receipt.as_ref()
    }

    fn before_dispatch(kind: HostErrorKind) -> Self {
        Self {
            kind,
            receipt: None,
        }
    }
}

impl fmt::Display for HostError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            HostErrorKind::Io(error) => write!(formatter, "connector host I/O: {error}"),
            HostErrorKind::Frame(error) => write!(formatter, "connector host framing: {error}"),
            HostErrorKind::Protocol(error) => write!(formatter, "connector host protocol: {error}"),
            HostErrorKind::IdentityMismatch => formatter
                .write_str("connector identity differs from explicitly selected pack/application"),
            HostErrorKind::UnexpectedFrame(expected) => {
                write!(formatter, "connector direction error: expected {expected}")
            }
            HostErrorKind::Closed => formatter.write_str("connector host channel is closed"),
        }?;
        if self.receipt.is_some() {
            formatter.write_str(
                "; outcome unknown, inspect the retained receipt before a fresh action",
            )?;
        }
        Ok(())
    }
}

impl Error for HostError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            HostErrorKind::Io(error) => Some(error),
            HostErrorKind::Frame(error) => Some(error),
            HostErrorKind::Protocol(error) => Some(error),
            _ => None,
        }
    }
}

/// One explicitly selected peer and negotiated session. This SDK never launches
/// a process, attaches a native window, reads a source, or retries a failed call.
pub struct HostChannel {
    connection: PipeConnection,
    session: Session,
}

impl HostChannel {
    /// PipeListener already pins the expected OS peer. Claims in Hello must also
    /// match the host's independently selected installation; neither claim grants
    /// launch consent or document mutation authority.
    #[allow(clippy::too_many_arguments)]
    pub fn accept(
        listener: PipeListener,
        expected_pack_id: &str,
        expected_application_id: &str,
        policy: &HostPolicy,
        session_id: String,
        generation: u64,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<Self, HostError> {
        let result = (|| -> Result<Self, HostErrorKind> {
            let budget = Deadline::new(timeout, cancel).map_err(HostErrorKind::Io)?;
            let mut connection = listener
                .accept(budget.remaining().map_err(HostErrorKind::Io)?, cancel)
                .map_err(HostErrorKind::Io)?;
            let hello = match connection
                .recv::<WireFrame>(budget.remaining().map_err(HostErrorKind::Io)?, cancel)
                .map_err(HostErrorKind::Frame)?
            {
                Some(WireFrame::Hello { hello }) => hello,
                Some(WireFrame::Error { error }) => return Err(HostErrorKind::Protocol(error)),
                None => return Err(HostErrorKind::Io(disconnected())),
                _ => return Err(HostErrorKind::UnexpectedFrame("connector Hello")),
            };
            if hello.identity.pack_id != expected_pack_id
                || hello.identity.application_id != expected_application_id
            {
                return Err(HostErrorKind::IdentityMismatch);
            }
            let (session, welcome) = Session::negotiate(policy, &hello, session_id, generation)
                .map_err(HostErrorKind::Protocol)?;
            connection
                .send(
                    &WireFrame::Welcome { welcome },
                    budget.remaining().map_err(HostErrorKind::Io)?,
                    cancel,
                )
                .map_err(HostErrorKind::Frame)?;
            Ok(Self {
                connection,
                session,
            })
        })();
        result.map_err(HostError::before_dispatch)
    }

    pub fn session(&self) -> &Session {
        &self.session
    }
    pub fn peer(&self) -> &PeerIdentity {
        self.connection.peer()
    }
    pub fn address(&self) -> &PipeAddress {
        self.connection.address()
    }

    /// Release only the transport; callers should request an explicit Preserve
    /// detach while the channel is healthy. No foreign application is terminated.
    pub fn close(&mut self) {
        self.connection.close();
    }

    /// `trusted` must come from the host's own human/agent action dispatch. The
    /// outbound wire caller is constructed here; there is no peer-supplied caller
    /// parameter. Identical cached requests replay locally without sending bytes.
    #[allow(clippy::too_many_arguments)]
    pub fn call(
        &mut self,
        id: &str,
        source: Option<SourceScope>,
        operation: Operation,
        trusted: CallerAuthority,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<ReplyEnvelope, HostError> {
        let budget = Deadline::new(timeout, cancel)
            .map_err(|error| HostError::before_dispatch(HostErrorKind::Io(error)))?;
        // A closed channel may still serve a terminal local receipt. It cannot
        // admit a new request or revive a session by itself.
        if !self.connection.is_open() && self.session.state(id).is_none() {
            return Err(HostError::before_dispatch(HostErrorKind::Closed));
        }
        let welcome = self.session.welcome();
        let request = RequestEnvelope {
            session_id: welcome.session_id.clone(),
            generation: welcome.generation,
            request_id: id.into(),
            caller: trusted.clone(),
            source,
            operation,
        };
        let admitted = self
            .session
            .begin(request, &trusted)
            .map_err(|error| HostError::before_dispatch(HostErrorKind::Protocol(error)))?;
        let request = match admitted {
            Begin::Replay(reply) => return Ok(reply),
            Begin::Dispatch(request) => request,
        };
        let exchange = (|| -> Result<ReplyEnvelope, HostErrorKind> {
            self.connection
                .send(
                    &WireFrame::Request { request },
                    budget.remaining().map_err(HostErrorKind::Io)?,
                    cancel,
                )
                .map_err(HostErrorKind::Frame)?;
            let reply = match self
                .connection
                .recv::<WireFrame>(budget.remaining().map_err(HostErrorKind::Io)?, cancel)
                .map_err(HostErrorKind::Frame)?
            {
                Some(WireFrame::Reply { reply }) => reply,
                Some(WireFrame::Error { error }) => return Err(HostErrorKind::Protocol(error)),
                None => return Err(HostErrorKind::Io(disconnected())),
                _ => return Err(HostErrorKind::UnexpectedFrame("connector Reply")),
            };
            if reply.request_id != id {
                return Err(HostErrorKind::Protocol(ProtocolError::new(
                    ErrorCode::InvalidResult,
                    "reply does not correlate with the dispatched request",
                )));
            }
            let accepted = self
                .session
                .accept_reply(reply)
                .map_err(HostErrorKind::Protocol)?;
            // A provider can itself report an uncertain native effect, and the
            // bounded core receipt cache can also substitute OutcomeUnknown.
            // Retain that receipt but require explicit reconnect/reconciliation
            // before admitting further work against a potentially stale source.
            if matches!(accepted.outcome, ReplyBody::OutcomeUnknown { .. }) {
                self.connection.close();
            }
            Ok(accepted)
        })();
        exchange.map_err(|kind| self.fail_dispatched(id, kind))
    }

    fn fail_dispatched(&mut self, id: &str, kind: HostErrorKind) -> HostError {
        self.connection.close();
        // Session is private and call is exclusive: this exact dispatch still
        // owns the reserved receipt slot, even after malformed provider output.
        let receipt = self
            .session
            .timeout(id)
            .expect("dispatched request owns a terminal receipt slot");
        HostError {
            kind,
            receipt: Some(receipt),
        }
    }
}

fn disconnected() -> io::Error {
    io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "connector disconnected before a complete reply",
    )
}
