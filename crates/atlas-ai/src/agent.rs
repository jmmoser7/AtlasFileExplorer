//! File-based live link for local agent portals.
//!
//! This module is deliberately vendor-free and scene-model-free. Slate writes
//! context and prompt requests here; any local sidecar can read them and write
//! session state back. Cursor is only one provider resolved by name.

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::config::LINK_DIR;

const WRITE_INTERVAL: Duration = Duration::from_secs(1);
const READ_INTERVAL: Duration = Duration::from_secs(1);

const README: &str = "\
Slate writes `context.json` and prompt `request.json` here for this agent portal. \
Local sidecars write `session.json` with status and turn history, and write board \
edit proposals under `../stage/`. See `docs/agent-link-contract.md` in the \
AtlasFileExplorer repository for the full schema.\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProvider {
    pub id: String,
    pub display_name: String,
    pub launch: LaunchKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchKind {
    Cursor,
    None,
}

pub fn providers() -> Vec<AgentProvider> {
    vec![
        provider_by_id("cursor"),
        AgentProvider {
            id: "local".into(),
            display_name: "Local agent".into(),
            launch: LaunchKind::None,
        },
    ]
}

pub fn provider_by_id(id: &str) -> AgentProvider {
    match id {
        "cursor" => AgentProvider {
            id: "cursor".into(),
            display_name: "Cursor".into(),
            launch: LaunchKind::Cursor,
        },
        _ => AgentProvider {
            id: "local".into(),
            display_name: "Local agent".into(),
            launch: LaunchKind::None,
        },
    }
}

pub fn launch_provider(provider: &str, workspace: &Path) -> Result<(), String> {
    match provider_by_id(provider).launch {
        LaunchKind::Cursor => crate::launch::launch_cursor(workspace),
        LaunchKind::None => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
    pub app: &'static str,
    pub session: String,
    pub provider: String,
    pub workbook: Option<PathBuf>,
    pub format_version: u32,
    pub scope: String,
    pub selection: Vec<String>,
    pub viewport: Option<Viewport>,
    pub board_summary: String,
    pub generated_at: u64,
}

impl AgentContext {
    pub fn fingerprint(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.app.hash(&mut h);
        self.session.hash(&mut h);
        self.provider.hash(&mut h);
        self.workbook.hash(&mut h);
        self.format_version.hash(&mut h);
        self.scope.hash(&mut h);
        self.selection.hash(&mut h);
        self.board_summary.hash(&mut h);
        if let Some(v) = &self.viewport {
            v.hash_into(&mut h);
        }
        h.finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub zoom: f32,
}

impl Viewport {
    fn hash_into(&self, h: &mut DefaultHasher) {
        self.x.to_bits().hash(h);
        self.y.to_bits().hash(h);
        self.w.to_bits().hash(h);
        self.h.to_bits().hash(h);
        self.zoom.to_bits().hash(h);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRequest {
    pub id: String,
    pub prompt: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    #[serde(default)]
    pub status: AgentStatus,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub turns: Vec<AgentTurn>,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    #[default]
    Idle,
    Thinking,
    Offline,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTurn {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Default)]
pub struct AgentLink {
    last_write_attempt: Option<Instant>,
    last_fingerprint: u64,
    last_read_attempt: Option<Instant>,
    last_session_mtime: Option<SystemTime>,
    readmes: BTreeMap<String, bool>,
}

impl AgentLink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tick_write_context(
        &mut self,
        ai_workspace: &Path,
        session: &str,
        ctx: &AgentContext,
    ) -> bool {
        if let Some(t) = self.last_write_attempt {
            if t.elapsed() < WRITE_INTERVAL {
                return false;
            }
        }
        self.last_write_attempt = Some(Instant::now());

        let fp = ctx.fingerprint();
        if fp == self.last_fingerprint {
            return false;
        }
        let dir = agent_dir(ai_workspace, session);
        if std::fs::create_dir_all(&dir).is_err() {
            return false;
        }
        if atomic_write_json(&dir.join("context.json"), ctx).is_err() {
            return false;
        }
        self.last_fingerprint = fp;
        self.write_readme_if_needed(session, &dir);
        true
    }

    pub fn send_request(
        &mut self,
        ai_workspace: &Path,
        session: &str,
        req: &AgentRequest,
    ) -> std::io::Result<()> {
        let dir = agent_dir(ai_workspace, session);
        std::fs::create_dir_all(&dir)?;
        atomic_write_json(&dir.join("request.json"), req)?;
        self.write_readme_if_needed(session, &dir);
        Ok(())
    }

    pub fn tick_read_session(
        &mut self,
        ai_workspace: &Path,
        session: &str,
    ) -> Option<AgentSession> {
        if let Some(t) = self.last_read_attempt {
            if t.elapsed() < READ_INTERVAL {
                return None;
            }
        }
        self.last_read_attempt = Some(Instant::now());

        let path = agent_dir(ai_workspace, session).join("session.json");
        let metadata = std::fs::metadata(&path).ok()?;
        let mtime = metadata.modified().ok()?;
        if self.last_session_mtime == Some(mtime) {
            return None;
        }
        let text = std::fs::read_to_string(&path).ok()?;
        let session = serde_json::from_str::<AgentSession>(&text).ok()?;
        self.last_session_mtime = Some(mtime);
        Some(session)
    }

    fn write_readme_if_needed(&mut self, session: &str, dir: &Path) {
        if self.readmes.get(session).copied().unwrap_or(false) {
            return;
        }
        let path = dir.join("README.md");
        if path.exists() || std::fs::write(&path, README).is_ok() {
            self.readmes.insert(session.to_string(), true);
        }
    }

    #[cfg(test)]
    fn force_elapsed(&mut self) {
        self.last_write_attempt =
            Some(Instant::now() - WRITE_INTERVAL - Duration::from_millis(100));
        self.last_read_attempt = Some(Instant::now() - READ_INTERVAL - Duration::from_millis(100));
    }
}

pub fn agent_dir(ai_workspace: &Path, session: &str) -> PathBuf {
    ai_workspace.join(LINK_DIR).join("agent").join(session)
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atlas_ai_agent_{name}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn context() -> AgentContext {
        AgentContext {
            app: "slate",
            session: "s1".into(),
            provider: "cursor".into(),
            workbook: Some(PathBuf::from("board.slate")),
            format_version: 2,
            scope: "selection".into(),
            selection: vec!["node:1".into()],
            viewport: Some(Viewport {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
                zoom: 1.0,
            }),
            board_summary: "1 selected".into(),
            generated_at: 1,
        }
    }

    #[test]
    fn context_write_is_fingerprint_gated() {
        let ws = temp_workspace("write");
        let mut link = AgentLink::new();
        let ctx = context();
        assert!(link.tick_write_context(&ws, "s1", &ctx));
        assert!(!link.tick_write_context(&ws, "s1", &ctx));
        link.force_elapsed();
        assert!(!link.tick_write_context(&ws, "s1", &ctx));
        assert!(agent_dir(&ws, "s1").join("README.md").exists());
        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn request_and_session_round_trip_on_mtime() {
        let ws = temp_workspace("session");
        let mut link = AgentLink::new();
        let req = AgentRequest {
            id: "r1".into(),
            prompt: "Summarize".into(),
            at: 1,
        };
        link.send_request(&ws, "s1", &req).unwrap();
        let text = std::fs::read_to_string(agent_dir(&ws, "s1").join("request.json")).unwrap();
        assert!(text.contains("Summarize"));

        let session = AgentSession {
            status: AgentStatus::Thinking,
            provider: "cursor".into(),
            turns: vec![AgentTurn {
                role: "assistant".into(),
                text: "Working".into(),
                at: 2,
            }],
            updated_at: 2,
        };
        atomic_write_json(&agent_dir(&ws, "s1").join("session.json"), &session).unwrap();
        link.force_elapsed();
        assert_eq!(link.tick_read_session(&ws, "s1"), Some(session.clone()));
        link.force_elapsed();
        assert_eq!(link.tick_read_session(&ws, "s1"), None);

        std::thread::sleep(Duration::from_millis(1100));
        let mut changed = session;
        changed.status = AgentStatus::Idle;
        atomic_write_json(&agent_dir(&ws, "s1").join("session.json"), &changed).unwrap();
        link.force_elapsed();
        assert_eq!(link.tick_read_session(&ws, "s1"), Some(changed));
        let _ = std::fs::remove_dir_all(ws);
    }
}
