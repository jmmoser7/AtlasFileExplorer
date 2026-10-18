//! Conversations the person lets act without asking for permission.
//!
//! The grant is machine-local state for this operating-system user, kept with
//! the other Atlas app data and never written into a workbook, a journal, or
//! the synced agent link folder: a `.slate` you receive must not arrive with an
//! agent already trusted to run commands on your machine. Keys are session
//! folder names (`agent-…`), which survive a moved AI workspace.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Immutable machine-local policy snapshot for cached UI presentation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    full: BTreeSet<String>,
    /// Sessions whose full access also applies to messages another agent wrote.
    #[serde(default)]
    relay: BTreeSet<String>,
    /// Explicit denials survive a later Artifact default change.
    #[serde(default)]
    overrides: BTreeMap<String, bool>,
    #[serde(default)]
    artifact_default_full: bool,
}

/// Per-user store beside the other Atlas app data.
pub fn store_path() -> PathBuf {
    atlas_core::index::data_dir().join("agent-access.json")
}

fn grants_in(path: &Path) -> Snapshot {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok())
        .unwrap_or_default()
}

pub fn load_snapshot_in(path: &Path) -> Snapshot {
    grants_in(path)
}

impl Snapshot {
    pub fn full_sessions(&self) -> BTreeSet<String> {
        let mut full = self.full.clone();
        for (session, allowed) in &self.overrides {
            if *allowed {
                full.insert(session.clone());
            } else {
                full.remove(session);
            }
        }
        full
    }
    pub fn granted(&self, session: &str) -> bool {
        self.overrides
            .get(session)
            .copied()
            .unwrap_or_else(|| self.full.contains(session))
    }
    pub fn artifact_granted(&self, session: &str) -> bool {
        self.overrides
            .get(session)
            .copied()
            .unwrap_or_else(|| self.full.contains(session) || self.artifact_default_full)
    }
    pub fn relay_granted(&self, session: &str) -> bool {
        self.relay.contains(session)
    }
    pub fn artifact_default_full_access(&self) -> bool {
        self.artifact_default_full
    }
}

/// Sessions granted full access in the store at `path`.
pub fn load_in(path: &Path) -> BTreeSet<String> {
    grants_in(path).full_sessions()
}
pub fn granted_in(path: &Path, session: &str) -> bool {
    grants_in(path).granted(session)
}

pub fn artifact_default_full_access() -> bool {
    artifact_default_full_access_in(&store_path())
}

pub fn artifact_default_full_access_in(path: &Path) -> bool {
    grants_in(path).artifact_default_full
}

/// Applies to subsequent Artifact turns only; relay and read-only rules remain separate.
pub fn artifact_granted(session: &str) -> bool {
    artifact_granted_in(&store_path(), session)
}

pub fn artifact_granted_in(path: &Path, session: &str) -> bool {
    grants_in(path).artifact_granted(session)
}

/// Select the grant's scope; turn policy and relay authority are enforced by runtime.
pub fn granted_for_request(session: &str, request: &atlas_agent::AgentRequest) -> bool {
    if request.artifact.is_some() {
        artifact_granted(session)
    } else {
        granted(session)
    }
}

pub fn set_artifact_default_full_access(on: bool) -> Result<(), String> {
    set_artifact_default_full_access_in(&store_path(), on)
}

pub fn set_artifact_default_full_access_in(path: &Path, on: bool) -> Result<(), String> {
    let mut grants = grants_in(path);
    grants.artifact_default_full = on;
    save(path, &mut grants)
}

/// None follows the Artifact default. Some(false) explicitly keeps this session restricted.
pub fn set_artifact_override(session: &str, value: Option<bool>) -> Result<(), String> {
    set_artifact_override_in(&store_path(), session, value)
}

pub fn set_artifact_override_in(
    path: &Path,
    session: &str,
    value: Option<bool>,
) -> Result<(), String> {
    if session.is_empty() {
        return Err("This card has no conversation yet.".into());
    }
    let mut grants = grants_in(path);
    grants.full.remove(session);
    match value {
        Some(on) => {
            grants.overrides.insert(session.into(), on);
        }
        None => {
            grants.overrides.remove(session);
        }
    }
    save(path, &mut grants)
}

/// Whether `session` may act without asking, from the per-user store.
pub fn granted(session: &str) -> bool {
    granted_in(&store_path(), session)
}

pub fn relay_granted_in(path: &Path, session: &str) -> bool {
    grants_in(path).relay_granted(session)
}

/// Whether `session`'s full access also covers messages relayed from another
/// agent. Full access itself is still [`granted`].
pub fn relay_granted(session: &str) -> bool {
    relay_granted_in(&store_path(), session)
}

/// Record or withdraw the grant for one session.
pub fn set_in(path: &Path, session: &str, on: bool) -> Result<(), String> {
    set_artifact_override_in(path, session, Some(on))
}

/// Record or withdraw the relay extension of one session's grant.
pub fn set_relay_in(path: &Path, session: &str, on: bool) -> Result<(), String> {
    update(path, session, on, |g| &mut g.relay)
}

fn update(
    path: &Path,
    session: &str,
    on: bool,
    set: impl FnOnce(&mut Snapshot) -> &mut BTreeSet<String>,
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
    save(path, &mut grants)
}

fn save(path: &Path, grants: &mut Snapshot) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not save the grant: {e}"))?;
    }
    grants.version = 2;
    crate::agent::atomic_write_json(path, grants)
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

    #[test]
    fn artifact_default_preserves_explicit_denials_and_relay_is_independent() {
        let path = temp_store("artifact");
        set_in(&path, "denied", false).unwrap();
        set_relay_in(&path, "denied", true).unwrap();
        set_artifact_default_full_access_in(&path, true).unwrap();
        let snapshot = load_snapshot_in(&path);
        assert!(snapshot.artifact_default_full_access());
        assert!(!snapshot.artifact_granted("denied"));
        assert!(snapshot.relay_granted("denied"));
        assert!(artifact_granted_in(&path, "new"));
        assert!(
            !granted_in(&path, "new"),
            "Artifact default never grants chat access"
        );
        assert!(!artifact_granted_in(&path, "denied"));
        assert!(relay_granted_in(&path, "denied"));
        set_artifact_override_in(&path, "denied", None).unwrap();
        assert!(artifact_granted_in(&path, "denied"));
        set_in(&path, "allowed", true).unwrap();
        set_artifact_default_full_access_in(&path, false).unwrap();
        assert!(
            snapshot.artifact_granted("new"),
            "cached snapshots change only when explicitly refreshed"
        );
        assert!(!load_snapshot_in(&path).artifact_default_full_access());
        assert!(artifact_granted_in(&path, "allowed"));
        assert!(!artifact_granted_in(&path, "new"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
