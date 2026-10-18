//! Bounded ComfyUI WebSocket event transport, including fragmented messages.
use super::encode_component;
use std::{
    io::{Read, Write},
    time::Duration,
};

const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct Message {
    opcode: Option<u8>,
    bytes: Vec<u8>,
}

impl Message {
    fn push(
        &mut self,
        opcode: u8,
        fin: bool,
        payload: Vec<u8>,
        limit: usize,
    ) -> std::io::Result<Option<(u8, Vec<u8>)>> {
        let invalid_sequence = match opcode {
            0x0 => self.opcode.is_none(),
            0x1 | 0x2 => self.opcode.is_some(),
            _ => true,
        };
        if invalid_sequence
            || self
                .bytes
                .len()
                .checked_add(payload.len())
                .is_none_or(|length| length > limit)
        {
            *self = Self::default();
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid or oversized event message",
            ));
        }
        if opcode != 0 {
            if fin {
                return Ok(Some((opcode, payload)));
            }
            self.opcode = Some(opcode);
            self.bytes = payload;
            return Ok(None);
        }
        if self.bytes.try_reserve_exact(payload.len()).is_err() {
            *self = Self::default();
            return Err(std::io::Error::other("event buffer allocation failed"));
        }
        self.bytes.extend(payload);
        if fin {
            return Ok(Some((
                self.opcode.take().expect("message opcode"),
                std::mem::take(&mut self.bytes),
            )));
        }
        Ok(None)
    }
}

/// One WebSocket frame: opcode, FIN, and unmasked payload.
pub(super) fn read_frame<R: Read>(r: &mut R) -> std::io::Result<(u8, bool, Vec<u8>)> {
    let mut head = [0u8; 2];
    r.read_exact(&mut head)?;
    let (fin, opcode) = (head[0] & 0x80 != 0, head[0] & 0x0f);
    let mut len = u64::from(head[1] & 0x7f);
    if len == 126 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b)?;
        len = u64::from(u16::from_be_bytes(b));
    } else if len == 127 {
        let mut b = [0u8; 8];
        r.read_exact(&mut b)?;
        len = u64::from_be_bytes(b);
    }
    if len > MAX_MESSAGE_BYTES as u64 {
        return Err(std::io::Error::other("oversized frame"));
    }
    let mask = if head[1] & 0x80 != 0 {
        let mut m = [0u8; 4];
        r.read_exact(&mut m)?;
        Some(m)
    } else {
        None
    };
    let mut payload = vec![0u8; len as usize];
    r.read_exact(&mut payload)?;
    if let Some(m) = mask {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= m[i % 4];
        }
    }
    Ok((opcode, fin, payload))
}

/// The encoded image in a ComfyUI binary preview message, if it is one.
pub(super) fn preview_image(payload: &[u8]) -> Option<&[u8]> {
    const PREVIEW_IMAGE: u32 = 1;
    let event = u32::from_be_bytes(payload.get(..4)?.try_into().ok()?);
    (event == PREVIEW_IMAGE && payload.len() > 8).then(|| &payload[8..])
}

/// ComfyUI's event socket for the client that queued a prompt: progress and
/// preview frames. Loopback and server-to-client only, so a small reader
/// replaces a WebSocket dependency.
pub(super) struct Events {
    stream: std::net::TcpStream,
    reader: std::io::BufReader<std::net::TcpStream>,
    partial: Message,
}

impl Events {
    pub(super) fn connect(client_id: &str) -> Option<Self> {
        use std::io::BufRead;
        let addr = "127.0.0.1:8188".parse().ok()?;
        let mut stream =
            std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
        stream.set_nodelay(true).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
        write!(
            stream,
            "GET /ws?clientId={} HTTP/1.1\r\nHost: 127.0.0.1:8188\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
            encode_component(client_id)
        )
        .ok()?;
        let mut reader = std::io::BufReader::new(stream.try_clone().ok()?);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if !line.contains(" 101") {
            return None;
        }
        loop {
            line.clear();
            if reader.read_line(&mut line).ok()? == 0 || line == "\r\n" {
                break;
            }
        }
        Some(Self {
            stream,
            reader,
            partial: Message::default(),
        })
    }

    /// The next whole message, or `None` once `wait` passes with nothing to read.
    pub(super) fn next(&mut self, wait: Duration) -> std::io::Result<Option<(u8, Vec<u8>)>> {
        use std::io::BufRead;
        self.stream
            .set_read_timeout(Some(wait.max(Duration::from_millis(1))))?;
        match self.reader.fill_buf() {
            Ok([]) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(e),
        }
        // A frame has begun; finish it without the poll timeout.
        self.stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        loop {
            let (opcode, fin, payload) = read_frame(&mut self.reader)?;
            match opcode {
                0x8 => return Err(std::io::ErrorKind::ConnectionAborted.into()),
                0x9 => {
                    // Client frames are masked; a zero key leaves the payload as is.
                    let mut pong = vec![0x8A, 0x80 | payload.len().min(125) as u8, 0, 0, 0, 0];
                    pong.extend(payload.iter().take(125));
                    self.stream.write_all(&pong)?;
                }
                0xA => {}
                0x0..=0x2 => {
                    if let Some(message) =
                        self.partial.push(opcode, fin, payload, MAX_MESSAGE_BYTES)?
                    {
                        return Ok(Some(message));
                    }
                }
                _ => return Err(std::io::Error::other("unsupported event opcode")),
            }
            if !fin {
                continue;
            }
            return Ok(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_messages_and_first_fragments_move_the_existing_buffer() {
        let mut message = Message::default();
        let payload = b"image".to_vec();
        let pointer = payload.as_ptr();
        let (_, complete) = message.push(2, true, payload, 8).unwrap().unwrap();
        assert_eq!(complete.as_ptr(), pointer);
        let payload = b"first".to_vec();
        let pointer = payload.as_ptr();
        assert!(message.push(2, false, payload, 8).unwrap().is_none());
        assert_eq!(message.bytes.as_ptr(), pointer);
    }

    #[test]
    fn fragments_share_one_aggregate_limit_and_failure_releases_storage() {
        let mut message = Message::default();
        assert!(message.push(2, false, vec![1; 5], 8).unwrap().is_none());
        assert!(message.push(0, true, vec![2; 4], 8).is_err());
        assert!(message.bytes.is_empty());
        assert_eq!(message.bytes.capacity(), 0);
        assert!(message.opcode.is_none());
        assert_eq!(
            message.push(1, true, b"fresh".to_vec(), 8).unwrap(),
            Some((1, b"fresh".to_vec()))
        );
    }

    #[test]
    fn an_exact_limit_message_preserves_its_initial_opcode() {
        let mut message = Message::default();
        assert!(message.push(2, false, vec![1; 4], 8).unwrap().is_none());
        assert_eq!(
            message.push(0, true, vec![2; 4], 8).unwrap(),
            Some((2, vec![1, 1, 1, 1, 2, 2, 2, 2]))
        );
        assert!(message.opcode.is_none());
        assert!(message.bytes.is_empty());
    }

    #[test]
    fn continuations_and_overlapping_messages_refuse_invalid_sequences() {
        let mut message = Message::default();
        assert!(message.push(0, true, vec![1], 8).is_err());
        message.push(1, false, vec![1], 8).unwrap();
        assert!(message.push(2, true, vec![2], 8).is_err());
        assert!(message.opcode.is_none());
        assert!(message.bytes.is_empty());
    }
}
