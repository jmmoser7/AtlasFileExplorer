//! Best-effort read of folders Cursor has opened recently.
//!
//! I/O only — never call from the frame loop (Constitution Art. II). Slate
//! merges this list with its own agent-project MRU on a worker thread.

use std::path::{Path, PathBuf};

/// Existing directories Cursor appears to have used as project roots.
pub fn discover() -> Vec<PathBuf> {
    let Some(user) = cursor_user_dir() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if let Ok(text) = std::fs::read_to_string(user.join("globalStorage").join("storage.json")) {
        if let Ok(value) = serde_json::from_str(&text) {
            collect_file_uris(&value, &mut found);
        }
    }
    if let Ok(rd) = std::fs::read_dir(user.join("workspaceStorage")) {
        for entry in rd.flatten() {
            let json = entry.path().join("workspace.json");
            let Ok(text) = std::fs::read_to_string(json) else {
                continue;
            };
            let Ok(value) = serde_json::from_str(&text) else {
                continue;
            };
            collect_file_uris(&value, &mut found);
        }
    }
    dedupe_existing_dirs(found)
}

pub fn cursor_user_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Cursor").join("User"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join("Library")
                .join("Application Support")
                .join("Cursor")
                .join("User")
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".config").join("Cursor").join("User"))
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

fn collect_file_uris(value: &serde_json::Value, out: &mut Vec<PathBuf>) {
    match value {
        serde_json::Value::String(s) => {
            if let Some(p) = file_uri_to_path(s) {
                out.push(p);
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                collect_file_uris(v, out);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values() {
                collect_file_uris(v, out);
            }
        }
        _ => {}
    }
}

/// `file:///C:/Users/foo` / `file:///c%3A/Users/foo` → a filesystem path.
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    let path = if cfg!(windows) {
        let trimmed = decoded.trim_start_matches('/');
        PathBuf::from(trimmed.replace('/', "\\"))
    } else {
        PathBuf::from(decoded)
    };
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn dedupe_existing_dirs(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if !path.is_dir() {
            continue;
        }
        let canon = std::fs::canonicalize(&path).unwrap_or(path);
        if out.iter().any(|p: &PathBuf| paths_equal(p, &canon)) {
            continue;
        }
        out.push(canon);
        if out.len() >= 40 {
            break;
        }
    }
    out
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    #[cfg(windows)]
    {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_file_uri_decodes_to_a_path() {
        let p = file_uri_to_path("file:///C:/Users/jmoser/climate-grid").unwrap();
        let s = p.to_string_lossy();
        assert!(s.contains("climate-grid"), "{s}");
        assert!(s.contains("Users") || s.contains("users"), "{s}");
    }

    #[test]
    fn a_percent_encoded_drive_letter_decodes() {
        let p = file_uri_to_path("file:///c%3A/work/atlas").unwrap();
        let s = p.to_string_lossy().to_ascii_lowercase();
        assert!(s.contains("work"), "{s}");
        assert!(s.contains("atlas"), "{s}");
    }
}
