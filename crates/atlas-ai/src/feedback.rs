//! Agent notes about using Slate.
//!
//! An agent writes `feedback.json` beside `session.json`. Slate keeps every
//! note in `feedback-log.json` in that same link folder. When `gh` is signed
//! in, the note is also filed as an issue on the feedback repo. No account is
//! required: a missing `gh` leaves the local note and says so in
//! `feedback.result.json`.

use atlas_agent::FeedbackNote;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Where notes go when the person has not named another repo.
pub const DEFAULT_REPO: &str = "jmmoser7/slate-agent-feedback";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackLog {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub notes: Vec<FeedbackNote>,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedbackResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Read `feedback.json` if it is there, append it to the local log, try to
/// file it, then remove it. Blocking I/O.
pub fn consume(link_dir: &Path) -> Result<Option<FeedbackResult>, String> {
    let path = link_dir.join("feedback.json");
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read(&path).map_err(|e| format!("Could not read feedback.json: {e}"))?;
    let note = match serde_json::from_slice::<FeedbackNote>(&raw) {
        Ok(note) if !note.what.trim().is_empty() => note,
        Ok(_) => return Err(reject(link_dir, "feedback.json needs a what sentence")),
        Err(e) => return Err(reject(link_dir, &format!("feedback.json: {e}"))),
    };
    if let Some(why) = refused(&note) {
        return Err(reject(link_dir, why));
    }
    let mut log = load(link_dir);
    if !log.notes.iter().any(|n| n == &note) {
        log.notes.push(note.clone());
        crate::agent::atomic_write_json(&link_dir.join("feedback-log.json"), &log)
            .map_err(|e| format!("Could not save the note: {e}"))?;
    }
    let result = file_issue(&note);
    let _ = crate::agent::atomic_write_json(&link_dir.join("feedback.result.json"), &result);
    let _ = std::fs::remove_file(path);
    Ok(Some(result))
}

fn reject(link_dir: &Path, error: &str) -> String {
    let _ = crate::agent::atomic_write_json(
        &link_dir.join("feedback.result.json"),
        &FeedbackResult {
            ok: false,
            filed: None,
            url: None,
            error: Some(error.to_string()),
        },
    );
    let _ = std::fs::remove_file(link_dir.join("feedback.json"));
    error.to_string()
}

/// Notes stay short and free of secrets. The person's prompt is not included.
fn refused(note: &FeedbackNote) -> Option<&'static str> {
    for field in [&note.what, &note.tried, &note.missing] {
        if field.len() > 800 {
            return Some("Keep each feedback field under 800 characters.");
        }
        let lower = field.to_ascii_lowercase();
        if lower.contains("sk-")
            || lower.contains("api_key")
            || lower.contains("api key")
            || lower.contains("cursor_")
            || lower.contains("bearer ")
        {
            return Some("Feedback must not include a key or token.");
        }
    }
    None
}

fn load(link_dir: &Path) -> FeedbackLog {
    std::fs::read(link_dir.join("feedback-log.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(FeedbackLog {
            version: 1,
            notes: Vec::new(),
        })
}

fn repo() -> String {
    std::env::var("ATLAS_FEEDBACK_REPO")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_REPO.to_string())
}

fn file_issue(note: &FeedbackNote) -> FeedbackResult {
    if std::env::var("ATLAS_FEEDBACK").ok().as_deref() == Some("0") {
        return FeedbackResult {
            ok: true,
            filed: Some(false),
            url: None,
            error: Some("Saved locally. GitHub filing is off.".into()),
        };
    }
    let title = format!(
        "Agent: {}",
        note.what.trim().chars().take(72).collect::<String>()
    );
    let body = format!(
        "What: {}\n\nTried: {}\n\nUnreachable: {}\n\nThis note was written by an agent using a Slate board. It has no file contents and no secrets.",
        note.what.trim(),
        dash(&note.tried),
        dash(&note.missing)
    );
    let gh = Command::new("gh")
        .args([
            "issue",
            "create",
            "--repo",
            &repo(),
            "--title",
            &title,
            "--body",
            &body,
        ])
        .output();
    match gh {
        Ok(out) if out.status.success() => {
            let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
            FeedbackResult {
                ok: true,
                filed: Some(true),
                url: (!url.is_empty()).then_some(url),
                error: None,
            }
        }
        Ok(out) => FeedbackResult {
            ok: true,
            filed: Some(false),
            url: None,
            error: Some(clip(&String::from_utf8_lossy(&out.stderr))),
        },
        Err(_) => FeedbackResult {
            ok: true,
            filed: Some(false),
            url: None,
            error: Some("Saved locally. gh is not installed.".into()),
        },
    }
}

fn dash(text: &str) -> &str {
    if text.trim().is_empty() {
        "—"
    } else {
        text.trim()
    }
}

fn clip(text: &str) -> String {
    text.chars().take(240).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "atlas_feedback_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn a_note_is_kept_locally_and_a_secret_is_refused() {
        let link = dir("keep");
        std::env::set_var("ATLAS_FEEDBACK", "0");
        std::fs::write(
            link.join("feedback.json"),
            r#"{"id":"a","what":"Spawning a dashboard took three tries","tried":"return.json","missing":"the gray circle stayed empty"}"#,
        )
        .unwrap();
        let result = consume(&link).unwrap().unwrap();
        assert!(result.ok && result.filed == Some(false));
        assert!(!link.join("feedback.json").exists());
        assert_eq!(load(&link).notes.len(), 1);
        std::fs::write(
            link.join("feedback.json"),
            r#"{"what":"here is sk-abc1234567890"}"#,
        )
        .unwrap();
        assert!(consume(&link).unwrap_err().contains("key"));
        assert_eq!(load(&link).notes.len(), 1, "a refused note is not logged");
        std::env::remove_var("ATLAS_FEEDBACK");
        let _ = std::fs::remove_dir_all(link);
    }
}
