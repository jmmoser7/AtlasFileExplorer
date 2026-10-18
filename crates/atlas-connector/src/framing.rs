use std::fmt;
use std::io::{self, Read, Write};

use serde::{de::DeserializeOwned, Serialize};

pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub enum FrameError {
    Io(io::Error),
    Json(serde_json::Error),
    InvalidLength(u32),
    TooLarge { length: usize, limit: usize },
    Truncated { expected: usize, received: usize },
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "frame I/O: {e}"),
            Self::Json(e) => write!(f, "invalid frame JSON: {e}"),
            Self::InvalidLength(n) => write!(f, "invalid frame length: {n}"),
            Self::TooLarge { length, limit } => write!(f, "frame length {length} exceeds {limit}"),
            Self::Truncated { expected, received } => write!(
                f,
                "truncated frame: expected {expected}, received {received}"
            ),
        }
    }
}

impl std::error::Error for FrameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

/// Returns `None` only for clean EOF before the first header byte. Readers must
/// enforce their own deadline and disconnect after any error; partial frames
/// cannot safely be resumed by starting a new `read_frame` call.
pub fn read_frame<T: DeserializeOwned>(reader: &mut impl Read) -> Result<Option<T>, FrameError> {
    let Some(payload) = read_raw_frame(reader, MAX_FRAME_BYTES)? else {
        return Ok(None);
    };
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(FrameError::Json)
}

/// Binary transfer chunks use the same prefix and global 4MiB frame limit.
/// `max_len` may reduce the bound; it cannot enlarge it.
pub fn read_raw_frame(
    reader: &mut impl Read,
    max_len: usize,
) -> Result<Option<Vec<u8>>, FrameError> {
    let mut header = [0; 4];
    let count = read_part(reader, &mut header)?;
    if count == 0 {
        return Ok(None);
    }
    if count != header.len() {
        return Err(FrameError::Truncated {
            expected: 4,
            received: count,
        });
    }
    let raw = u32::from_le_bytes(header);
    let length = raw as usize;
    if length == 0 {
        return Err(FrameError::InvalidLength(raw));
    }
    let limit = max_len.min(MAX_FRAME_BYTES);
    if length > limit {
        return Err(FrameError::TooLarge { length, limit });
    }
    let mut payload = vec![0; length];
    let count = read_part(reader, &mut payload)?;
    if count != length {
        return Err(FrameError::Truncated {
            expected: length,
            received: count,
        });
    }
    Ok(Some(payload))
}

fn read_part(reader: &mut impl Read, bytes: &mut [u8]) -> Result<usize, FrameError> {
    let mut received = 0;
    while received < bytes.len() {
        match reader.read(&mut bytes[received..]) {
            Ok(0) => break,
            Ok(n) => received += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(FrameError::Io(e)),
        }
    }
    Ok(received)
}

/// Serializes through a bounded writer so an oversized value cannot first
/// allocate an unbounded JSON buffer. A failed write makes the stream unusable.
pub fn write_frame<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), FrameError> {
    let mut payload = BoundedBuffer(Vec::new());
    if let Err(error) = serde_json::to_writer(&mut payload, value) {
        if payload.0.len() == MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge {
                length: MAX_FRAME_BYTES + 1,
                limit: MAX_FRAME_BYTES,
            });
        }
        return Err(FrameError::Json(error));
    }
    write_raw_frame(writer, &payload.0)
}

pub fn write_raw_frame(writer: &mut impl Write, bytes: &[u8]) -> Result<(), FrameError> {
    if bytes.is_empty() {
        return Err(FrameError::InvalidLength(0));
    }
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge {
            length: bytes.len(),
            limit: MAX_FRAME_BYTES,
        });
    }
    writer
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(FrameError::Io)?;
    writer.write_all(bytes).map_err(FrameError::Io)?;
    writer.flush().map_err(FrameError::Io)
}

struct BoundedBuffer(Vec<u8>);
impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = MAX_FRAME_BYTES - self.0.len();
        if bytes.len() > remaining {
            self.0.extend_from_slice(&bytes[..remaining]);
            return Err(io::Error::other("frame size limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
