//! Conversations the person lets act without asking for permission.
//!
//! The grant is machine-local state for this operating-system user, kept with
//! the other Atlas app data and never written into a workbook, a journal, or
//! the synced agent link folder: a `.slate` you receive must not arrive with an
//! agent already trusted to run commands on your machine. Keys are session
//! folder names (`agent-…`), which survive a moved AI workspace.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct Grants {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    full: BTreeSet<String>,
}

/// Per-user store beside the other Atlas app data.
pub fn store_path() -> PathBuf {
    atlas_core::index::data_dir().join("agent-access.json")
}

/// Sessions granted full access in the store at `path`.
pub fn load_in(path: &Path) -> BTreeSet<String> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Grants>(&bytes).ok())
        .map(|g| g.full)
        .unwrap_or_default()
}

pub fn granted_in(path: &Path, session: &str) -> bool {
    load_in(path).contains(session)
}

/// Whether `session` may act without asking, from the per-user store.
pub fn granted(session: &str) -> bool {
    granted_in(&store_path(), session)
}

/// Record or withdraw the grant for one session.
pub fn set_in(path: &Path, session: &str, on: bool) -> Result<(), String> {
    if session.is_empty() {
        return Err("This card has no conversation yet.".into());
    }
    let mut full = load_in(path);
    let changed = if on {
        full.insert(session.to_string())
    } else {
        full.remove(session)
    };
    if !changed {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not save the grant: {e}"))?;
    }
    crate::agent::atomic_write_json(path, &Grants { version: 1, full })
        .map_err(|e| format!("Could not save the grant: {e}"))
}

/// The session folder name of a link dir (`<ws>/.atlas-ai/agent/<session>`).
pub fn session_of(link_dir: &Path) -> Option<String> {
    link_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "atlas_access_{tag}_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
            .join("agent-access.json")
    }

    #[test]
    fn a_grant_persists_per_session_and_can_be_withdrawn() {
        let path = temp_store("grant");
        assert!(!granted_in(&path, "agent-a"));
        set_in(&path, "agent-a", true).unwrap();
        assert!(granted_in(&path, "agent-a"));
        assert!(
            !granted_in(&path, "agent-b"),
            "grants never spread to other sessions"
        );
        set_in(&path, "agent-a", false).unwrap();
        assert!(!granted_in(&path, "agent-a"));
        assert!(set_in(&path, "", true).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_link_dir_names_its_session() {
        assert_eq!(
            session_of(Path::new("C:/ws/.atlas-ai/agent/agent-req-1")).as_deref(),
            Some("agent-req-1")
        );
    }
}
