//! File-based live link for local agent portals.
//!
//! This module is deliberately vendor-free and scene-model-free. Slate writes
//! context and prompt requests here; any local sidecar can read them and write
//! session state back. Cursor is only one provider resolved by name.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::config::LINK_DIR;

const WRITE_INTERVAL: Duration = Duration::from_secs(1);
const READ_INTERVAL: Duration = Duration::from_secs(1);
const STREAM_READ_INTERVAL: Duration = Duration::from_millis(100);

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
    #[serde(default)]
    pub view: PortalView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchKind {
    Cursor,
    Codex,
    None,
}

pub fn providers() -> Vec<AgentProvider> {
    ["cursor", "codex", "local", "image-link"]
        .into_iter()
        .map(provider_by_id)
        .collect()
}

pub fn provider_by_id(id: &str) -> AgentProvider {
    let (name, launch) = match id {
        "cursor" => ("Cursor", LaunchKind::Cursor),
        "codex" => ("Codex", LaunchKind::Codex),
        "ollama" => ("Ollama", LaunchKind::None),
        "local" => ("Local agent", LaunchKind::None),
        "image-link" => ("Image link", LaunchKind::None),
        _ => (id, LaunchKind::None),
    };
    AgentProvider {
        id: id.into(),
        display_name: name.into(),
        launch,
        view: if id == "image-link" {
            PortalView::Images
        } else {
            PortalView::Chat
        },
    }
}

pub fn launch_provider(provider: &str, workspace: &Path) -> Result<(), String> {
    match provider_by_id(provider).launch {
        LaunchKind::Cursor => crate::launch::launch_cursor(workspace),
        LaunchKind::Codex => crate::runtime::launch_codex(workspace),
        LaunchKind::None => Err(
            "This sidecar has no application launcher. Open its link folder to configure it."
                .into(),
        ),
    }
}

pub use atlas_agent::*;

enum LinkWork {
    Context(PathBuf, AgentContext),
    Request(PathBuf, AgentRequest),
    Read(PathBuf),
}

/// One worker and shared immutable snapshot per linked source, regardless of how
/// many cards project it. The caller's NodeIds never become runtime identities.
#[derive(Default)]
pub struct AgentSources {
    links: std::collections::HashMap<PathBuf, AgentLink>,
    snapshots: std::collections::HashMap<PathBuf, std::sync::Arc<AgentSession>>,
}
impl AgentSources {
    pub fn retain(&mut self, dirs: &std::collections::HashSet<PathBuf>) {
        self.links.retain(|dir, _| dirs.contains(dir));
        self.snapshots.retain(|dir, _| dirs.contains(dir));
    }
    pub fn send(&mut self, dir: &Path, request: &AgentRequest) -> std::io::Result<()> {
        self.links
            .entry(dir.into())
            .or_default()
            .send_request_in(dir, request)
    }
    pub fn poll(
        &mut self,
        dir: &Path,
        context: Option<&AgentContext>,
    ) -> Option<std::sync::Arc<AgentSession>> {
        let link = self.links.entry(dir.into()).or_default();
        if let Some(context) = context {
            link.tick_write_context_in(dir, context);
        }
        if let Some(session) = link.tick_read_session_file(&dir.join("session.json")) {
            self.snapshots
                .insert(dir.into(), std::sync::Arc::new(session));
        }
        self.snapshots.get(dir).cloned()
    }
}

/// The UI only exchanges small messages. All filesystem work stays on a worker.
pub struct AgentLink {
    tx: crossbeam_channel::Sender<LinkWork>,
    latest: std::sync::Arc<std::sync::Mutex<Option<AgentSession>>>,
    next_read: Option<Instant>,
    next_write: Option<Instant>,
    streaming: bool,
}
impl Default for AgentLink {
    fn default() -> Self {
        Self::new()
    }
}
impl AgentLink {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let latest = std::sync::Arc::new(std::sync::Mutex::new(None));
        let result = latest.clone();
        std::thread::spawn(move || {
            let mut link = FileAgentLink::new();
            while let Ok(work) = rx.recv() {
                let update = match work {
                    LinkWork::Context(dir, ctx) => {
                        link.tick_write_context_in(&dir, &ctx);
                        None
                    }
                    LinkWork::Request(dir, req) => {
                        link.send_request_in(&dir, &req)
                            .err()
                            .map(|e| AgentSession {
                                approval: None,
                                conversation: String::new(),
                                artifacts: Vec::new(),
                                request: req.id.clone(),
                                status: AgentStatus::Error(format!(
                                    "Could not write agent request: {e}"
                                )),
                                provider: String::new(),
                                turns: vec![],
                                updated_at: req.at,
                                bundle: Default::default(),
                            })
                    }
                    LinkWork::Read(path) => link.tick_read_session_file(&path),
                };
                if update.is_some() {
                    if let Ok(mut value) = result.lock() {
                        *value = update;
                    }
                }
            }
        });
        Self {
            tx,
            latest,
            next_read: None,
            next_write: None,
            streaming: false,
        }
    }
    pub fn tick_write_context(&mut self, ws: &Path, id: &str, ctx: &AgentContext) -> bool {
        self.tick_write_context_in(&agent_dir(ws, id), ctx)
    }
    pub fn tick_write_context_in(&mut self, dir: &Path, ctx: &AgentContext) -> bool {
        if self
            .next_write
            .is_some_and(|t| t.elapsed() < WRITE_INTERVAL)
        {
            return false;
        }
        self.next_write = Some(Instant::now());
        self.tx
            .try_send(LinkWork::Context(dir.into(), ctx.clone()))
            .is_ok()
    }
    pub fn send_request(&mut self, ws: &Path, id: &str, req: &AgentRequest) -> std::io::Result<()> {
        self.send_request_in(&agent_dir(ws, id), req)
    }
    pub fn send_request_in(&mut self, dir: &Path, req: &AgentRequest) -> std::io::Result<()> {
        self.streaming = true;
        self.next_read = None;
        self.tx
            .try_send(LinkWork::Request(dir.into(), req.clone()))
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "Agent link is busy. Try again.",
                )
            })
    }
    pub fn tick_read_session(&mut self, ws: &Path, id: &str) -> Option<AgentSession> {
        self.tick_read_session_file(&agent_dir(ws, id).join("session.json"))
    }
    pub fn tick_read_session_file(&mut self, path: &Path) -> Option<AgentSession> {
        let interval = if self.streaming {
            STREAM_READ_INTERVAL
        } else {
            READ_INTERVAL
        };
        if self.next_read.is_none_or(|t| t.elapsed() >= interval) {
            self.next_read = Some(Instant::now());
            let _ = self.tx.try_send(LinkWork::Read(path.into()));
        }
        let update = self.latest.try_lock().ok()?.take();
        if let Some(session) = &update {
            self.streaming = matches!(session.status, AgentStatus::Thinking);
        }
        update
    }
}

#[derive(Debug, Default)]
struct FileAgentLink {
    last_write_attempt: Option<Instant>,
    last_fingerprint: u64,
    last_read_attempt: Option<Instant>,
    last_session_mtime: Option<SystemTime>,
    readmes: BTreeMap<String, bool>,
}

impl FileAgentLink {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn tick_write_context(
        &mut self,
        ai_workspace: &Path,
        session: &str,
        ctx: &AgentContext,
    ) -> bool {
        self.tick_write_context_in(&agent_dir(ai_workspace, session), ctx)
    }
    fn tick_write_context_in(&mut self, dir: &Path, ctx: &AgentContext) -> bool {
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
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        if atomic_write_json(&dir.join("context.json"), ctx).is_err() {
            return false;
        }
        self.last_fingerprint = fp;
        self.write_readme_if_needed(&dir.to_string_lossy(), dir);
        true
    }

    #[cfg(test)]
    pub fn send_request(
        &mut self,
        ai_workspace: &Path,
        session: &str,
        req: &AgentRequest,
    ) -> std::io::Result<()> {
        self.send_request_in(&agent_dir(ai_workspace, session), req)
    }
    fn send_request_in(&mut self, dir: &Path, req: &AgentRequest) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        atomic_write_json(&dir.join("request.json"), req)?;
        self.write_readme_if_needed(&dir.to_string_lossy(), dir);
        Ok(())
    }

    #[cfg(test)]
    pub fn tick_read_session(
        &mut self,
        ai_workspace: &Path,
        session: &str,
    ) -> Option<AgentSession> {
        self.tick_read_session_file(&agent_dir(ai_workspace, session).join("session.json"))
    }
    fn tick_read_session_file(&mut self, path: &Path) -> Option<AgentSession> {
        if let Some(t) = self.last_read_attempt {
            if t.elapsed() < STREAM_READ_INTERVAL {
                return None;
            }
        }
        self.last_read_attempt = Some(Instant::now());

        let metadata = std::fs::metadata(path).ok()?;
        let mtime = metadata.modified().ok()?;
        if self.last_session_mtime == Some(mtime) {
            return None;
        }
        if metadata.len() > 16 * 1024 * 1024 {
            self.last_session_mtime = Some(mtime);
            return Some(AgentSession { approval:None, conversation: String::new(), artifacts: Vec::new(),request:String::new(),provider:String::new(),turns:vec![],updated_at:0,bundle:Default::default(),status:AgentStatus::Error("The sidecar session exceeds 16 MB. Archive its older turns in the source program.".into())});
        }
        let text = std::fs::read_to_string(path).ok()?;
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

pub fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
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
    fn streaming_snapshots_arrive_before_completion_without_fast_idle_polling() {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let session = AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: Vec::new(),
            status: AgentStatus::Thinking,
            provider: "test".into(),
            turns: vec![AgentTurn {
                role: "assistant".into(),
                text: "partial".into(),
                at: 1,
            }],
            updated_at: 1,
            bundle: Default::default(),
            request: "r1".into(),
        };
        let latest = std::sync::Arc::new(std::sync::Mutex::new(Some(session.clone())));
        let mut link = AgentLink {
            tx,
            latest: latest.clone(),
            next_read: Some(Instant::now()),
            next_write: None,
            streaming: false,
        };
        let path = Path::new("unused-session.json");
        assert_eq!(
            link.tick_read_session_file(path).unwrap().turns[0].text,
            "partial"
        );
        link.next_read = Some(Instant::now() - Duration::from_millis(150));
        link.tick_read_session_file(path);
        assert!(matches!(rx.try_recv(), Ok(LinkWork::Read(_))));
        *latest.lock().unwrap() = Some(AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: Vec::new(),
            status: AgentStatus::Idle,
            ..session
        });
        link.tick_read_session_file(path);
        link.next_read = Some(Instant::now() - Duration::from_millis(150));
        link.tick_read_session_file(path);
        assert!(
            rx.try_recv().is_err(),
            "idle sources must retain their slower poll cadence"
        );
    }

    #[test]
    fn context_write_is_fingerprint_gated() {
        let ws = temp_workspace("write");
        let mut link = FileAgentLink::new();
        let ctx = context();
        assert!(link.tick_write_context(&ws, "s1", &ctx));
        assert!(!link.tick_write_context(&ws, "s1", &ctx));
        link.force_elapsed();
        assert!(!link.tick_write_context(&ws, "s1", &ctx));
        assert!(agent_dir(&ws, "s1").join("README.md").exists());
        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn saved_link_directory_is_used_for_context_request_and_session() {
        let ws = temp_workspace("saved_link");
        let dir = ws.join("saved-project-link");
        let mut link = FileAgentLink::new();
        assert!(link.tick_write_context_in(&dir, &context()));
        let request = AgentRequest {
            model: None,
            history: vec![],
            id: "saved-run".into(),
            prompt: "Continue".into(),
            at: 1,
            inputs: Default::default(),
        };
        link.send_request_in(&dir, &request).unwrap();
        let state = AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: Vec::new(),
            request: request.id,
            status: AgentStatus::Idle,
            provider: "codex".into(),
            turns: vec![],
            updated_at: 2,
            bundle: Default::default(),
        };
        atomic_write_json(&dir.join("session.json"), &state).unwrap();
        assert_eq!(
            link.tick_read_session_file(&dir.join("session.json")),
            Some(state)
        );
        assert!(dir.join("request.json").is_file());
        assert!(dir.join("context.json").is_file());
        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn request_and_session_round_trip_on_mtime() {
        let ws = temp_workspace("session");
        let mut link = FileAgentLink::new();
        let req = AgentRequest {
            model: None,
            history: vec![],
            inputs: Default::default(),
            id: "r1".into(),
            prompt: "Summarize".into(),
            at: 1,
        };
        link.send_request(&ws, "s1", &req).unwrap();
        let text = std::fs::read_to_string(agent_dir(&ws, "s1").join("request.json")).unwrap();
        assert!(text.contains("Summarize"));

        let session = AgentSession {
            approval: None,
            conversation: String::new(),
            artifacts: Vec::new(),
            request: String::new(),
            bundle: Default::default(),
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

/// Friendly display names never change the provider model ID sent on the wire.
pub fn model_label(name: &str) -> &str {
    for (suffix, label) in [
        ("astra", "Astra"),
        ("sol", "Sol"),
        ("terra", "Terra"),
        ("luna", "Luna"),
    ] {
        if name.to_ascii_lowercase().split(['-', ' ']).last() == Some(suffix) {
            return label;
        }
    }
    name
}

#[cfg(test)]
mod model_label_tests {
    #[test]
    fn friendly_names_preserve_unknown_model_identity() {
        for (id, expected) in [
            ("gpt-6-astra", "Astra"),
            ("GPT-5.6 Sol", "Sol"),
            ("gpt-5.6-terra", "Terra"),
            ("gpt-5.6-luna", "Luna"),
            ("llama3.1:8b", "llama3.1:8b"),
        ] {
            assert_eq!(super::model_label(id), expected);
        }
    }
}
