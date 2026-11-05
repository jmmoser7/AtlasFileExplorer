use super::{Job, VideoEvent, VideoFrame, PLAY_EDGE, STRIP_EDGE, STRIP_FRAMES};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"AVD1";
pub(super) const MAX_PACKET: usize = 4 * 1024 * 1024;
const MAX_PATH: usize = 64 * 1024;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid video worker packet")
}

fn allocate(len: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| io::Error::other("video allocation failed"))?;
    bytes.resize(len, 0);
    Ok(bytes)
}

fn record(out: &mut impl Write, header: &[u8], pixels: &[u8]) -> io::Result<()> {
    let size = header
        .len()
        .checked_add(pixels.len())
        .filter(|v| *v <= MAX_PACKET)
        .ok_or_else(invalid)?;
    out.write_all(MAGIC)?;
    out.write_all(&(size as u32).to_le_bytes())?;
    out.write_all(header)?;
    out.write_all(pixels)?;
    out.flush()
}

fn read_record(input: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut magic = [0; 4];
    input.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(invalid());
    }
    let mut size = [0; 4];
    input.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    if size == 0 || size > MAX_PACKET {
        return Err(invalid());
    }
    let mut bytes = allocate(size)?;
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

struct Fields<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Fields<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self.offset.checked_add(n).ok_or_else(invalid)?;
        let value = self.bytes.get(self.offset..end).ok_or_else(invalid)?;
        self.offset = end;
        Ok(value)
    }
    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| invalid())?,
        ))
    }
    fn float(&mut self) -> io::Result<f32> {
        let value = f32::from_bits(self.u32()?);
        if !value.is_finite() || value < 0.0 {
            return Err(invalid());
        }
        Ok(value)
    }
}

pub(super) fn write_job(out: &mut impl Write, job: &Job) -> io::Result<()> {
    let (id, token, path, start, end, from, looped, kind) = match job {
        Job::Strip {
            id,
            token,
            path,
            start,
            end,
        } => (*id, *token, path, *start, *end, 0.0, false, 0u8),
        Job::Play {
            id,
            token,
            path,
            start,
            end,
            from,
            looped,
        } => (*id, *token, path, *start, *end, *from, *looped, 1u8),
    };
    let path = path_bytes(path)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(40 + path.len())
        .map_err(|_| io::Error::other("video allocation failed"))?;
    bytes.push(kind);
    bytes.extend_from_slice(&id.to_le_bytes());
    bytes.extend_from_slice(&token.to_le_bytes());
    bytes.extend_from_slice(&start.to_bits().to_le_bytes());
    bytes.push(u8::from(end.is_some()));
    bytes.extend_from_slice(&end.unwrap_or(0.0).to_bits().to_le_bytes());
    bytes.extend_from_slice(&from.to_bits().to_le_bytes());
    bytes.push(u8::from(looped));
    bytes.extend_from_slice(&(path.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&path);
    record(out, &bytes, &[])
}

pub(super) fn read_job(input: &mut impl Read) -> io::Result<Job> {
    let bytes = read_record(input)?;
    let mut fields = Fields {
        bytes: &bytes,
        offset: 0,
    };
    let kind = fields.byte()?;
    let (id, token) = (fields.u64()?, fields.u64()?);
    // Request times are normalized by the production playback-window owner.
    let start = f32::from_bits(fields.u32()?);
    let has_end = fields.byte()?;
    let end = f32::from_bits(fields.u32()?);
    let end = match has_end {
        0 => None,
        1 => Some(end),
        _ => return Err(invalid()),
    };
    let from = f32::from_bits(fields.u32()?);
    let looped = match fields.byte()? {
        0 => false,
        1 => true,
        _ => return Err(invalid()),
    };
    let len = fields.u32()? as usize;
    if len == 0 || len > MAX_PATH {
        return Err(invalid());
    }
    let path = decode_path(fields.take(len)?)?;
    if fields.offset != bytes.len() {
        return Err(invalid());
    }
    match kind {
        0 => Ok(Job::Strip {
            id,
            token,
            path,
            start,
            end,
        }),
        1 => Ok(Job::Play {
            id,
            token,
            path,
            start,
            end,
            from,
            looped,
        }),
        _ => Err(invalid()),
    }
}

pub(super) fn write_event(out: &mut impl Write, event: &VideoEvent) -> io::Result<()> {
    let (id, token) = event.key();
    let mut header = Vec::with_capacity(40);
    let (kind, time, index, frame) = match event {
        VideoEvent::Probed { duration, .. } => (2u8, Some(*duration), None, None),
        VideoEvent::Strip {
            time, index, frame, ..
        } => (3, Some(*time), Some(*index), Some(frame)),
        VideoEvent::StripDone { .. } => (4, None, None, None),
        VideoEvent::Play { time, frame, .. } => (5, Some(*time), None, Some(frame)),
        VideoEvent::Ended { .. } => (6, None, None, None),
        VideoEvent::Failed { .. } => (7, None, None, None),
    };
    header.push(kind);
    header.extend_from_slice(&id.to_le_bytes());
    header.extend_from_slice(&token.to_le_bytes());
    if let Some(time) = time {
        if !time.is_finite() || time < 0.0 {
            return Err(invalid());
        }
        header.extend_from_slice(&time.to_bits().to_le_bytes());
    }
    if let Some(index) = index {
        header.extend_from_slice(&index.to_le_bytes());
    }
    if let Some(frame) = frame {
        validate_frame(
            frame.w,
            frame.h,
            frame.rgba.len(),
            if kind == 3 { STRIP_EDGE } else { PLAY_EDGE },
        )?;
        header.extend_from_slice(&frame.w.to_le_bytes());
        header.extend_from_slice(&frame.h.to_le_bytes());
        record(out, &header, &frame.rgba)
    } else {
        record(out, &header, &[])
    }
}

pub(super) fn read_event(input: &mut impl Read) -> io::Result<VideoEvent> {
    let mut bytes = read_record(input)?;
    let mut fields = Fields {
        bytes: &bytes,
        offset: 0,
    };
    let kind = fields.byte()?;
    let (id, token) = (fields.u64()?, fields.u64()?);
    let event = match kind {
        2 => VideoEvent::Probed {
            id,
            token,
            duration: fields.float()?,
        },
        4 => VideoEvent::StripDone { id, token },
        6 => VideoEvent::Ended { id, token },
        7 => VideoEvent::Failed { id, token },
        3 | 5 => {
            let time = fields.float()?;
            let index = if kind == 3 { fields.u16()? } else { 0 };
            if index as usize >= STRIP_FRAMES {
                return Err(invalid());
            }
            let (w, h) = (fields.u32()?, fields.u32()?);
            let offset = fields.offset;
            validate_frame(
                w,
                h,
                bytes.len() - offset,
                if kind == 3 { STRIP_EDGE } else { PLAY_EDGE },
            )?;
            bytes.drain(..offset);
            let frame = VideoFrame { w, h, rgba: bytes };
            return Ok(if kind == 3 {
                VideoEvent::Strip {
                    id,
                    token,
                    index,
                    time,
                    frame,
                }
            } else {
                VideoEvent::Play {
                    id,
                    token,
                    time,
                    frame,
                }
            });
        }
        _ => return Err(invalid()),
    };
    if fields.offset != bytes.len() {
        return Err(invalid());
    }
    Ok(event)
}

fn validate_frame(w: u32, h: u32, len: usize, edge: u32) -> io::Result<()> {
    if w == 0
        || h == 0
        || w > edge
        || h > edge
        || (w as usize)
            .checked_mul(h as usize)
            .and_then(|v| v.checked_mul(4))
            != Some(len)
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(windows)]
fn path_bytes(path: &Path) -> io::Result<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    let mut bytes = Vec::new();
    bytes
        .try_reserve(MAX_PATH)
        .map_err(|_| io::Error::other("video allocation failed"))?;
    for unit in path.as_os_str().encode_wide() {
        if bytes.len() + 2 > MAX_PATH || unit == 0 {
            return Err(invalid());
        }
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    if bytes.is_empty() {
        return Err(invalid());
    }
    Ok(bytes)
}
#[cfg(windows)]
fn decode_path(bytes: &[u8]) -> io::Result<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid());
    }
    let mut units = Vec::new();
    units
        .try_reserve_exact(bytes.len() / 2)
        .map_err(|_| io::Error::other("video allocation failed"))?;
    for pair in bytes.chunks_exact(2) {
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            return Err(invalid());
        }
        units.push(unit);
    }
    Ok(std::ffi::OsString::from_wide(&units).into())
}
#[cfg(not(windows))]
fn path_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let bytes = path.to_str().ok_or_else(invalid)?.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_PATH || bytes.contains(&0) {
        return Err(invalid());
    }
    Ok(bytes.to_vec())
}
#[cfg(not(windows))]
fn decode_path(bytes: &[u8]) -> io::Result<PathBuf> {
    Ok(std::str::from_utf8(bytes).map_err(|_| invalid())?.into())
}

#[cfg(test)]
mod tests;
