//! Low-level 3dm chunk grammar: bounds-checked byte cursor and chunk headers.
//!
//! A .3dm archive is a flat byte stream of "chunks". Each chunk starts with a
//! 4-byte little-endian typecode followed by a length/value field. For archive
//! version >= 50 (Rhino 5 and newer, October 2009) the length field is 8 bytes
//! (`ON_BinaryArchive::SizeofChunkLength` in opennurbs_archive.cpp); this
//! crate only supports those archives, so the header is always 12 bytes.
//!
//! Two chunk shapes exist (opennurbs_3dm.h, "Typecode format"):
//!
//! * short chunks — the `TCODE_SHORT` bit (0x80000000) is set and the value
//!   field *is* the payload; no data block follows.
//! * big chunks — the value field is the byte length of the data block that
//!   follows. If the typecode has the `TCODE_CRC` bit (0x8000) set, the last
//!   4 bytes of that block are a CRC32 which is not part of the content
//!   (`ON_BinaryArchive::PushBigChunk` in opennurbs_archive.cpp). We skip
//!   CRCs rather than verifying them.
//!
//! Everything here is defensive: all reads are bounds-checked and lengths are
//! validated against the remaining input so malformed files cannot cause
//! panics or out-of-range slicing.

/// Bit flags and typecodes from opennurbs_3dm.h. Only the ones this crate
/// consumes are defined.
pub const TCODE_SHORT: u32 = 0x8000_0000;
pub const TCODE_CRC: u32 = 0x8000;

const TCODE_TABLE: u32 = 0x1000_0000;
const TCODE_TABLEREC: u32 = 0x2000_0000;
const TCODE_INTERFACE: u32 = 0x0200_0000;
const TCODE_OPENNURBS_OBJECT: u32 = 0x0002_0000;
const TCODE_USER: u32 = 0x4000_0000;

pub const TCODE_ENDOFFILE: u32 = 0x0000_7FFF;
pub const TCODE_OBJECT_TABLE: u32 = TCODE_TABLE | 0x0013;
pub const TCODE_ENDOFTABLE: u32 = 0xFFFF_FFFF;
pub const TCODE_OBJECT_RECORD: u32 = TCODE_TABLEREC | TCODE_CRC | 0x0070;
/// Short chunk whose value is the record's `ON::object_type` bit.
pub const TCODE_OBJECT_RECORD_TYPE: u32 = TCODE_INTERFACE | TCODE_SHORT | 0x0071;
pub const TCODE_OBJECT_RECORD_ATTRIBUTES: u32 = TCODE_INTERFACE | TCODE_CRC | 0x0072;
pub const TCODE_OBJECT_RECORD_END: u32 = TCODE_INTERFACE | TCODE_SHORT | 0x007F;
pub const TCODE_OPENNURBS_CLASS: u32 = TCODE_OPENNURBS_OBJECT | 0x7FFA;
pub const TCODE_OPENNURBS_CLASS_UUID: u32 = TCODE_OPENNURBS_OBJECT | TCODE_CRC | 0x7FFB;
pub const TCODE_OPENNURBS_CLASS_DATA: u32 = TCODE_OPENNURBS_OBJECT | TCODE_CRC | 0x7FFC;
pub const TCODE_OPENNURBS_CLASS_END: u32 = TCODE_OPENNURBS_OBJECT | TCODE_SHORT | 0x7FFF;
pub const TCODE_ANONYMOUS_CHUNK: u32 = TCODE_USER | TCODE_CRC;

/// A parsed chunk header plus its content slice (CRC already stripped).
pub struct Chunk<'a> {
    pub typecode: u32,
    /// Raw value field: payload for short chunks, content length for big ones.
    pub value: i64,
    /// Content bytes (big chunks only, empty for short chunks). Trailing
    /// CRC32 bytes are excluded when the typecode carries the CRC bit.
    pub content: &'a [u8],
}

impl Chunk<'_> {
    pub fn is_short(&self) -> bool {
        self.typecode & TCODE_SHORT != 0
    }
}

/// Bounds-checked forward-only reader over a byte slice.
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }

    pub fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    pub fn u32(&mut self) -> Option<u32> {
        let b = self.take(4)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }

    pub fn i64(&mut self) -> Option<i64> {
        let b = self.take(8)?;
        Some(i64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// One-byte chunk version as written by `Write3dmChunkVersion`
    /// (opennurbs_archive.cpp): high nibble = major, low nibble = minor.
    pub fn chunk_version(&mut self) -> Option<(u8, u8)> {
        let v = self.u8()?;
        Some((v >> 4, v & 0x0F))
    }

    /// Read the next chunk header + content. Big-chunk content is validated
    /// to fit in the remaining input. `None` means truncated/garbage data.
    pub fn chunk(&mut self) -> Option<Chunk<'a>> {
        let typecode = self.u32()?;
        let value = self.i64()?;
        if typecode & TCODE_SHORT != 0 {
            return Some(Chunk {
                typecode,
                value,
                content: &[],
            });
        }
        // Big chunk: negative lengths are invalid in v50+ archives.
        if value < 0 {
            return None;
        }
        let len = usize::try_from(value).ok()?;
        let body = self.take(len)?;
        // The CRC (when present) trails the content inside the counted body.
        let content = if typecode & TCODE_CRC != 0 {
            body.get(..len.checked_sub(4)?)?
        } else {
            body
        };
        Some(Chunk {
            typecode,
            value,
            content,
        })
    }

    /// Read the next chunk and require an exact typecode.
    pub fn expect_chunk(&mut self, typecode: u32) -> Option<Chunk<'a>> {
        let c = self.chunk()?;
        (c.typecode == typecode).then_some(c)
    }

    /// Skip a chunk expected to be a `TCODE_ANONYMOUS_CHUNK` wrapper (the
    /// framing openNURBS uses for nested sub-structures we don't decode).
    pub fn skip_anonymous_chunk(&mut self) -> Option<()> {
        self.expect_chunk(TCODE_ANONYMOUS_CHUNK).map(|_| ())
    }
}

/// UUIDs are serialized field-wise little-endian: u32, u16, u16, then 8 raw
/// bytes (`ON_BinaryArchive::ReadUuid`, opennurbs_archive.cpp). We keep them
/// as the canonical lowercase hyphenated string for comparisons.
pub fn read_uuid(cur: &mut Cursor<'_>) -> Option<String> {
    let b = cur.take(16)?;
    let d1 = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let d2 = u16::from_le_bytes([b[4], b[5]]);
    let d3 = u16::from_le_bytes([b[6], b[7]]);
    Some(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        d1, d2, d3, b[8], b[9], b[10], b[11], b[12], b[13], b[14], b[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn big_chunk(typecode: u32, content: &[u8], crc: bool) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&typecode.to_le_bytes());
        let len = content.len() as i64 + if crc { 4 } else { 0 };
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(content);
        if crc {
            out.extend_from_slice(&[0u8; 4]);
        }
        out
    }

    #[test]
    fn short_chunk_carries_value_in_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&TCODE_OBJECT_RECORD_TYPE.to_le_bytes());
        bytes.extend_from_slice(&0x20i64.to_le_bytes());
        let mut cur = Cursor::new(&bytes);
        let c = cur.chunk().expect("chunk");
        assert!(c.is_short());
        assert_eq!(c.value, 0x20);
        assert!(c.content.is_empty());
        assert!(cur.is_empty());
    }

    #[test]
    fn big_chunk_with_crc_strips_trailing_bytes() {
        let bytes = big_chunk(TCODE_ANONYMOUS_CHUNK, b"payload", true);
        let mut cur = Cursor::new(&bytes);
        let c = cur.chunk().expect("chunk");
        assert_eq!(c.content, b"payload");
        assert!(cur.is_empty());
    }

    #[test]
    fn truncated_big_chunk_is_rejected() {
        let mut bytes = big_chunk(TCODE_ANONYMOUS_CHUNK, b"payload", false);
        bytes.truncate(bytes.len() - 3);
        let mut cur = Cursor::new(&bytes);
        assert!(cur.chunk().is_none());
    }

    #[test]
    fn negative_length_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&TCODE_ANONYMOUS_CHUNK.to_le_bytes());
        bytes.extend_from_slice(&(-5i64).to_le_bytes());
        let mut cur = Cursor::new(&bytes);
        assert!(cur.chunk().is_none());
    }

    #[test]
    fn crc_chunk_shorter_than_crc_is_rejected() {
        // Claims CRC framing but the body cannot even hold the 4 CRC bytes.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&TCODE_ANONYMOUS_CHUNK.to_le_bytes());
        bytes.extend_from_slice(&2i64.to_le_bytes());
        bytes.extend_from_slice(&[0u8; 2]);
        let mut cur = Cursor::new(&bytes);
        assert!(cur.chunk().is_none());
    }

    #[test]
    fn uuid_round_trip_formatting() {
        // ON_Mesh class id 4ED7D4E4-E947-11d3-BFE5-0010830122F0 serialized
        // field-wise little-endian.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x4ED7_D4E4u32.to_le_bytes());
        bytes.extend_from_slice(&0xE947u16.to_le_bytes());
        bytes.extend_from_slice(&0x11D3u16.to_le_bytes());
        bytes.extend_from_slice(&[0xBF, 0xE5, 0x00, 0x10, 0x83, 0x01, 0x22, 0xF0]);
        let mut cur = Cursor::new(&bytes);
        assert_eq!(
            read_uuid(&mut cur).as_deref(),
            Some("4ed7d4e4-e947-11d3-bfe5-0010830122f0")
        );
    }
}
