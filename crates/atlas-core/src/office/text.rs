//! Opening text from Word, Excel, OpenDocument, and RTF files.
//!
//! Used for one placed document at a time. A cloud placeholder is refused
//! before any byte is read. Each zip entry is capped so a large workbook
//! cannot be pulled fully into memory for a short excerpt.

use std::io::{Read, Write};
use std::path::Path;

/// Extensions [`document_excerpt`] knows how to read. Callers that classify
/// files (see `slate-doc::media`) must treat every one of these as a
/// structured text package, so a failed extract does not surface zip bytes.
pub const EXCERPT_EXTENSIONS: &[&str] = &[
    "docx", "docm", "dotx", "dotm", "xlsx", "xlsm", "xltx", "xltm", "odt", "ods", "rtf",
];

const CHAR_CAP: usize = 2000;
const ENTRY_CAP: usize = 512 * 1024;

/// Plain-text opening of a Word, Excel, OpenDocument, or RTF file.
/// `None` when the extension is unsupported, the file is cloud-only, or no
/// text could be read.
pub fn document_excerpt(path: &Path) -> Option<String> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    let text = match ext.as_str() {
        "docx" | "docm" | "dotx" | "dotm" => docx_excerpt(path)?,
        "xlsx" | "xlsm" | "xltx" | "xltm" => xlsx_excerpt(path)?,
        "odt" => odf_excerpt(path, false)?,
        "ods" => odf_excerpt(path, true)?,
        "rtf" => rtf_excerpt(path)?,
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn docx_excerpt(path: &Path) -> Option<String> {
    let xml = read_zip_text(path, "word/document.xml")?;
    let text = word_body(&xml);
    nonempty(text)
}

fn xlsx_excerpt(path: &Path) -> Option<String> {
    let strings = read_zip_text(path, "xl/sharedStrings.xml")
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();
    let sheet = worksheet_xml(path)?;
    nonempty(sheet_grid(&sheet, &strings))
}

fn odf_excerpt(path: &Path, spreadsheet: bool) -> Option<String> {
    let xml = read_zip_text(path, "content.xml")?;
    let text = if spreadsheet {
        ods_grid(&xml)
    } else {
        odf_blocks(&xml)
    };
    nonempty(text)
}

fn rtf_excerpt(path: &Path) -> Option<String> {
    let bytes = read_prefix(path, ENTRY_CAP)?;
    rtf_text(&bytes)
}

fn nonempty(text: String) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

struct Buf {
    text: String,
    chars: usize,
}

impl Buf {
    fn new() -> Self {
        Self {
            text: String::new(),
            chars: 0,
        }
    }

    fn full(&self) -> bool {
        self.chars >= CHAR_CAP
    }

    fn push_char(&mut self, c: char) {
        if !self.full() {
            self.text.push(c);
            self.chars += 1;
        }
    }

    fn push_str(&mut self, s: &str) {
        for c in s.chars() {
            if self.full() {
                break;
            }
            self.push_char(c);
        }
    }

    fn finish(self) -> String {
        self.text
    }
}

fn word_body(xml: &str) -> String {
    let mut out = Buf::new();
    let mut rest = xml;
    while !out.full() {
        let p = rest.find("</w:p>");
        let t = find_tag(rest, "w:t");
        let tab = find_tag(rest, "w:tab");
        let br = find_tag(rest, "w:br").or_else(|| find_tag(rest, "w:cr"));
        let Some(i) = [p, t, tab, br].into_iter().flatten().min() else {
            break;
        };
        rest = &rest[i..];
        if rest.starts_with("</w:p>") {
            if !out.text.is_empty() && !out.text.ends_with('\n') {
                out.push_char('\n');
            }
            rest = &rest["</w:p>".len()..];
        } else if rest.starts_with("<w:tab") {
            out.push_char('\t');
            rest = skip_tag(rest);
        } else if rest.starts_with("<w:br") || rest.starts_with("<w:cr") {
            out.push_char('\n');
            rest = skip_tag(rest);
        } else if rest.starts_with("<w:t") {
            let Some(gt) = rest.find('>') else { break };
            if rest[..gt].ends_with('/') {
                rest = &rest[gt + 1..];
                continue;
            }
            rest = &rest[gt + 1..];
            let Some(end) = rest.find("</w:t>") else {
                break;
            };
            push_xml_text(&mut out, &rest[..end]);
            rest = &rest[end + "</w:t>".len()..];
        } else {
            break;
        }
    }
    out.finish()
}

fn shared_strings(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in xml.split("</si>") {
        if !part.contains("<si") {
            continue;
        }
        let mut buf = Buf::new();
        push_t_nodes(&mut buf, part);
        out.push(buf.finish());
        if out.len() >= 400 {
            break;
        }
    }
    out
}

/// One spreadsheet cell. `fill` is an authored solid color (`xl/styles.xml`);
/// pattern-none and theme-only colors stay `None` so the card can use the
/// canvas theme.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetCell {
    pub text: String,
    pub fill: Option<[u8; 3]>,
}

impl SheetCell {
    fn empty() -> Self {
        Self {
            text: String::new(),
            fill: None,
        }
    }

    /// Ink that stays readable on an authored fill, in either canvas theme.
    pub fn ink_on(fill: [u8; 3]) -> [u8; 3] {
        let y =
            0.2126 * f32::from(fill[0]) + 0.7152 * f32::from(fill[1]) + 0.0722 * f32::from(fill[2]);
        if y >= 160.0 {
            [0x1b, 0x1e, 0x22]
        } else {
            [0xf5, 0xf7, 0xfa]
        }
    }
}

/// First worksheet as rows of text. Empty sheets are `None`.
pub fn xlsx_table(path: &Path, max_rows: usize, max_cols: usize) -> Option<Vec<Vec<String>>> {
    Some(
        xlsx_grid(path, max_rows, max_cols)?
            .into_iter()
            .map(|row| row.into_iter().map(|cell| cell.text).collect())
            .collect(),
    )
}

/// First worksheet, including solid cell fills. A cloud placeholder is `None`.
pub fn xlsx_sheet(path: &Path, max_rows: usize, max_cols: usize) -> Option<Vec<Vec<SheetCell>>> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    xlsx_grid(path, max_rows, max_cols)
}

fn xlsx_grid(path: &Path, max_rows: usize, max_cols: usize) -> Option<Vec<Vec<SheetCell>>> {
    let strings = read_zip_text(path, "xl/sharedStrings.xml")
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();
    let fills = read_zip_text(path, "xl/styles.xml")
        .map(|xml| xf_colors(&xml))
        .unwrap_or_default();
    let sheet = worksheet_xml(path)?;
    let rows = sheet_cells(&sheet, &strings, &fills, max_rows, max_cols);
    if rows.is_empty() {
        None
    } else {
        Some(rows)
    }
}

fn sheet_cells(
    xml: &str,
    strings: &[String],
    fills: &[Option<[u8; 3]>],
    max_rows: usize,
    max_cols: usize,
) -> Vec<Vec<SheetCell>> {
    let mut rows = Vec::new();
    for row in xml.split("</row>") {
        if rows.len() >= max_rows || !row.contains("<row") {
            continue;
        }
        let mut cells = Vec::new();
        for cell in row.split("</c>") {
            let Some((col, value)) = cell_at(cell, strings, fills) else {
                continue;
            };
            let index = if col == usize::MAX { cells.len() } else { col };
            if index >= max_cols {
                continue;
            }
            if cells.len() <= index {
                cells.resize(index + 1, SheetCell::empty());
            }
            cells[index] = value;
        }
        if cells
            .iter()
            .all(|cell| cell.text.is_empty() && cell.fill.is_none())
        {
            continue;
        }
        rows.push(cells);
    }
    rows
}

/// `cellXfs` index → solid fill, when that style has one.
fn xf_colors(xml: &str) -> Vec<Option<[u8; 3]>> {
    let fills = fill_colors(xml);
    let Some(start) = xml.find("<cellXfs") else {
        return Vec::new();
    };
    let rest = &xml[start..];
    let end = rest.find("</cellXfs>").unwrap_or(rest.len());
    rest[..end]
        .split("<xf")
        .skip(1)
        .map(|xf| {
            let id = xml_attr(xf, "fillId")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            fills.get(id).copied().flatten()
        })
        .collect()
}

fn fill_colors(xml: &str) -> Vec<Option<[u8; 3]>> {
    let Some(start) = xml.find("<fills") else {
        return Vec::new();
    };
    let rest = &xml[start..];
    let end = rest.find("</fills>").unwrap_or(rest.len());
    let mut rest = &rest[..end];
    let mut out = Vec::new();
    while let Some(at) = find_tag(rest, "fill") {
        rest = &rest[at..];
        let close = rest.find("</fill>");
        let fill = close.map(|end| &rest[..end]).unwrap_or("");
        let solid = fill.contains("patternType=\"solid\"") || fill.contains("patternType='solid'");
        let color = if solid {
            fill.find("<fgColor")
                .and_then(|i| xml_attr(&fill[i..], "rgb"))
                .and_then(rgb_of)
        } else {
            None
        };
        out.push(color);
        rest = match close {
            Some(end) => &rest[end + "</fill>".len()..],
            None => break,
        };
    }
    out
}

fn rgb_of(raw: &str) -> Option<[u8; 3]> {
    let hex = raw.trim();
    let hex = if hex.len() == 8 { &hex[2..] } else { hex };
    if hex.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

fn xml_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    for quote in ['"', '\''] {
        let key = format!("{name}={quote}");
        let Some(i) = tag.find(&key) else {
            continue;
        };
        let rest = &tag[i + key.len()..];
        let end = rest.find(quote)?;
        return Some(&rest[..end]);
    }
    None
}

/// Column from `r="B2"`. `usize::MAX` means the sheet did not name one.
fn col_index(reference: &str) -> usize {
    let mut n = 0usize;
    let mut any = false;
    for c in reference.chars() {
        if !c.is_ascii_alphabetic() {
            break;
        }
        any = true;
        let v = usize::from(c.to_ascii_uppercase() as u8 - b'A') + 1;
        n = n.saturating_mul(26).saturating_add(v);
    }
    if any {
        n.saturating_sub(1)
    } else {
        usize::MAX
    }
}

fn cell_at(
    cell: &str,
    strings: &[String],
    fills: &[Option<[u8; 3]>],
) -> Option<(usize, SheetCell)> {
    let open = cell.rfind("<c")?;
    let tag = &cell[open..];
    if !(tag.starts_with("<c ") || tag.starts_with("<c>") || tag.starts_with("<c/>")) {
        return None;
    }
    let col = xml_attr(tag, "r").map(col_index).unwrap_or(usize::MAX);
    let style = xml_attr(tag, "s").and_then(|v| v.parse::<usize>().ok());
    let fill = style.and_then(|i| fills.get(i).copied().flatten());
    let text = cell_value(cell, strings).unwrap_or_default();
    let text = text.chars().take(200).collect();
    Some((col, SheetCell { text, fill }))
}

fn sheet_grid(xml: &str, strings: &[String]) -> String {
    let mut out = Buf::new();
    for row in xml.split("</row>") {
        if out.full() || !row.contains("<row") {
            continue;
        }
        let mut cells = Vec::new();
        for cell in row.split("</c>") {
            let Some(value) = cell_value(cell, strings) else {
                continue;
            };
            cells.push(value);
        }
        if cells.iter().all(|c| c.is_empty()) {
            continue;
        }
        if !out.text.is_empty() {
            out.push_char('\n');
        }
        out.push_str(&cells.join("\t"));
    }
    out.finish()
}

fn cell_value(cell: &str, strings: &[String]) -> Option<String> {
    let open = cell.rfind("<c")?;
    let tag = &cell[open..];
    if !(tag.starts_with("<c ") || tag.starts_with("<c>") || tag.starts_with("<c/>")) {
        return None;
    }
    if tag.contains("t=\"inlineStr\"") || tag.contains("t='inlineStr'") {
        let mut buf = Buf::new();
        push_t_nodes(&mut buf, tag);
        return Some(buf.finish());
    }
    let raw = tag_inner(tag, "<v>", "</v>")?.trim();
    if tag.contains("t=\"s\"") || tag.contains("t='s'") {
        let index = raw.parse::<usize>().ok()?;
        return strings.get(index).cloned();
    }
    let mut buf = Buf::new();
    push_xml_text(&mut buf, raw);
    Some(buf.finish())
}

fn odf_blocks(xml: &str) -> String {
    let mut out = Buf::new();
    let mut rest = xml;
    while !out.full() {
        let p = find_tag(rest, "text:p");
        let h = find_tag(rest, "text:h");
        let Some((at, end_tag)) = (match (p, h) {
            (Some(p), Some(h)) if h < p => Some((h, "</text:h>")),
            (Some(p), _) => Some((p, "</text:p>")),
            (None, Some(h)) => Some((h, "</text:h>")),
            _ => None,
        }) else {
            break;
        };
        rest = &rest[at..];
        let Some(gt) = rest.find('>') else { break };
        if rest[..gt].ends_with('/') {
            rest = &rest[gt + 1..];
            continue;
        }
        let body_at = gt + 1;
        let Some(end) = rest[body_at..].find(end_tag) else {
            break;
        };
        if !out.text.is_empty() && !out.text.ends_with('\n') {
            out.push_char('\n');
        }
        let mut plain = Buf::new();
        push_xml_text(&mut plain, &strip_tags(&rest[body_at..body_at + end]));
        out.push_str(plain.text.trim());
        rest = &rest[body_at + end + end_tag.len()..];
    }
    out.finish()
}

fn ods_grid(xml: &str) -> String {
    let mut out = Buf::new();
    for row in xml.split("</table:table-row>") {
        if out.full() || !row.contains("<table:table-row") {
            continue;
        }
        let mut cells = Vec::new();
        for cell in row.split("</table:table-cell>") {
            if !cell.contains("<table:table-cell") {
                continue;
            }
            let mut plain = Buf::new();
            let mut rest = cell;
            while let Some(at) = find_tag(rest, "text:p") {
                rest = &rest[at..];
                let Some(gt) = rest.find('>') else { break };
                let body_at = gt + 1;
                let Some(end) = rest[body_at..].find("</text:p>") else {
                    break;
                };
                if !plain.text.is_empty() {
                    plain.push_char(' ');
                }
                push_xml_text(&mut plain, &strip_tags(&rest[body_at..body_at + end]));
                rest = &rest[body_at + end + "</text:p>".len()..];
            }
            cells.push(plain.finish());
        }
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if !out.text.is_empty() {
            out.push_char('\n');
        }
        out.push_str(&cells.join("\t"));
    }
    out.finish()
}

fn rtf_text(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if !trimmed.starts_with("{\\rtf") {
        return None;
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let mut out = Buf::new();
    let mut i = 0;
    while i < chars.len() && !out.full() {
        match chars[i] {
            '{' | '}' | '\r' | '\n' => i += 1,
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }
                match chars[i] {
                    '\\' | '{' | '}' => {
                        out.push_char(chars[i]);
                        i += 1;
                    }
                    '\'' => {
                        if i + 2 >= chars.len() {
                            break;
                        }
                        let hex: String = chars[i + 1..i + 3].iter().collect();
                        if let Ok(b) = u8::from_str_radix(&hex, 16) {
                            out.push_char(char::from(b));
                        }
                        i += 3;
                    }
                    c if c.is_ascii_alphabetic() => {
                        let start = i;
                        while i < chars.len() && chars[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word: String = chars[start..i].iter().collect();
                        if i < chars.len() && chars[i] == '-' {
                            i += 1;
                        }
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                        if i < chars.len() && chars[i] == ' ' {
                            i += 1;
                        }
                        match word.as_str() {
                            "par" | "line" => out.push_char('\n'),
                            "tab" => out.push_char('\t'),
                            _ => {}
                        }
                    }
                    _ => i += 1,
                }
            }
            c => {
                out.push_char(c);
                i += 1;
            }
        }
    }
    nonempty(out.finish())
}

/// Index of `<name` when the next character starts the tag (`>`, space, `/`).
fn find_tag(xml: &str, name: &str) -> Option<usize> {
    let needle = format!("<{name}");
    let mut rest = xml;
    let mut base = 0;
    while let Some(at) = rest.find(&needle) {
        let next = rest[at + needle.len()..].chars().next();
        if matches!(next, Some('>' | ' ' | '/' | '\n' | '\r' | '\t')) {
            return Some(base + at);
        }
        let skip = at + needle.len();
        base += skip;
        rest = &rest[skip..];
    }
    None
}

fn push_t_nodes(out: &mut Buf, xml: &str) {
    let mut rest = xml;
    while !out.full() {
        let Some(at) = find_tag(rest, "t") else {
            break;
        };
        rest = &rest[at..];
        let Some(gt) = rest.find('>') else { break };
        if rest[..gt].ends_with('/') {
            rest = &rest[gt + 1..];
            continue;
        }
        rest = &rest[gt + 1..];
        let Some(end) = rest.find("</t>") else { break };
        push_xml_text(out, &rest[..end]);
        rest = &rest[end + "</t>".len()..];
    }
}

fn push_xml_text(out: &mut Buf, raw: &str) {
    let mut rest = raw;
    while !out.full() {
        let Some(amp) = rest.find('&') else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        if let Some(end) = rest.find(';') {
            if let Some(ch) = decode_entity(&rest[1..end]) {
                out.push_char(ch);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push_char('&');
        rest = &rest[1..];
    }
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let num = entity.strip_prefix('#')?;
            let code = if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                num.parse().ok()?
            };
            char::from_u32(code)
        }
    }
}

fn strip_tags(xml: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in xml.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn tag_inner<'a>(xml: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = xml.find(open)? + open.len();
    let end = xml[start..].find(close)? + start;
    Some(&xml[start..end])
}

fn skip_tag(xml: &str) -> &str {
    match xml.find('>') {
        Some(i) => &xml[i + 1..],
        None => "",
    }
}

fn read_zip_text(path: &Path, name: &str) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let bytes = read_limited(&mut entry, ENTRY_CAP)?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn worksheet_xml(path: &Path) -> Option<String> {
    if let Some(xml) = read_zip_text(path, "xl/worksheets/sheet1.xml") {
        return Some(xml);
    }
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let mut names: Vec<String> = archive
        .file_names()
        .map(|n| n.replace('\\', "/"))
        .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .collect();
    names.sort();
    let name = names.into_iter().next()?;
    let mut entry = archive.by_name(&name).ok()?;
    let bytes = read_limited(&mut entry, ENTRY_CAP)?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_prefix(path: &Path, cap: usize) -> Option<Vec<u8>> {
    let mut file = std::fs::File::open(path).ok()?;
    read_limited(&mut file, cap)
}

fn read_limited(reader: &mut impl Read, cap: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    while buf.len() < cap {
        let n = reader.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        let take = n.min(cap - buf.len());
        buf.extend_from_slice(&chunk[..take]);
    }
    Some(buf)
}

/// What a cell held before a write. `present` is false when the edit created
/// the cell; putting this value back removes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorCell {
    pub text: String,
    pub present: bool,
    /// Delimited-file row length before the edit. Excel leaves this at 0 and
    /// uses [`Self::present`].
    pub row_len: usize,
}

const SHEET_WRITE_CAP: usize = 2 * 1024 * 1024;

/// Replace one cell on the first worksheet. A number is stored as `<v>`;
/// anything else is an inline string so the shared-string table stays put.
/// The cell's style index is kept. A cloud placeholder is `None`.
pub fn set_xlsx_cell(path: &Path, row: usize, col: usize, text: &str) -> Option<PriorCell> {
    mutate_xlsx_cell(path, row, col, Some(text))
}

/// Remove one cell. Returns the value that was there, marked present, so the
/// caller can write it back.
pub fn clear_xlsx_cell(path: &Path, row: usize, col: usize) -> Option<PriorCell> {
    mutate_xlsx_cell(path, row, col, None)
}

/// Index of the column an "add column" action should create (one past the
/// rightmost cell). Empty sheets return 0.
pub fn xlsx_next_column(path: &Path) -> Option<usize> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let (name, xml) = read_worksheet(path)?;
    let _ = name;
    Some(rightmost_column(&xml).map(|c| c + 1).unwrap_or(0))
}

fn mutate_xlsx_cell(path: &Path, row: usize, col: usize, text: Option<&str>) -> Option<PriorCell> {
    if crate::cloud::is_dehydrated(path) {
        return None;
    }
    let (entry, xml) = read_worksheet(path)?;
    let strings = read_zip_text(path, "xl/sharedStrings.xml")
        .map(|xml| shared_strings(&xml))
        .unwrap_or_default();
    let (new_xml, prior) = match text {
        Some(text) => upsert_cell(&xml, &strings, row, col, text)?,
        None => remove_cell(&xml, &strings, row, col)?,
    };
    rewrite_zip_entry(path, &entry, new_xml.as_bytes())?;
    Some(prior)
}

fn read_worksheet(path: &Path) -> Option<(String, String)> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let name = worksheet_entry_name(&mut archive)?;
    let mut entry = archive.by_name(&name).ok()?;
    let bytes = read_capped(&mut entry, SHEET_WRITE_CAP)?;
    let xml = String::from_utf8(bytes).ok()?;
    Some((name, xml))
}

fn worksheet_entry_name(
    archive: &mut zip::ZipArchive<impl Read + std::io::Seek>,
) -> Option<String> {
    let names: Vec<String> = archive.file_names().map(|n| n.to_string()).collect();
    if let Some(name) = names
        .iter()
        .find(|n| n.replace('\\', "/") == "xl/worksheets/sheet1.xml")
    {
        return Some(name.clone());
    }
    let mut sheets: Vec<String> = names
        .into_iter()
        .filter(|n| {
            let n = n.replace('\\', "/");
            n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")
        })
        .collect();
    sheets.sort();
    sheets.into_iter().next()
}

fn read_capped(reader: &mut impl Read, cap: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    while buf.len() <= cap {
        let n = reader.read(&mut chunk).ok()?;
        if n == 0 {
            return Some(buf);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    None
}

fn rewrite_zip_entry(path: &Path, entry_name: &str, bytes: &[u8]) -> Option<()> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(file)).ok()?;
    let tmp = path.with_extension("slate-sheet-tmp");
    let out = std::fs::File::create(&tmp).ok()?;
    let mut writer = zip::ZipWriter::new(out);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let target = entry_name.replace('\\', "/");
    let len = archive.len();
    for i in 0..len {
        let entry = archive.by_index(i).ok()?;
        let name = entry.name().replace('\\', "/");
        if name == target {
            let stored = entry.name().to_string();
            drop(entry);
            writer.start_file(&stored, opts).ok()?;
            writer.write_all(bytes).ok()?;
        } else {
            writer.raw_copy_file(entry).ok()?;
        }
    }
    writer.finish().ok()?;
    replace_file(&tmp, path)
}

fn replace_file(tmp: &Path, dest: &Path) -> Option<()> {
    if std::fs::rename(tmp, dest).is_ok() {
        return Some(());
    }
    let bak = dest.with_extension("slate-sheet-bak");
    if std::fs::rename(dest, &bak).is_err() {
        let _ = std::fs::remove_file(tmp);
        return None;
    }
    if std::fs::rename(tmp, dest).is_err() {
        let _ = std::fs::rename(&bak, dest);
        let _ = std::fs::remove_file(tmp);
        return None;
    }
    let _ = std::fs::remove_file(&bak);
    Some(())
}

fn upsert_cell(
    xml: &str,
    strings: &[String],
    row: usize,
    col: usize,
    text: &str,
) -> Option<(String, PriorCell)> {
    let reference = cell_ref(col, row);
    let replacement = cell_element(&reference, None, text);
    if let Some(inner) = find_row_inner(xml, row + 1) {
        if let Some(span) = find_cell(xml, inner, &reference) {
            let old = &xml[span.0..span.1];
            let style = xml_attr(old, "s").map(|s| s.to_string());
            let prior = PriorCell {
                text: cell_value(old, strings).unwrap_or_default(),
                present: true,
                row_len: 0,
            };
            let replacement = cell_element(&reference, style.as_deref(), text);
            return Some((splice(xml, span, &replacement), prior));
        }
        let prior = PriorCell {
            text: String::new(),
            present: false,
            row_len: 0,
        };
        let at = inner.1;
        return Some((
            expand_dimension(&splice(xml, (at, at), &replacement), &reference),
            prior,
        ));
    }
    let row_xml = format!("<row r=\"{}\">{replacement}</row>", row + 1);
    let at = xml.rfind("</sheetData>")?;
    let prior = PriorCell {
        text: String::new(),
        present: false,
        row_len: 0,
    };
    Some((
        expand_dimension(&splice(xml, (at, at), &row_xml), &reference),
        prior,
    ))
}

fn remove_cell(
    xml: &str,
    strings: &[String],
    row: usize,
    col: usize,
) -> Option<(String, PriorCell)> {
    let reference = cell_ref(col, row);
    let inner = find_row_inner(xml, row + 1)?;
    let span = find_cell(xml, inner, &reference)?;
    let old = &xml[span.0..span.1];
    let prior = PriorCell {
        text: cell_value(old, strings).unwrap_or_default(),
        present: true,
        row_len: 0,
    };
    let mut xml = splice(xml, span, "");
    if let Some(inner) = find_row_inner(&xml, row + 1) {
        if !xml[inner.0..inner.1].contains("<c") {
            let start = xml[..inner.0].rfind("<row")?;
            let end = inner.1 + "</row>".len();
            xml = splice(&xml, (start, end), "");
        }
    }
    Some((xml, prior))
}

fn splice(xml: &str, span: (usize, usize), replacement: &str) -> String {
    let mut out = String::with_capacity(xml.len() + replacement.len());
    out.push_str(&xml[..span.0]);
    out.push_str(replacement);
    out.push_str(&xml[span.1..]);
    out
}

fn cell_element(reference: &str, style: Option<&str>, text: &str) -> String {
    let style = style
        .map(|s| format!(" s=\"{}\"", xml_escape(s)))
        .unwrap_or_default();
    let text = text.trim();
    if plain_number(text) {
        format!("<c r=\"{reference}\"{style}><v>{text}</v></c>")
    } else {
        format!(
            "<c r=\"{reference}\"{style} t=\"inlineStr\"><is><t>{}</t></is></c>",
            xml_escape(text)
        )
    }
}

fn plain_number(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let rest = text.strip_prefix('-').unwrap_or(text);
    if rest.is_empty() {
        return false;
    }
    let mut dot = false;
    for (i, c) in rest.chars().enumerate() {
        if c == '.' {
            if dot || i == 0 || i + 1 == rest.len() {
                return false;
            }
            dot = true;
        } else if !c.is_ascii_digit() {
            return false;
        }
    }
    let digits = rest.split('.').next().unwrap_or("");
    !(digits.len() > 1 && digits.starts_with('0'))
}

fn xml_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn cell_ref(col: usize, row: usize) -> String {
    format!("{}{}", column_name(col), row + 1)
}

fn column_name(col: usize) -> String {
    let mut n = col + 1;
    let mut bytes = Vec::new();
    while n > 0 {
        n -= 1;
        bytes.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    bytes.reverse();
    String::from_utf8(bytes).unwrap_or_else(|_| "A".into())
}

fn find_row_inner(xml: &str, row_n: usize) -> Option<(usize, usize)> {
    let mut rest = 0usize;
    let mut ordinal = 0usize;
    while let Some(rel) = xml[rest..].find("<row") {
        let start = rest + rel;
        let boundary = xml.as_bytes().get(start + 4).copied();
        if !matches!(boundary, Some(b' ' | b'>' | b'/')) {
            rest = start + 4;
            continue;
        }
        let gt = xml[start..].find('>')? + start;
        if xml.as_bytes().get(gt.wrapping_sub(1)) == Some(&b'/') {
            rest = gt + 1;
            continue;
        }
        let close = xml[gt + 1..].find("</row>")? + gt + 1;
        let named = xml_attr(&xml[start..gt], "r").and_then(|s| s.parse::<usize>().ok());
        let hit = match named {
            Some(n) => n == row_n,
            None => {
                ordinal += 1;
                ordinal == row_n
            }
        };
        if hit {
            return Some((gt + 1, close));
        }
        rest = close + "</row>".len();
    }
    None
}

fn find_cell(xml: &str, inner: (usize, usize), reference: &str) -> Option<(usize, usize)> {
    let mut i = inner.0;
    while i < inner.1 {
        let Some(rel) = xml[i..inner.1].find("<c") else {
            break;
        };
        let start = i + rel;
        let boundary = xml.as_bytes().get(start + 2).copied();
        if !matches!(boundary, Some(b' ' | b'>' | b'/')) {
            i = start + 2;
            continue;
        }
        let gt = xml[start..inner.1].find('>')? + start;
        let tag = &xml[start..=gt];
        let end = if tag.ends_with("/>") {
            gt + 1
        } else {
            let close = xml[gt + 1..inner.1].find("</c>")? + gt + 1;
            close + "</c>".len()
        };
        if xml_attr(tag, "r") == Some(reference) {
            return Some((start, end));
        }
        i = end;
    }
    None
}

fn rightmost_column(xml: &str) -> Option<usize> {
    let mut max: Option<usize> = None;
    for quote in ['"', '\''] {
        let key = format!("r={quote}");
        let mut rest = xml;
        while let Some(i) = rest.find(&key) {
            rest = &rest[i + key.len()..];
            let Some(end) = rest.find(quote) else { break };
            let col = col_index(&rest[..end]);
            if col != usize::MAX {
                max = Some(max.map(|m| m.max(col)).unwrap_or(col));
            }
            rest = &rest[end..];
        }
    }
    max
}

fn expand_dimension(xml: &str, reference: &str) -> String {
    let Some(at) = xml.find("<dimension") else {
        return xml.to_string();
    };
    let Some(gt) = xml[at..].find('>') else {
        return xml.to_string();
    };
    let gt = at + gt;
    let tag = &xml[at..=gt];
    let Some(existing) = xml_attr(tag, "ref") else {
        return xml.to_string();
    };
    let Some(updated) = expand_ref(existing, reference) else {
        return xml.to_string();
    };
    if updated == existing {
        return xml.to_string();
    }
    let key_at = tag.find("ref=").unwrap() + at;
    let quote = xml.as_bytes()[key_at + 4] as char;
    let value_at = key_at + 5;
    let Some(value_end) = xml[value_at..].find(quote) else {
        return xml.to_string();
    };
    splice(xml, (value_at, value_at + value_end), &updated)
}

fn expand_ref(existing: &str, cell: &str) -> Option<String> {
    let (cell_c, cell_r) = parse_a1(cell)?;
    let (left, right) = existing.split_once(':').unwrap_or((existing, existing));
    let (c0, r0) = parse_a1(left)?;
    let (c1, r1) = parse_a1(right)?;
    let min_c = c0.min(c1).min(cell_c);
    let max_c = c0.max(c1).max(cell_c);
    let min_r = r0.min(r1).min(cell_r);
    let max_r = r0.max(r1).max(cell_r);
    if min_c == c0.min(c1) && max_c == c0.max(c1) && min_r == r0.min(r1) && max_r == r0.max(r1) {
        return Some(existing.to_string());
    }
    Some(format!(
        "{}:{}",
        cell_ref(min_c, min_r),
        cell_ref(max_c, max_r)
    ))
}

fn parse_a1(reference: &str) -> Option<(usize, usize)> {
    let col = col_index(reference);
    if col == usize::MAX {
        return None;
    }
    let digits: String = reference
        .chars()
        .skip_while(|c| c.is_ascii_alphabetic())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let row: usize = digits.parse().ok()?;
    if row == 0 {
        return None;
    }
    Some((col, row - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_with(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nfa-text-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        for (entry, body) in files {
            zip.start_file(*entry, opts).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn docx_excerpt_keeps_paragraphs_and_entities() {
        let path = zip_with(
            "note.docx",
            &[(
                "word/document.xml",
                r#"<w:document><w:body>
                <w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p>
                <w:p><w:r><w:t>Second &amp; line</w:t></w:r></w:p>
                </w:body></w:document>"#,
            )],
        );
        assert_eq!(
            document_excerpt(&path).as_deref(),
            Some("Hello world\nSecond & line")
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn xlsx_excerpt_is_a_grid() {
        let path = zip_with(
            "book.xlsx",
            &[
                (
                    "xl/sharedStrings.xml",
                    r#"<sst><si><t>Name</t></si><si><r><t>Ada</t></r><r><t> Lovelace</t></r></si></sst>"#,
                ),
                (
                    "xl/worksheets/sheet1.xml",
                    r#"<worksheet><sheetData>
                    <row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>2</v></c></row>
                    <row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2" t="inlineStr"><is><t>ok</t></is></c></row>
                    </sheetData></worksheet>"#,
                ),
            ],
        );
        assert_eq!(
            document_excerpt(&path).as_deref(),
            Some("Name\t2\nAda Lovelace\tok")
        );
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn xlsx_sheet_keeps_a_solid_fill_and_the_column() {
        let path = zip_with(
            "colored.xlsx",
            &[
                (
                    "xl/styles.xml",
                    r#"<styleSheet><fills count="3">
                    <fill><patternFill patternType="none"/></fill>
                    <fill><patternFill patternType="gray125"/></fill>
                    <fill><patternFill patternType="solid"><fgColor rgb="FF2D6A4F"/></patternFill></fill>
                    </fills><cellXfs count="2">
                    <xf fillId="0"/>
                    <xf fillId="2" applyFill="1"/>
                    </cellXfs></styleSheet>"#,
                ),
                (
                    "xl/sharedStrings.xml",
                    r#"<sst><si><t>Status</t></si><si><t>Open</t></si></sst>"#,
                ),
                (
                    "xl/worksheets/sheet1.xml",
                    r#"<worksheet><sheetData>
                    <row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1" t="s" s="1"><v>1</v></c></row>
                    </sheetData></worksheet>"#,
                ),
            ],
        );
        let rows = xlsx_sheet(&path, 8, 6).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].text, "Status");
        assert_eq!(rows[0][0].fill, None);
        assert_eq!(rows[0][1].text, "");
        assert_eq!(rows[0][2].text, "Open");
        assert_eq!(rows[0][2].fill, Some([0x2d, 0x6a, 0x4f]));
        assert_eq!(SheetCell::ink_on([0x2d, 0x6a, 0x4f]), [0xf5, 0xf7, 0xfa]);
        let number = set_xlsx_cell(&path, 0, 2, "9").unwrap();
        assert_eq!(number.text, "Open");
        assert!(number.present);
        let rows = xlsx_sheet(&path, 8, 6).unwrap();
        assert_eq!(rows[0][2].text, "9");
        assert_eq!(rows[0][2].fill, Some([0x2d, 0x6a, 0x4f]));
        let added = set_xlsx_cell(&path, 0, 3, "Qty").unwrap();
        assert!(!added.present);
        assert_eq!(xlsx_next_column(&path), Some(4));
        let rows = xlsx_sheet(&path, 8, 8).unwrap();
        assert_eq!(rows[0][3].text, "Qty");
        let removed = clear_xlsx_cell(&path, 0, 3).unwrap();
        assert_eq!(removed.text, "Qty");
        assert!(removed.present);
        let rows = xlsx_sheet(&path, 8, 8).unwrap();
        assert_eq!(rows[0].len(), 3);
        assert_eq!(rows[0][2].text, "9");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn odt_excerpt_reads_paragraphs() {
        let path = zip_with(
            "note.odt",
            &[(
                "content.xml",
                r#"<office:document><text:p>Alpha</text:p><text:h>Beta &lt;2&gt;</text:h></office:document>"#,
            )],
        );
        assert_eq!(document_excerpt(&path).as_deref(), Some("Alpha\nBeta <2>"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rtf_excerpt_drops_control_words() {
        let dir = std::env::temp_dir().join(format!(
            "nfa-rtf-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("note.rtf");
        std::fs::write(&path, r"{\rtf1\ansi Hello \b world\b0\par Second}").unwrap();
        assert_eq!(
            document_excerpt(&path).as_deref(),
            Some("Hello world\nSecond")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn csv_and_broken_zip_are_not_office_text() {
        let dir = std::env::temp_dir().join(format!(
            "nfa-csv-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let csv = dir.join("rows.csv");
        std::fs::write(&csv, "a,b\n1,2").unwrap();
        assert!(document_excerpt(&csv).is_none());
        let fake = dir.join("essay.docx");
        std::fs::write(&fake, b"PK fake").unwrap();
        assert!(document_excerpt(&fake).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
