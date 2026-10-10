//! Page size from a PDF's page boxes, without loading pdfium.
//!
//! A dropped page is sized from these points before pdfium has rendered
//! anything. Rendering still goes through pdfium; this only answers width
//! and height, including a `/Rotate` of 90 or 270, a box inherited from a
//! parent `/Pages` node, and page objects packed into a compressed object
//! stream (PDF 1.5+).

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// Largest file [`file_page_sizes`] opens. It runs when a page is placed,
/// so a bigger document keeps the default card instead of stalling the drop.
pub const MAX_SIZE_READ_BYTES: u64 = 32 * 1024 * 1024;

/// Largest object stream inflated while looking for page dictionaries.
const MAX_INFLATED_BYTES: u64 = 16 * 1024 * 1024;

/// Width and height in PDF points for every page, in page order. `None`
/// for a page whose box could not be read.
pub fn page_sizes(bytes: &[u8]) -> Vec<Option<(f32, f32)>> {
    let objs = objects(bytes);
    page_ids(&objs)
        .into_iter()
        .map(|id| {
            let (w, h) = inherited(id, &objs, &|b| page_box(b, "/CropBox"))
                .or_else(|| inherited(id, &objs, &|b| page_box(b, "/MediaBox")))?;
            let turn = inherited(id, &objs, &|b| number_after(b, "/Rotate")).unwrap_or(0.0);
            Some(if (turn as i32).rem_euclid(180) == 90 {
                (h, w)
            } else {
                (w, h)
            })
        })
        .collect()
}

/// Width and height in PDF points for 0-based `page`.
pub fn page_points(bytes: &[u8], page: u16) -> Option<(f32, f32)> {
    page_sizes(bytes).get(page as usize).copied().flatten()
}

/// Every page's size for the file at `path`, from one read. A cloud
/// placeholder is never opened (one byte hydrates the whole file), and
/// neither is a file over [`MAX_SIZE_READ_BYTES`].
pub fn file_page_sizes(path: &Path) -> Vec<Option<(f32, f32)>> {
    if crate::cloud::is_dehydrated(path) {
        return Vec::new();
    }
    let fits = std::fs::metadata(path).is_ok_and(|m| m.len() > 0 && m.len() <= MAX_SIZE_READ_BYTES);
    if !fits {
        return Vec::new();
    }
    std::fs::read(path)
        .map(|bytes| page_sizes(&bytes))
        .unwrap_or_default()
}

/// Object number → dictionary text. A later plain object replaces an
/// earlier one (incremental update); packed objects fill the gaps.
fn objects(bytes: &[u8]) -> HashMap<u32, String> {
    let mut out = HashMap::new();
    let mut packed = Vec::new();
    let mut at = 0;
    while let Some(rel) = find(&bytes[at..], b" obj") {
        let start = at + rel;
        at = start + 4;
        let Some(id) = object_number(&bytes[start.saturating_sub(24)..start]) else {
            continue;
        };
        let end = find(&bytes[at..], b"endobj").map_or(bytes.len(), |e| at + e);
        let body = &bytes[at..end];
        let dict_end = find(body, b"stream").unwrap_or(body.len());
        let dict = String::from_utf8_lossy(&body[..dict_end]).into_owned();
        if type_names(&dict).any(|t| t == "ObjStm") {
            packed.push((dict.clone(), &body[dict_end..]));
        }
        out.insert(id, dict);
        at = end;
    }
    for (dict, stream) in packed {
        unpack(&dict, stream, &mut out);
    }
    out
}

/// `N G` immediately before ` obj`.
fn object_number(head: &[u8]) -> Option<u32> {
    let head = String::from_utf8_lossy(head);
    let mut words = head.split_whitespace().rev();
    words.next()?.parse::<u32>().ok()?;
    words.next()?.parse().ok()
}

fn unpack(dict: &str, raw: &[u8], out: &mut HashMap<u32, String>) {
    let (Some(n), Some(first)) = (number_after(dict, "/N"), number_after(dict, "/First")) else {
        return;
    };
    let (n, first) = (n as usize, first as usize);
    let mut data = raw.get(6..).unwrap_or_default();
    if let Some(rest) = data
        .strip_prefix(b"\r\n")
        .or_else(|| data.strip_prefix(b"\n"))
    {
        data = rest;
    }
    if let Some(end) = find(data, b"endstream") {
        data = &data[..end];
    }
    let data = if dict.contains("/FlateDecode") {
        let mut inflated = Vec::new();
        let _ = flate2::read::ZlibDecoder::new(data)
            .take(MAX_INFLATED_BYTES)
            .read_to_end(&mut inflated);
        inflated
    } else {
        data.to_vec()
    };
    let Some(header) = data.get(..first) else {
        return;
    };
    let nums: Vec<usize> = String::from_utf8_lossy(header)
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let pairs: Vec<(u32, usize)> = nums
        .chunks_exact(2)
        .take(n)
        .map(|pair| (pair[0] as u32, first + pair[1]))
        .collect();
    for (index, &(id, start)) in pairs.iter().enumerate() {
        let end = pairs.get(index + 1).map_or(data.len(), |next| next.1);
        if let Some(obj) = data.get(start..end.max(start)) {
            out.entry(id)
                .or_insert_with(|| String::from_utf8_lossy(obj).into_owned());
        }
    }
}

/// Pages in document order: the catalog's tree, else the parentless
/// `/Pages` node, else every page object by number.
fn page_ids(objs: &HashMap<u32, String>) -> Vec<u32> {
    let root = objs
        .values()
        .find(|b| type_names(b).any(|t| t == "Catalog"))
        .and_then(|b| reference_after(b, "/Pages"))
        .or_else(|| {
            objs.iter()
                .filter(|(_, b)| type_names(b).any(|t| t == "Pages") && !b.contains("/Parent"))
                .map(|(id, _)| *id)
                .min()
        });
    let mut ids = Vec::new();
    if let Some(root) = root {
        walk(root, objs, &mut ids, 0);
    }
    if ids.is_empty() {
        ids = objs
            .iter()
            .filter(|(_, b)| type_names(b).any(|t| t == "Page"))
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
    }
    ids
}

fn walk(id: u32, objs: &HashMap<u32, String>, ids: &mut Vec<u32>, depth: u32) {
    let Some(body) = objs.get(&id).filter(|_| depth <= 16) else {
        return;
    };
    if type_names(body).any(|t| t == "Page") {
        ids.push(id);
        return;
    }
    for kid in kids(body) {
        walk(kid, objs, ids, depth + 1);
    }
}

/// A page attribute, or the nearest ancestor's (`/MediaBox`, `/CropBox`,
/// and `/Rotate` are inheritable).
fn inherited<T>(
    id: u32,
    objs: &HashMap<u32, String>,
    read: &dyn Fn(&str) -> Option<T>,
) -> Option<T> {
    let mut id = id;
    for _ in 0..16 {
        let body = objs.get(&id)?;
        if let Some(value) = read(body) {
            return Some(value);
        }
        id = reference_after(body, "/Parent")?;
    }
    None
}

/// Every `/Type` value in a dictionary, nested ones included.
fn type_names(body: &str) -> impl Iterator<Item = &str> {
    body.match_indices("/Type").filter_map(|(at, _)| {
        let rest = body[at + 5..].trim_start().strip_prefix('/')?;
        let end = rest
            .find(|c: char| c.is_whitespace() || "/<>[]()".contains(c))
            .unwrap_or(rest.len());
        Some(&rest[..end])
    })
}

fn kids(body: &str) -> Vec<u32> {
    let Some(start) = body.find("/Kids") else {
        return Vec::new();
    };
    let list = &body[start..];
    let (Some(open), Some(close)) = (list.find('['), list.find(']')) else {
        return Vec::new();
    };
    let words: Vec<&str> = list[open + 1..close.max(open + 1)]
        .split_whitespace()
        .collect();
    words
        .iter()
        .enumerate()
        .filter(|(_, w)| **w == "R")
        .filter_map(|(i, _)| words.get(i.checked_sub(2)?)?.parse().ok())
        .collect()
}

/// The object number of an `N G R` reference following `key`.
fn reference_after(body: &str, key: &str) -> Option<u32> {
    let at = key_at(body, key)?;
    body[at + key.len()..]
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn number_after(body: &str, key: &str) -> Option<f32> {
    let at = key_at(body, key)?;
    body[at + key.len()..]
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// `key` as a whole name: `/N` does not match `/Names`.
fn key_at(body: &str, key: &str) -> Option<usize> {
    body.match_indices(key).map(|(at, _)| at).find(|&at| {
        !body[at + key.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

fn page_box(body: &str, key: &str) -> Option<(f32, f32)> {
    let at = key_at(body, key)?;
    let rest = body[at + key.len()..].trim_start().strip_prefix('[')?;
    let nums: Vec<f32> = rest[..rest.find(']')?]
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [llx, lly, urx, ury] = nums.as_slice() else {
        return None;
    };
    let (w, h) = ((urx - llx).abs(), (ury - lly).abs());
    (w > 0.0 && h > 0.0).then_some((w, h))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn page(id: u32, box_: &str, extra: &str) -> String {
        format!("{id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox {box_} {extra} >> endobj\n")
    }

    fn pdf(kids: &str, body: &str) -> Vec<u8> {
        format!(
            "%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
             2 0 obj << /Type /Pages /Kids [{kids}] /Count 3 >> endobj\n{body}\
             trailer << /Root 1 0 R >>\n"
        )
        .into_bytes()
    }

    #[test]
    fn mixed_pages_keep_their_own_boxes_and_rotation() {
        let body = page(3, "[0 0 612 792]", "")
            + &page(4, "[0 0 792 612]", "")
            + &page(5, "[0 0 200 400]", "/Rotate 90");
        let bytes = pdf("3 0 R 4 0 R 5 0 R", &body);
        assert_eq!(page_points(&bytes, 0), Some((612.0, 792.0)));
        assert_eq!(page_points(&bytes, 1), Some((792.0, 612.0)));
        assert_eq!(page_points(&bytes, 2), Some((400.0, 200.0)));
    }

    #[test]
    fn page_order_follows_the_tree_and_boxes_inherit() {
        let body = "6 0 obj << /Type /Pages /Parent 2 0 R /Kids [4 0 R 3 0 R] \
                    /MediaBox [0 0 300 100] /Rotate 270 >> endobj\n"
            .to_string()
            + "3 0 obj << /Type /Page /Parent 6 0 R >> endobj\n"
            + &page(4, "[0 0 50 80]", "/CropBox [10 10 40 70]");
        let bytes = pdf("6 0 R", &body);
        assert_eq!(
            page_sizes(&bytes),
            vec![Some((60.0, 30.0)), Some((100.0, 300.0))]
        );
    }

    #[test]
    fn pages_packed_in_a_compressed_object_stream_are_found() {
        let objs = [
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 842 595] >>",
        ];
        let header = format!("3 0 4 {} ", objs[0].len() + 1);
        let packed = format!("{header}{} {}", objs[0], objs[1]);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(packed.as_bytes()).unwrap();
        let stream = z.finish().unwrap();
        let mut bytes = pdf("3 0 R 4 0 R", "").to_vec();
        bytes.extend_from_slice(
            format!(
                "9 0 obj << /Type /ObjStm /N 2 /First {} /Filter /FlateDecode /Length {} >>\nstream\n",
                header.len(),
                stream.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(&stream);
        bytes.extend_from_slice(b"\nendstream\nendobj\n");
        assert_eq!(
            page_sizes(&bytes),
            vec![Some((612.0, 792.0)), Some((842.0, 595.0))]
        );
    }
}
