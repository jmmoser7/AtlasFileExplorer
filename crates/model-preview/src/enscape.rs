use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Card copy for a confirmed Enscape standalone. The board shows this instead
/// of a viewport. Nothing in this crate launches the program.
pub const CARD: &str = "Enscape standalone. Double-click to open the walkthrough.";

/// Bytes read from the start of an `.exe` when looking for an Enscape package.
/// The standalone is a self-contained program, often hundreds of megabytes;
/// the marker lives in the launcher stub or the packed client name, not in
/// a scene we can draw.
pub const SNIFF_PREFIX: u64 = 16 * 1024 * 1024;
/// Extra bytes read from the end, for packages whose marker sits in a trailer.
pub const SNIFF_TAIL: u64 = 1024 * 1024;

/// ASCII markers observed in Enscape standalone packages. UTF-16LE copies of
/// the same strings are accepted too. Update these when a Chaos release moves
/// the marker, and add a byte-sample test — do not commit a real executable.
const MARKERS: &[&[u8]] = &[b"EnscapeStandalone", b"EnscapeClient.exe"];

pub fn sample_is_enscape(sample: &[u8]) -> bool {
    MARKERS.iter().any(|marker| contains(sample, marker))
        || MARKERS
            .iter()
            .any(|marker| contains(sample, &utf16_le_ascii(marker)))
}

/// Read at most [`SNIFF_PREFIX`] from the start and [`SNIFF_TAIL`] from the
/// end. The caller must already know the file is local: reading any byte of
/// a dehydrated cloud file downloads the whole file.
pub fn read_sample(path: &Path) -> std::io::Result<Vec<u8>> {
    read_sample_limited(path, SNIFF_PREFIX, SNIFF_TAIL)
}

pub fn read_sample_limited(path: &Path, prefix: u64, tail: u64) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let prefix_n = prefix.min(len) as usize;
    let mut buf = vec![0u8; prefix_n];
    file.read_exact(&mut buf)?;
    if tail > 0 && (prefix_n as u64) < len {
        let tail_n = tail.min(len - prefix_n as u64) as usize;
        file.seek(SeekFrom::End(-(tail_n as i64)))?;
        let start = buf.len();
        buf.resize(start + tail_n, 0);
        file.read_exact(&mut buf[start..])?;
    }
    Ok(buf)
}

fn utf16_le_ascii(ascii: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ascii.len() * 2);
    for byte in ascii {
        out.push(*byte);
        out.push(0);
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && hay.len() >= needle.len()
        && hay.windows(needle.len()).any(|w| w == needle)
}
