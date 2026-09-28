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
    /// Sessions whose full access also applies to messages another agent wrote.
    #[serde(default)]
    relay: BTreeSet<String>,
}

/// Per-user store beside the other Atlas app data.
pub fn store_path() -> PathBuf {
    atlas_core::index::data_dir().join("agent-access.json")
}

fn grants_in(path: &Path) -> Grants {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Grants>(&bytes).ok())
        .unwrap_or_default()
}

/// Sessions granted full access in the store at `path`.
pub fn load_in(path: &Path) -> BTreeSet<String> {
    grants_in(path).full
}

pub fn granted_in(path: &Path, session: &str) -> bool {
    load_in(path).contains(session)
}

/// Whether `session` may act without asking, from the per-user store.
pub fn granted(session: &str) -> bool {
    granted_in(&store_path(), session)
}

pub fn relay_granted_in(path: &Path, session: &str) -> bool {
    grants_in(path).relay.contains(session)
}

/// Whether `session`'s full access also covers messages relayed from another
/// agent. Full access itself is still [`granted`].
pub fn relay_granted(session: &str) -> bool {
    relay_granted_in(&store_path(), session)
}

/// Record or withdraw the grant for one session.
pub fn set_in(path: &Path, session: &str, on: bool) -> Result<(), String> {
    update(path, session, on, |g| &mut g.full)
}

/// Record or withdraw the relay extension of one session's grant.
pub fn set_relay_in(path: &Path, session: &str, on: bool) -> Result<(), String> {
    update(path, session, on, |g| &mut g.relay)
}

fn update(
    path: &Path,
    session: &str,
    on: bool,
    set: impl FnOnce(&mut Grants) -> &mut BTreeSet<String>,
) -> Result<(), String> {
    if session.is_empty() {
        return Err("This card has no conversation yet.".into());
    }
    let mut grants = grants_in(path);
    let sessions = set(&mut grants);
    let changed = if on {
        sessions.insert(session.to_string())
    } else {
        sessions.remove(session)
    };
    if !changed {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not save the grant: {e}"))?;
    }
    grants.version = 1;
    crate::agent::atomic_write_json(path, &grants)
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
    fn full_and_relay_grants_survive_each_others_writes() {
        let path = temp_store("relay");
        assert!(!relay_granted_in(&path, "agent-a"));
        set_relay_in(&path, "agent-a", true).unwrap();
        assert!(relay_granted_in(&path, "agent-a"));
        assert!(
            !granted_in(&path, "agent-a"),
            "relay alone is not full access"
        );
        set_in(&path, "agent-a", true).unwrap();
        set_in(&path, "agent-b", true).unwrap();
        assert!(relay_granted_in(&path, "agent-a"), "set_in keeps relay");
        set_relay_in(&path, "agent-a", false).unwrap();
        assert!(!relay_granted_in(&path, "agent-a"));
        assert!(granted_in(&path, "agent-a"), "set_relay_in keeps full");
        assert!(granted_in(&path, "agent-b"));
        set_relay_in(&path, "agent-b", true).unwrap();
        set_in(&path, "agent-b", false).unwrap();
        assert!(relay_granted_in(&path, "agent-b"));
        assert!(!relay_granted_in(&path, "agent-c"));
        assert!(set_relay_in(&path, "", true).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_store_written_before_relay_still_reads() {
        let path = temp_store("legacy");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"version":1,"full":["agent-a"]}"#).unwrap();
        assert!(granted_in(&path, "agent-a"));
        assert!(!relay_granted_in(&path, "agent-a"));
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
