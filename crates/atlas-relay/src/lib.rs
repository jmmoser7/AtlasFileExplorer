//! The bounded participant-messaging slice of the self-hosted relay.
//!
//! There is no board model or agent authority here. Live membership expires
//! when a socket closes. The app owns persistent identity and conversation
//! history, and must persist an incoming message **before** calling `ack`.
//! `Accepted` means queued for a connected recipient; only `Ack` means that
//! recipient explicitly acknowledged persistence. Nothing sends automatically.
mod client;
mod host;
mod room;
#[cfg(test)]
mod security_tests;
mod tls;
mod transport;
mod wire;

pub use client::Client;
pub use host::Host;
use serde::{Deserialize, Serialize};
pub use tls::Invite;

pub const MAX_TEXT_BYTES: usize = 32 * 1024;
/// Tiny display portrait, encoded by the app. The relay never decodes images.
pub const MAX_AVATAR_BYTES: usize = 2048;
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_PEERS: usize = 20;
pub(crate) const QUEUE: usize = 128;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Untrusted encoded image bytes. Decode on a worker with dimension and
    /// allocation limits; the transport checks only the byte-length bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Vec<u8>>,
}

impl Profile {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            avatar: None,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), RelayError> {
        if !valid_id(&self.id) || self.name.trim().is_empty() || self.name.len() > 160 {
            return Err(RelayError::Invalid("Invalid participant profile"));
        }
        if self
            .avatar
            .as_ref()
            .is_some_and(|bytes| bytes.is_empty() || bytes.len() > MAX_AVATAR_BYTES)
        {
            return Err(RelayError::Invalid(
                "Participant portrait must contain 1–2048 bytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Connected {
        peers: Vec<Profile>,
    },
    Peers {
        peers: Vec<Profile>,
    },
    Profile {
        profile: Profile,
    },
    Incoming {
        from: Profile,
        message_id: String,
        text: String,
    },
    Accepted {
        message_id: String,
    },
    Ack {
        message_id: String,
        recipient: String,
    },
    Rejected {
        message_id: String,
        reason: String,
    },
    Disconnected,
    Error {
        message: String,
    },
}

#[derive(Debug)]
pub enum RelayError {
    Invalid(&'static str),
    Transport(String),
    Backpressure,
    Disconnected,
}

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) => f.write_str(s),
            Self::Transport(s) => write!(f, "Relay transport: {s}"),
            Self::Backpressure => f.write_str("Relay is busy; message was not queued"),
            Self::Disconnected => f.write_str("Relay is disconnected"),
        }
    }
}

impl std::error::Error for RelayError {}

impl From<std::io::Error> for RelayError {
    fn from(value: std::io::Error) -> Self {
        Self::Transport(value.to_string())
    }
}

pub(crate) fn valid_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b':'))
}

pub(crate) fn validate_message(recipient: &str, id: &str, text: &str) -> Result<(), RelayError> {
    if !valid_id(recipient) || !valid_id(id) {
        return Err(RelayError::Invalid(
            "Invalid message or recipient identifier",
        ));
    }
    if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
        return Err(RelayError::Invalid("Message must contain 1–32768 bytes"));
    }
    Ok(())
}

pub(crate) fn channel_error<T>(err: crossbeam_channel::TrySendError<T>) -> RelayError {
    match err {
        crossbeam_channel::TrySendError::Full(_) => RelayError::Backpressure,
        crossbeam_channel::TrySendError::Disconnected(_) => RelayError::Disconnected,
    }
}
