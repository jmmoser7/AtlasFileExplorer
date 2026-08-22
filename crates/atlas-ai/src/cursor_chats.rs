//! Saved Cursor composer chats and IDE agent transcripts for a project folder.
//!
//! This is a **local catalog**, not a live attach to the IDE thread. Slate
//! cannot become the Cursor chat window (Constitution Art. I.2 / VII.8).
//! I/O only — never call [`discover_for`] from the frame loop (Art. II).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::cursor_recents::{cursor_user_dir, file_uri_to_path};

/// One saved composer / chat Cursor stored for a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorChat {
    pub id: String,
    pub title: String,
    pub updated_at: u64,
}

/// Agents this folder already has: IDE transcripts under `.cursor/projects`,
/// plus composer rows in Cursor's workspace DB.
pub fn discover_for(folder: &Path) -> Vec<CursorChat> {
    let mut out = transcripts_for(folder);
    out.extend(composer_chats_for(folder));
    dedupe_sort(out)
}

fn composer_chats_for(folder: &Path) -> Vec<CursorChat> {
    let Some(user) = cursor_user_dir() else {
        return Vec::new();
    };
    let Ok(want) = std::fs::canonicalize(folder) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(user.join("workspaceStorage")) else {
        return out;
    };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !workspace_matches(&dir.join("workspace.json"), &want) {
            continue;
        }
        let db = dir.join("state.vscdb");
        if db.is_file() {
            out.extend(chats_from_vscdb(&db));
        }
    }
    out
}

/// `C:\Users\me\proj` → `c-Users-me-proj` (Cursor's `.cursor/projects` slug).
pub fn cursor_project_slug(path: &Path) -> String {
    let owned = path.to_string_lossy();
    let raw = owned
        .strip_prefix(r"\\?\")
        .or_else(|| owned.strip_prefix("//?/"))
        .unwrap_or(&owned);
    let mut s = raw.replace(['/', '\\'], "-").replace(':', "");
    if let Some(first) = s.chars().next() {
        if first.is_ascii_alphabetic() {
            s.replace_range(..1, &first.to_ascii_lowercase().to_string());
        }
    }
    s.trim_matches('-').to_string()
}

fn cursor_data_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".cursor"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cursor"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cursor"))
    }
    #[cfg(not(any(windows, unix)))]
    {
        None
    }
}

fn transcripts_for(folder: &Path) -> Vec<CursorChat> {
    let Some(root) = cursor_data_dir() else {
        return Vec::new();
    };
    let mut slugs = vec![cursor_project_slug(folder)];
    if let Ok(canon) = std::fs::canonicalize(folder) {
        let slug = cursor_project_slug(&canon);
        if !slugs.iter().any(|s| s == &slug) {
            slugs.push(slug);
        }
    }
    let mut out = Vec::new();
    for slug in slugs {
        let dir = root.join("projects").join(&slug).join("agent-transcripts");
        out.extend(transcripts_in(&dir));
    }
    out
}

fn transcripts_in(dir: &Path) -> Vec<CursorChat> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = match path.file_name().and_then(|n| n.to_str()) {
            Some(id) if !id.is_empty() => id.to_string(),
            _ => continue,
        };
        let jsonl = path.join(format!("{id}.jsonl"));
        if !jsonl.is_file() {
            continue;
        }
        let updated_at = std::fs::metadata(&jsonl)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let title = title_from_transcript(&jsonl).unwrap_or_else(|| format!("Agent {id}"));
        out.push(CursorChat {
            id,
            title,
            updated_at,
        });
    }
    out
}

fn title_from_transcript(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file).read_line(&mut line).ok()?;
    title_from_transcript_line(&line)
}

/// First user prompt in a Cursor agent jsonl line, stripped of chrome tags.
pub fn title_from_transcript_line(line: &str) -> Option<String> {
    let value: Value = serde_json::from_str(line).ok()?;
    let mut texts = Vec::new();
    collect_text_blobs(&value, &mut texts);
    texts.into_iter().find_map(|t| clean_user_query(&t))
}

fn collect_text_blobs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => {
            for v in items {
                collect_text_blobs(v, out);
            }
        }
        Value::Object(map) => {
            if let Some(Value::String(s)) = map.get("text") {
                out.push(s.clone());
            }
            for v in map.values() {
                collect_text_blobs(v, out);
            }
        }
        _ => {}
    }
}

fn clean_user_query(raw: &str) -> Option<String> {
    let mut t = raw;
    if let Some(end) = raw.find("</timestamp>") {
        t = raw[end + "</timestamp>".len()..].trim();
    }
    let t = t
        .replace("<user_query>", "")
        .replace("</user_query>", "")
        .replace('\n', " ");
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return None;
    }
    let mut chars = t.chars();
    let short: String = chars.by_ref().take(72).collect();
    Some(if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    })
}

fn workspace_matches(json: &Path, want: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(json) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    let mut found = Vec::new();
    collect_paths(&value, &mut found);
    found.iter().any(|p| paths_equal(p, want))
}

fn collect_paths(value: &Value, out: &mut Vec<PathBuf>) {
    match value {
        Value::String(s) => {
            if let Some(p) = file_uri_to_path(s) {
                if let Ok(c) = std::fs::canonicalize(&p) {
                    out.push(c);
                } else {
                    out.push(p);
                }
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_paths(v, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_paths(v, out);
            }
        }
        _ => {}
    }
}

fn chats_from_vscdb(path: &Path) -> Vec<CursorChat> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let uri = format!(
        "file:{}?mode=ro",
        path.display().to_string().replace('\\', "/")
    );
    let Ok(conn) = Connection::open_with_flags(&uri, flags) else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT key, value FROM ItemTable WHERE key LIKE '%composer%' OR key LIKE '%Composer%'",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |row| {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        Ok((key, value))
    });
    let Ok(rows) = rows else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in rows.flatten() {
        if let Ok(value) = serde_json::from_str::<Value>(&row.1) {
            collect_chats(&value, &mut out);
        }
    }
    out
}

/// Walk a composer JSON blob and collect chat-like objects.
pub fn collect_chats(value: &Value, out: &mut Vec<CursorChat>) {
    match value {
        Value::Array(items) => {
            for v in items {
                collect_chats(v, out);
            }
        }
        Value::Object(map) => {
            if let Some(chat) = chat_from_obj(map) {
                out.push(chat);
            }
            for v in map.values() {
                collect_chats(v, out);
            }
        }
        _ => {}
    }
}

fn chat_from_obj(map: &serde_json::Map<String, Value>) -> Option<CursorChat> {
    let looks_like = map.contains_key("composerId")
        || map.contains_key("conversationId")
        || (map.contains_key("unifiedMode") && map.contains_key("name"));
    if !looks_like {
        return None;
    }
    let id = map
        .get("composerId")
        .or_else(|| map.get("conversationId"))
        .or_else(|| map.get("id"))
        .and_then(|v| v.as_str())?;
    if id.is_empty() {
        return None;
    }
    let title = map
        .get("name")
        .or_else(|| map.get("title"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Untitled chat")
        .to_string();
    let updated_at = map
        .get("lastUpdatedAt")
        .or_else(|| map.get("updatedAt"))
        .or_else(|| map.get("createdAt"))
        .and_then(json_u64)
        .unwrap_or(0);
    Some(CursorChat {
        id: id.to_string(),
        title,
        updated_at,
    })
}

fn json_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().map(|n| n.max(0) as u64))
        .or_else(|| v.as_f64().map(|n| n.max(0.0) as u64))
}

fn dedupe_sort(mut chats: Vec<CursorChat>) -> Vec<CursorChat> {
    chats.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
    let mut seen = std::collections::HashSet::new();
    chats.retain(|c| seen.insert(c.id.clone()));
    chats.truncate(40);
    chats
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
    fn composer_json_yields_named_chats() {
        let value: Value = serde_json::from_str(
            r#"{
                "allComposers": [
                    {
                        "composerId": "abc",
                        "name": "Climate grid",
                        "lastUpdatedAt": 200,
                        "unifiedMode": "agent"
                    },
                    {
                        "composerId": "def",
                        "name": "Older",
                        "lastUpdatedAt": 100
                    }
                ]
            }"#,
        )
        .unwrap();
        let mut out = Vec::new();
        collect_chats(&value, &mut out);
        let sorted = dedupe_sort(out);
        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted[0].id, "abc");
        assert_eq!(sorted[0].title, "Climate grid");
        assert_eq!(sorted[1].id, "def");
    }

    #[test]
    fn random_objects_with_an_id_are_not_chats() {
        let value: Value = serde_json::from_str(r#"{"id":"nope","foo":1}"#).unwrap();
        let mut out = Vec::new();
        collect_chats(&value, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn a_windows_project_path_slugs_like_cursor() {
        let slug = cursor_project_slug(Path::new(
            r"C:\Users\jmoser\source\repos\AtlasFileExplorer",
        ));
        assert_eq!(slug, "c-Users-jmoser-source-repos-AtlasFileExplorer");
        let verbatim = cursor_project_slug(Path::new(
            r"\\?\C:\Users\jmoser\source\repos\AtlasFileExplorer",
        ));
        assert_eq!(verbatim, "c-Users-jmoser-source-repos-AtlasFileExplorer");
    }

    #[test]
    fn a_transcript_line_yields_the_first_prompt() {
        let line = r#"{"role":"user","message":{"content":[{"type":"text","text":"<timestamp>Saturday, Aug 15, 2026, 8:54 PM (UTC-4)</timestamp>\n<user_query>\nHello hello revisit the agent portal\n</user_query>"}]}}"#;
        let title = title_from_transcript_line(line).expect("title");
        assert_eq!(title, "Hello hello revisit the agent portal");
    }

    #[test]
    fn this_repo_lists_its_cursor_agents_when_present() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(canon) = std::fs::canonicalize(&root) else {
            return;
        };
        let slug = cursor_project_slug(&canon);
        let dir = match cursor_data_dir() {
            Some(home) => home.join("projects").join(&slug).join("agent-transcripts"),
            None => return,
        };
        if !dir.is_dir() {
            return;
        }
        let found = transcripts_in(&dir);
        assert!(
            !found.is_empty(),
            "agent-transcripts exist under {} — discovery must list them",
            dir.display()
        );
        assert!(found.iter().all(|c| !c.title.is_empty()));
    }
}
