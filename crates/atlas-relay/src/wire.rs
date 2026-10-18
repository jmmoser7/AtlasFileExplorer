use crate::{Profile, MAX_FRAME_BYTES};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Command {
    Hello { version: u8, room: String, credential: String, profile: Profile },
    Send { recipient: String, message_id: String, text: String },
    Ack { message_id: String },
    UpdateProfile { profile: Profile },
    Disconnect,
}

pub(crate) fn config() -> tungstenite::protocol::WebSocketConfig {
    tungstenite::protocol::WebSocketConfig::default()
        .read_buffer_size(16 * 1024)
        .write_buffer_size(16 * 1024)
        .max_write_buffer_size(MAX_FRAME_BYTES * 2)
        .max_message_size(Some(MAX_FRAME_BYTES))
        .max_frame_size(Some(MAX_FRAME_BYTES))
}

pub(crate) fn retryable(err: &tungstenite::Error) -> bool {
    matches!(err, tungstenite::Error::Io(e)
        if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut))
}

/// A connection gets a small burst, then 20 commands/sec. Applied before JSON
/// dispatch, including invalid requests and acknowledgments.
pub(crate) struct Rate {
    period: Instant,
    count: u32,
}

impl Rate {
    pub fn new() -> Self { Self { period: Instant::now(), count: 0 } }
    pub fn allow(&mut self) -> bool {
        if self.period.elapsed() >= Duration::from_secs(1) {
            self.period = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        self.count <= 20
    }
}
