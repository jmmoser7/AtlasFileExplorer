//! Page size from a PDF's `/MediaBox`, without loading pdfium.
//!
//! A dropped page is sized from these points. Rendering still goes through
//! pdfium; this only answers width and height, including a `/Rotate` of
//! 90 or 270 and a box inherited from a parent `/Pages` node.

/// Width and height in PDF points for 0-based `page`.
pub fn page_points(bytes: &[u8], page: u16) -> Option<(f32, f32)> {
    let text = String::from_utf8_lossy(bytes);
    let objs = objects(&text);
    let id = *page_ids(&objs).get(page as usize)?;
    let (w, h) = inherited_box(id, &objs, 0)?;
    let turn = objs
        .iter()
        .find(|(n, _)| *n == id)
        .and_then(|(_, body)| rotate_deg(body))
        .unwrap_or(0);
    Some(if turn == 90 || turn == 270 {
        (h, w)
    } else {
        (w, h)
    })
}

fn objects(src: &str) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(at) = rest.find(" obj") {
        let head = &rest[..at];
        let mut words = head.split_whitespace().rev();
        let gen = words.next();
        let id = words.next().and_then(|w| w.parse::<u32>().ok());
        rest = &rest[at + 4..];
        if gen != Some("0") {
            continue;
        }
        let Some(id) = id else { continue };
        let end = rest.find("endobj").unwrap_or(rest.len());
        let mut body = rest[..end].to_string();
        if let Some(stream) = body.find("stream") {
            body.truncate(stream);
        }
        out.push((id, body));
        rest = &rest[end..];
    }
    out
}

fn page_ids(objs: &[(u32, String)]) -> Vec<u32> {
    let root = objs
        .iter()
        .find_map(|(id, body)| is_pages(body).then_some(*id));
    let mut ids = Vec::new();
    if let Some(root) = root {
        walk(root, objs, &mut ids, 0);
    }
    if ids.is_empty() {
        for (id, body) in objs {
            if is_page(body) {
                ids.push(*id);
            }
        }
    }
    ids
}

fn walk(id: u32, objs: &[(u32, String)], ids: &mut Vec<u32>, depth: u32) {
    if depth > 12 {
        return;
    }
    let Some(body) = objs.iter().find(|(n, _)| *n == id).map(|(_, b)| b.as_str()) else {
        return;
    };
    if is_page(body) {
        ids.push(id);
        return;
    }
    for kid in kids(body) {
        walk(kid, objs, ids, depth + 1);
    }
}

fn inherited_box(id: u32, objs: &[(u32, String)], depth: u32) -> Option<(f32, f32)> {
    if depth > 8 {
        return None;
    }
    let body = objs.iter().find(|(n, _)| *n == id)?.1.as_str();
    media_box(body).or_else(|| inherited_box(parent_of(body)?, objs, depth + 1))
}

fn is_pages(body: &str) -> bool {
    body.contains("/Type /Pages") || body.contains("/Type/Pages")
}

fn is_page(body: &str) -> bool {
    let Some(marker) = body.find("/Type /Page").or_else(|| body.find("/Type/Page")) else {
        return false;
    };
    let after = marker
        + if body[marker..].starts_with("/Type/Page") {
            10
        } else {
            11
        };
    !body.get(after..).is_some_and(|rest| rest.starts_with('s'))
}

fn kids(body: &str) -> Vec<u32> {
    let Some(start) = body.find("/Kids") else {
        return Vec::new();
    };
    let Some(rel) = body[start..].find('[') else {
        return Vec::new();
    };
    let open = start + rel;
    let Some(rel) = body[open..].find(']') else {
        return Vec::new();
    };
    body[open + 1..open + rel]
        .split(" R")
        .filter_map(|part| part.split_whitespace().next()?.parse().ok())
        .collect()
}

fn parent_of(body: &str) -> Option<u32> {
    let at = body.find("/Parent")?;
    body[at + "/Parent".len()..]
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn media_box(body: &str) -> Option<(f32, f32)> {
    let at = body.find("/MediaBox")?;
    let open = body[at..].find('[')? + at;
    let close = body[open..].find(']')? + open;
    let nums: Vec<f32> = body[open + 1..close]
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [llx, lly, urx, ury] = nums.as_slice() else {
        return None;
    };
    let w = (urx - llx).abs();
    let h = (ury - lly).abs();
    (w > 0.0 && h > 0.0).then_some((w, h))
}

fn rotate_deg(body: &str) -> Option<i32> {
    let at = body.find("/Rotate")?;
    let rest = body[at + 7..].trim_start();
    let token = rest.split_whitespace().next()?;
    token.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: u32, box_: &str, rotate: &str) -> String {
        format!("{id} 0 obj << /Type /Page /Parent 2 0 R /MediaBox {box_} {rotate} >> endobj\n")
    }

    #[test]
    fn mixed_pages_keep_their_own_boxes_and_rotation() {
        let pdf = format!(
            "%PDF-1.4\n\
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 >> endobj\n\
{}{}{}\
trailer << /Root 1 0 R >>\n",
            page(3, "[0 0 612 792]", ""),
            page(4, "[0 0 792 612]", ""),
            page(5, "[0 0 200 400]", "/Rotate 90"),
        );
        let bytes = pdf.into_bytes();
        assert_eq!(page_points(&bytes, 0), Some((612.0, 792.0)));
        assert_eq!(page_points(&bytes, 1), Some((792.0, 612.0)));
        assert_eq!(page_points(&bytes, 2), Some((400.0, 200.0)));
    }
}
