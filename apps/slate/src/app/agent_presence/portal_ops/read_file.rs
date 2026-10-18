use serde_json::{json, Value};
use std::{
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};

pub(super) fn read_text_page(
    root: PathBuf,
    path: PathBuf,
    locator: &str,
    generation: u64,
    expected_size: u64,
    expected_mtime: i64,
    cursor: usize,
    limit: usize,
) -> Result<Value, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "txt"
            | "md"
            | "csv"
            | "tsv"
            | "json"
            | "xml"
            | "yaml"
            | "yml"
            | "toml"
            | "ini"
            | "log"
            | "rs"
            | "py"
            | "js"
            | "ts"
            | "html"
            | "css"
    ) {
        return Err("This portal reader supports bounded UTF-8 text files only".into());
    }
    if atlas_core::cloud::is_dehydrated(&path) {
        return Err("Cloud-only placeholder was skipped to avoid downloading it".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("Could not verify Atlas root: {e}"))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|e| format!("Could not verify file path: {e}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("Resolved file escaped the granted Atlas root".into());
    }
    let metadata = std::fs::metadata(&canonical_path).map_err(|e| e.to_string())?;
    if metadata.len() != expected_size
        || metadata.len() > 64 * 1024 * 1024
        || metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64)
            != expected_mtime
    {
        return Err("File changed since the Atlas scan; refresh before reading".into());
    }
    if cursor as u64 > metadata.len() {
        return Err("Text cursor is past end of file".into());
    }
    let mut file = std::fs::File::open(&canonical_path).map_err(|e| e.to_string())?;
    if cursor > 0 {
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut prefix = vec![0; cursor];
        file.read_exact(&mut prefix).map_err(|e| e.to_string())?;
        std::str::from_utf8(&prefix).map_err(|_| "Cursor must be a UTF-8 character boundary")?;
    }
    file.seek(SeekFrom::Start(cursor as u64))
        .map_err(|e| e.to_string())?;
    let mut bytes = vec![0; limit.saturating_add(4)];
    let n = file.read(&mut bytes).map_err(|e| e.to_string())?;
    bytes.truncate(n);
    while std::str::from_utf8(&bytes).is_err() && !bytes.is_empty() {
        bytes.pop();
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "File is not valid UTF-8")?;
    let used = text.len().min(limit);
    let mut end = used;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let next = cursor + end;
    let after_read = std::fs::metadata(&canonical_path).map_err(|e| e.to_string())?;
    if after_read.len() != expected_size
        || after_read
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs() as i64)
            != expected_mtime
    {
        return Err("File changed while the bounded text page was read".into());
    }
    Ok(json!({
        "locator": locator,
        "text": &text[..end],
        "cursor": cursor,
        "next_cursor": (next < metadata.len() as usize).then_some(next),
        "generation": generation
    }))
}
