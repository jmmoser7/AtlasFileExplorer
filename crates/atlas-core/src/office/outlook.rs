//! Outlook `.msg` excerpts. The file is a Compound File Binary package; this
//! reads the header properties and a plain-text body, falling back to HTML
//! or compressed RTF. Attachment bytes are never read — only their names.

use std::io::Read;
use std::path::Path;

const PT_STRING8: u16 = 0x001E;
const PT_UNICODE: u16 = 0x001F;
const PT_SYSTIME: u16 = 0x0040;
const PT_BINARY: u16 = 0x0102;

const PR_SUBJECT: u16 = 0x0037;
const PR_CLIENT_SUBMIT_TIME: u16 = 0x0039;
const PR_SENDER_NAME: u16 = 0x0C1A;
const PR_SENDER_EMAIL: u16 = 0x0C1F;
const PR_DISPLAY_CC: u16 = 0x0E03;
const PR_DISPLAY_TO: u16 = 0x0E04;
const PR_MESSAGE_DELIVERY_TIME: u16 = 0x0E06;
const PR_BODY: u16 = 0x1000;
const PR_HTML: u16 = 0x1013;
const PR_RTF_COMPRESSED: u16 = 0x1009;
const PR_DISPLAY_NAME: u16 = 0x3001;
const PR_ATTACH_FILENAME: u16 = 0x3704;
const PR_ATTACH_LONG_FILENAME: u16 = 0x3707;

const READ_CAP: usize = 512 * 1024;

/// Header block, then body, for one local `.msg` file.
/// `None` when the file is cloud-only, not a message, or has nothing to show.
pub fn message_excerpt(path: &Path) -> Option<String> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let mut comp = cfb::open(path).ok()?;
    let entries = streams(&comp);
    let from = person(
        &prop_string(&mut comp, &entries, PR_SENDER_NAME),
        &prop_string(&mut comp, &entries, PR_SENDER_EMAIL),
    );
    let to = prop_string(&mut comp, &entries, PR_DISPLAY_TO);
    let cc = prop_string(&mut comp, &entries, PR_DISPLAY_CC);
    let subject = prop_string(&mut comp, &entries, PR_SUBJECT);
    let date = prop_time(&mut comp, &entries, PR_CLIENT_SUBMIT_TIME)
        .or_else(|| prop_time(&mut comp, &entries, PR_MESSAGE_DELIVERY_TIME));
    let body = prop_string(&mut comp, &entries, PR_BODY)
        .or_else(|| {
            prop_bytes(&mut comp, &entries, PR_HTML, PT_BINARY).and_then(|bytes| {
                let text = html_to_text(&decode_maybe_utf16(&bytes));
                nonempty(text)
            })
        })
        .or_else(|| {
            prop_bytes(&mut comp, &entries, PR_RTF_COMPRESSED, PT_BINARY)
                .and_then(|bytes| decompress_rtf(&bytes))
                .and_then(|rtf| super::text::rtf_text(&rtf))
        });
    let attachments = attachment_names(&mut comp, &entries);
    let mut lines = Vec::new();
    push_field(&mut lines, "From", from);
    push_field(&mut lines, "To", to);
    push_field(&mut lines, "Cc", cc);
    push_field(&mut lines, "Subject", subject);
    push_field(&mut lines, "Date", date);
    let mut text = lines.join("\n");
    if let Some(body) = body {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(body.trim());
    }
    if !attachments.is_empty() {
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str("Attachments: ");
        text.push_str(&attachments.join(", "));
    }
    nonempty(text)
}

#[derive(Clone)]
struct Stream {
    path: String,
    len: u64,
}

fn streams(comp: &cfb::CompoundFile<std::fs::File>) -> Vec<Stream> {
    comp.walk()
        .filter(|entry| entry.is_stream())
        .map(|entry| Stream {
            path: entry.path().to_string_lossy().replace('\\', "/"),
            len: entry.len(),
        })
        .collect()
}

fn prop_string(
    comp: &mut cfb::CompoundFile<std::fs::File>,
    entries: &[Stream],
    id: u16,
) -> Option<String> {
    if let Some(text) = prop_bytes(comp, entries, id, PT_UNICODE).and_then(|b| decode_utf16(&b)) {
        return Some(text);
    }
    prop_bytes(comp, entries, id, PT_STRING8).and_then(|b| {
        let text = String::from_utf8_lossy(&b)
            .trim_end_matches('\0')
            .trim()
            .to_string();
        nonempty(text)
    })
}

fn prop_time(
    comp: &mut cfb::CompoundFile<std::fs::File>,
    entries: &[Stream],
    id: u16,
) -> Option<String> {
    let bytes = prop_bytes(comp, entries, id, PT_SYSTIME)?;
    if bytes.len() < 8 {
        return None;
    }
    let ticks = u64::from_le_bytes(bytes[..8].try_into().ok()?);
    format_filetime(ticks)
}

fn prop_bytes(
    comp: &mut cfb::CompoundFile<std::fs::File>,
    entries: &[Stream],
    id: u16,
    ty: u16,
) -> Option<Vec<u8>> {
    let suffix = format!("__substg1.0_{id:04X}{ty:04X}").to_ascii_lowercase();
    let entry = entries
        .iter()
        .find(|entry| entry.path.to_ascii_lowercase().ends_with(&suffix))?;
    if entry.len > READ_CAP as u64 {
        return None;
    }
    let stream = comp.open_stream(&entry.path).ok()?;
    let mut buf = Vec::new();
    stream.take(READ_CAP as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn attachment_names(
    comp: &mut cfb::CompoundFile<std::fs::File>,
    entries: &[Stream],
) -> Vec<String> {
    let mut storages: Vec<String> = entries
        .iter()
        .filter_map(|entry| {
            let path = entry.path.to_ascii_lowercase();
            let marker = path.find("__attach_version1.0_")?;
            let rest = &entry.path[marker..];
            let storage = rest.split('/').next()?.to_string();
            Some(storage)
        })
        .collect();
    storages.sort();
    storages.dedup();
    let mut names = Vec::new();
    for storage in storages {
        let name = prop_in_storage(comp, entries, &storage, PR_ATTACH_LONG_FILENAME)
            .or_else(|| prop_in_storage(comp, entries, &storage, PR_ATTACH_FILENAME))
            .or_else(|| prop_in_storage(comp, entries, &storage, PR_DISPLAY_NAME));
        if let Some(name) = name {
            names.push(name);
        }
    }
    names
}

fn prop_in_storage(
    comp: &mut cfb::CompoundFile<std::fs::File>,
    entries: &[Stream],
    storage: &str,
    id: u16,
) -> Option<String> {
    let storage = storage.to_ascii_lowercase();
    let narrow: Vec<Stream> = entries
        .iter()
        .filter(|entry| entry.path.to_ascii_lowercase().contains(&storage))
        .cloned()
        .collect();
    prop_string(comp, &narrow, id)
}

fn person(name: &Option<String>, email: &Option<String>) -> Option<String> {
    match (name, email) {
        (Some(name), Some(email)) if !email.is_empty() && name != email => {
            Some(format!("{name} <{email}>"))
        }
        (Some(name), _) => Some(name.clone()),
        (None, Some(email)) => Some(email.clone()),
        _ => None,
    }
}

fn push_field(lines: &mut Vec<String>, label: &str, value: Option<String>) {
    if let Some(value) = value {
        if !value.is_empty() {
            lines.push(format!("{label}: {value}"));
        }
    }
}

fn nonempty(text: String) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn decode_utf16(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let (chunks, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = chunks.iter().map(|c| u16::from_le_bytes(*c)).collect();
    nonempty(String::from_utf16_lossy(&units).replace('\0', ""))
}

fn decode_maybe_utf16(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes.len().is_multiple_of(2) {
        let nuls = bytes.iter().step_by(2).skip(1).filter(|b| **b == 0).count();
        if nuls * 2 > bytes.len() / 2 {
            if let Some(text) = decode_utf16(bytes) {
                return text;
            }
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn html_to_text(html: &str) -> String {
    let restored = html
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ");
    html_to_text_plain(&restored)
}

/// Outlook HTML bodies carry large `<style>` blocks in `<head>`; their text is
/// not the message.
fn html_to_text_plain(html: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    let mut hidden: Option<String> = None;
    for c in html.chars() {
        if in_tag {
            if c == '>' {
                in_tag = false;
                let closing = tag.trim_start().starts_with('/');
                let name = tag
                    .trim()
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                tag.clear();
                if let Some(open) = &hidden {
                    if closing && *open == name {
                        hidden = None;
                    }
                    continue;
                }
                if !closing && matches!(name.as_str(), "style" | "script" | "head" | "title") {
                    hidden = Some(name);
                    continue;
                }
                if matches!(
                    name.as_str(),
                    "br" | "p" | "div" | "tr" | "li" | "h1" | "h2" | "h3"
                ) && !out.ends_with('\n')
                {
                    out.push('\n');
                }
            } else {
                tag.push(c);
            }
            continue;
        }
        if c == '<' {
            in_tag = true;
            continue;
        }
        if hidden.is_none() {
            out.push(c);
        }
    }
    collapse(&out)
}

fn collapse(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_filetime(ticks: u64) -> Option<String> {
    const TICKS_PER_SEC: u64 = 10_000_000;
    const UNIX_EPOCH: u64 = 11_644_473_600;
    let secs = ticks / TICKS_PER_SEC;
    if secs < UNIX_EPOCH {
        return None;
    }
    let unix = secs - UNIX_EPOCH;
    let days = (unix / 86_400) as i64;
    let tod = unix % 86_400;
    let (y, m, d) = civil_from_days(days)?;
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    Some(format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} UTC"))
}

fn civil_from_days(days: i64) -> Option<(i32, u32, u32)> {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((y as i32, m as u32, d as u32))
}

/// MS-OXRTFCP. Uncompressed payloads pass through; compressed ones expand
/// against the standard 4 KiB dictionary. CRC is not checked.
fn decompress_rtf(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 16 {
        return None;
    }
    let comp_size = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    let raw_size = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    let magic = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let data_len = comp_size.checked_sub(12)?;
    let data = bytes.get(16..16 + data_len).unwrap_or(&bytes[16..]);
    const UNCOMPRESSED: u32 = 0x414C_454D;
    const COMPRESSED: u32 = 0x7546_5A4C;
    let raw = match magic {
        UNCOMPRESSED => data.to_vec(),
        COMPRESSED => expand_lzfu(data)?,
        _ => return None,
    };
    if raw_size > 0 && raw.len() > raw_size.saturating_mul(4).max(raw_size + 64) {
        return None;
    }
    Some(raw)
}

fn expand_lzfu(data: &[u8]) -> Option<Vec<u8>> {
    const INIT: &[u8] = b"{\\rtf1\\ansi\\mac\\deff0\\deftab720{\\fonttbl;}{\\f0\\fnil \\froman \\fswiss \\fmodern \\fscript \\fdecor MS Sans SerifSymbolArialTimes New RomanCourier{\\colortbl\\red0\\green0\\blue0\r\n\\par \\pard\\plain\\f0\\fs20\\b\\i\\u\\tab\\tx";
    let mut dict = [0u8; 4096];
    let n = INIT.len().min(dict.len());
    dict[..n].copy_from_slice(&INIT[..n]);
    let mut write = n;
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let flags = data[i];
        i += 1;
        for bit in 0..8 {
            if i >= data.len() {
                break;
            }
            if flags & (1 << bit) == 0 {
                let byte = data[i];
                i += 1;
                out.push(byte);
                dict[write] = byte;
                write = (write + 1) % 4096;
            } else {
                if i + 1 >= data.len() {
                    return None;
                }
                let b1 = data[i] as usize;
                let b2 = data[i + 1] as usize;
                i += 2;
                let offset = (b1 << 4) | (b2 >> 4);
                let len = (b2 & 0x0F) + 2;
                if offset >= 4096 {
                    return None;
                }
                let mut read = offset;
                for _ in 0..len {
                    let byte = dict[read];
                    out.push(byte);
                    dict[write] = byte;
                    write = (write + 1) % 4096;
                    read = (read + 1) % 4096;
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_msg(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas-msg-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    }

    fn put(comp: &mut cfb::CompoundFile<std::fs::File>, name: &str, bytes: &[u8]) {
        let mut stream = comp.create_stream(name).unwrap();
        stream.write_all(bytes).unwrap();
    }

    #[test]
    fn a_synthesized_message_renders_headers_body_and_attachment_names() {
        let path = temp_msg("note.msg");
        let mut comp = cfb::create(&path).unwrap();
        put(&mut comp, "/__substg1.0_0C1A001F", &utf16("Ada Lovelace"));
        put(
            &mut comp,
            "/__substg1.0_0C1F001F",
            &utf16("ada@example.com"),
        );
        put(
            &mut comp,
            "/__substg1.0_0E04001F",
            &utf16("Charles Babbage"),
        );
        put(&mut comp, "/__substg1.0_0E03001F", &utf16("Grace Hopper"));
        put(
            &mut comp,
            "/__substg1.0_0037001F",
            &utf16("Analytical engine"),
        );
        let ticks: u64 = 134359344000000000;
        put(&mut comp, "/__substg1.0_00390040", &ticks.to_le_bytes());
        put(
            &mut comp,
            "/__substg1.0_1000001F",
            &utf16("The difference engine is ready."),
        );
        comp.create_storage("/__attach_version1.0_#00000000")
            .unwrap();
        put(
            &mut comp,
            "/__attach_version1.0_#00000000/__substg1.0_3707001F",
            &utf16("notes.pdf"),
        );
        drop(comp);

        let text = message_excerpt(&path).unwrap();
        assert!(
            text.contains("From: Ada Lovelace <ada@example.com>"),
            "{text}"
        );
        assert!(text.contains("To: Charles Babbage"), "{text}");
        assert!(text.contains("Cc: Grace Hopper"), "{text}");
        assert!(text.contains("Subject: Analytical engine"), "{text}");
        assert!(text.contains("Date: 2026-10-08 12:00 UTC"), "{text}");
        assert!(text.contains("The difference engine is ready."), "{text}");
        assert!(text.contains("Attachments: notes.pdf"), "{text}");
        let body_at = text.find("The difference engine").unwrap();
        let subject_at = text.find("Subject:").unwrap();
        assert!(subject_at < body_at);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn html_body_is_used_when_plain_text_is_missing() {
        let path = temp_msg("html.msg");
        let mut comp = cfb::create(&path).unwrap();
        put(&mut comp, "/__substg1.0_0037001F", &utf16("Hello"));
        let html = b"<html><head><style>p.MsoNormal { margin: 0 }</style></head>\
<body><p>Hello <b>there</b> &amp; welcome</p></body></html>";
        put(&mut comp, "/__substg1.0_10130102", html);
        drop(comp);
        let text = message_excerpt(&path).unwrap();
        assert!(text.contains("Subject: Hello"), "{text}");
        assert!(text.contains("Hello there & welcome"), "{text}");
        assert!(!text.contains("<b>"), "{text}");
        assert!(
            !text.contains("MsoNormal"),
            "style text is not the body: {text}"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn uncompressed_rtf_is_the_last_body_fallback() {
        let path = temp_msg("rtf.msg");
        let mut comp = cfb::create(&path).unwrap();
        put(&mut comp, "/__substg1.0_0037001F", &utf16("RTF"));
        let rtf = br"{\rtf1\ansi Hello from RTF\par }";
        let mut payload = Vec::new();
        let comp_size = (12 + rtf.len()) as u32;
        payload.extend_from_slice(&comp_size.to_le_bytes());
        payload.extend_from_slice(&(rtf.len() as u32).to_le_bytes());
        payload.extend_from_slice(&0x414C454Du32.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(rtf);
        put(&mut comp, "/__substg1.0_10090102", &payload);
        drop(comp);
        let text = message_excerpt(&path).unwrap();
        assert!(text.contains("Hello from RTF"), "{text}");
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn literal_lzfu_round_trips() {
        let flags = 0x00u8;
        let literals = b"Hello!!!";
        let mut data = vec![flags];
        data.extend_from_slice(literals);
        let raw = expand_lzfu(&data).unwrap();
        assert_eq!(&raw[..], literals);
    }

    #[cfg(windows)]
    #[test]
    fn a_cloud_placeholder_message_is_not_opened() {
        let path = temp_msg("cloud.msg");
        std::fs::write(&path, b"not really a message").unwrap();
        assert!(crate::cloud::mark_offline(&path));
        assert!(message_excerpt(&path).is_none());
        assert_eq!(std::fs::read(&path).unwrap(), b"not really a message");
        let _ = crate::cloud::clear_offline(&path);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }
}
