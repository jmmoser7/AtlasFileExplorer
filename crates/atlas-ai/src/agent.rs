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
        "comfy" => ("ComfyUI", LaunchKind::None),
        "codex-image" => ("ChatGPT", LaunchKind::None),
        "openai-image" => ("GPT Image", LaunchKind::None),
        "codex-text" => ("ChatGPT", LaunchKind::None),
        "openai-text" => ("OpenAI", LaunchKind::None),
        _ => (id, LaunchKind::None),
    };
    AgentProvider {
        id: id.into(),
        display_name: name.into(),
        launch,
        view: if id == "image-link" || local_image_engine(id) {
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
    /// Folders `return.json` paths resolve against after the output folder.
    Roots(Vec<PathBuf>),
}

/// One worker and shared immutable snapshot per linked source, regardless of how
/// many cards project it. The caller's NodeIds never become runtime identities.
#[derive(Default)]
pub struct AgentSources {
    links: std::collections::HashMap<PathBuf, AgentLink>,
    snapshots: std::collections::HashMap<PathBuf, std::sync::Arc<AgentSession>>,
    outputs: std::collections::HashMap<PathBuf, std::sync::Arc<crate::outputs::LinkOutputs>>,
}
impl AgentSources {
    pub fn retain(&mut self, dirs: &std::collections::HashSet<PathBuf>) {
        self.links.retain(|dir, _| dirs.contains(dir));
        self.snapshots.retain(|dir, _| dirs.contains(dir));
        self.outputs.retain(|dir, _| dirs.contains(dir));
    }
    /// The working folder and AI workspace of this link, in that order. The
    /// worker consumes `return.json` only once it knows them.
    pub fn set_roots(&mut self, dir: &Path, roots: Vec<PathBuf>) {
        self.links.entry(dir.into()).or_default().set_roots(roots);
    }
    /// Deliverables and versions, as of the last [`Self::poll`].
    pub fn outputs(&self, dir: &Path) -> Option<std::sync::Arc<crate::outputs::LinkOutputs>> {
        self.outputs.get(dir).cloned()
    }
    pub fn send(&mut self, dir: &Path, request: &AgentRequest) -> std::io::Result<()> {
        self.links
            .entry(dir.into())
            .or_default()
            .send_request_in(dir, request)
    }
    /// A request went out through another channel; read its session promptly.
    pub fn expect(&mut self, dir: &Path, request: &str) {
        let link = self.links.entry(dir.into()).or_default();
        link.streaming = true;
        link.awaited = Some(request.to_string());
        link.next_read = None;
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
        if let Some(outputs) = link.take_outputs() {
            self.outputs
                .insert(dir.into(), std::sync::Arc::new(outputs));
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
    /// Request whose session has not been read yet. A stale snapshot of the
    /// previous run must not end fast reads.
    awaited: Option<String>,
    outputs: std::sync::Arc<std::sync::Mutex<Option<crate::outputs::LinkOutputs>>>,
    /// Roots the worker has accepted.
    roots: Option<Vec<PathBuf>>,
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
        let outputs = std::sync::Arc::new(std::sync::Mutex::new(None));
        let outputs_out = outputs.clone();
        std::thread::spawn(move || {
            let mut link = FileAgentLink::new();
            // Snapshot files only when completion or the artifact list can have
            // changed, not on every streamed token.
            let mut captured: Option<(PathBuf, bool, usize, usize)> = None;
            let mut roots: Option<Vec<PathBuf>> = None;
            let mut turns = crate::outputs::skeleton(None);
            let mut watch = crate::outputs::OutputWatch::default();
            while let Ok(work) = rx.recv() {
                let update = match work {
                    LinkWork::Roots(r) => {
                        roots = Some(r);
                        None
                    }
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
                    LinkWork::Read(path) => {
                        let session = link.tick_read_session_file(&path);
                        let mut copied = false;
                        if let (Some(s), Some(dir)) = (&session, path.parent()) {
                            let key = (
                                dir.to_path_buf(),
                                s.status == AgentStatus::Thinking,
                                s.turns.len(),
                                s.artifacts.len(),
                            );
                            if captured.as_ref() != Some(&key) {
                                if let Ok(changed) = crate::versions::capture(dir, s) {
                                    captured = Some(key);
                                    copied = changed;
                                }
                            }
                            if s.turns.len() != turns.turns.len()
                                || s.artifacts.len() != turns.artifacts.len()
                            {
                                turns = crate::outputs::skeleton(Some(s));
                            }
                        }
                        if let (Some(roots), Some(dir)) = (&roots, path.parent()) {
                            if let Some(found) = watch.tick(dir, &turns, roots, copied) {
                                if let Ok(mut value) = outputs_out.lock() {
                                    *value = Some(found);
                                }
                            }
                        }
                        session
                    }
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
            awaited: None,
            outputs,
            roots: None,
        }
    }
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        if self.roots.as_ref() == Some(&roots) {
            return;
        }
        if self.tx.try_send(LinkWork::Roots(roots.clone())).is_ok() {
            self.roots = Some(roots);
        }
    }
    /// A new deliverables and versions snapshot, when the worker made one.
    pub fn take_outputs(&mut self) -> Option<crate::outputs::LinkOutputs> {
        self.outputs.try_lock().ok()?.take()
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
        self.awaited = Some(req.id.clone());
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
            if self.awaited.as_deref() == Some(session.request.as_str())
                && session.status != AgentStatus::Thinking
            {
                self.awaited = None;
            }
            self.streaming =
                matches!(session.status, AgentStatus::Thinking) || self.awaited.is_some();
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

/// The folder a conversation's new files go to. The first call records it in
/// `<link_dir>/output.json`, so renaming the board or conversation later never
/// moves outputs. `base` is the conversation's working folder (its project, or
/// the AI workspace); `board` is the workbook file stem. Creates only the top
/// folder. Blocking I/O: call from a worker.
pub fn output_dir(
    link_dir: &Path,
    base: &Path,
    board: Option<&str>,
    title: &str,
    now_secs: u64,
) -> std::io::Result<PathBuf> {
    let record = link_dir.join("output.json");
    if let Some(dir) = recorded_output_dir(link_dir) {
        std::fs::create_dir_all(&dir)?;
        return Ok(dir);
    }
    let board = board
        .map(|b| slug(b, ""))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "untitled-board".into());
    let session = link_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (y, m, d) = civil_date(now_secs);
    let dir = base.join("slate-outputs").join(board).join(format!(
        "{y:04}-{m:02}-{d:02}-{}-{}",
        slug(title, "conversation"),
        short_id(&session)
    ));
    std::fs::create_dir_all(&dir)?;
    std::fs::create_dir_all(link_dir)?;
    atomic_write_json(&record, &OutputRecord { dir: dir.clone() })?;
    Ok(dir)
}

#[derive(Serialize, Deserialize)]
struct OutputRecord {
    dir: PathBuf,
}

/// The output folder [`output_dir`] recorded for this link, if any. Blocking I/O.
pub fn recorded_output_dir(link_dir: &Path) -> Option<PathBuf> {
    std::fs::read(link_dir.join("output.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<OutputRecord>(&b).ok())
        .map(|r| r.dir)
        .filter(|d| d.is_absolute())
}

/// Lowercase ASCII letters and digits joined by single `-`, at most 40 chars.
fn slug(text: &str, fallback: &str) -> String {
    let mut out = String::new();
    for word in text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
    {
        let sep = usize::from(!out.is_empty());
        if out.len() + sep + word.len() > 40 {
            if out.is_empty() {
                out.push_str(&word[..40]);
            }
            break;
        }
        if sep == 1 {
            out.push('-');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        fallback.into()
    } else {
        out.to_ascii_lowercase()
    }
}

/// Six hex digits naming the session folder. Slate's folder names share an
/// `agent-req-<pid>-` prefix, so a leading slice would not tell them apart.
fn short_id(session: &str) -> String {
    let hash = session.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{:06x}", hash & 0xff_ffff)
}

/// UTC calendar date (proleptic Gregorian) of a Unix time.
fn civil_date(secs: u64) -> (i64, u32, u32) {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
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
            awaited: None,
            outputs: Default::default(),
            roots: None,
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
            ..session.clone()
        });
        link.tick_read_session_file(path);
        link.next_read = Some(Instant::now() - Duration::from_millis(150));
        link.tick_read_session_file(path);
        assert!(
            rx.try_recv().is_err(),
            "idle sources must retain their slower poll cadence"
        );

        // The next run's first read may still see the last run's finished session.
        link.streaming = true;
        link.awaited = Some("r2".into());
        *latest.lock().unwrap() = Some(AgentSession {
            status: AgentStatus::Idle,
            ..session.clone()
        });
        link.tick_read_session_file(path);
        link.next_read = Some(Instant::now() - Duration::from_millis(150));
        link.tick_read_session_file(path);
        assert!(
            matches!(rx.try_recv(), Ok(LinkWork::Read(_))),
            "a stale session keeps fast reads until the awaited request reports"
        );
    }

    #[test]
    fn output_folder_is_named_once_and_recorded() {
        let ws = temp_workspace("output");
        let link = ws.join(".atlas-ai/agent/agent-req-1-2-3");
        // 2026-09-23T19:19:00Z
        let now = 1_790_191_140;
        let dir = output_dir(&link, &ws, Some("Q3 Review"), "Chart the café sales!", now).unwrap();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(dir.parent().unwrap(), ws.join("slate-outputs/q3-review"));
        assert!(
            name.starts_with("2026-09-23-chart-the-caf-sales-"),
            "{name}"
        );
        assert_eq!(name.len(), "2026-09-23-chart-the-caf-sales-".len() + 6);
        assert!(dir.is_dir());
        assert!(!dir.join("assets").exists());
        assert!(link.join("output.json").is_file());
        let renamed = output_dir(&link, &ws, Some("Other"), "Renamed", now + 86_400 * 9).unwrap();
        assert_eq!(renamed, dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(output_dir(&link, &ws, None, "", now).unwrap(), dir);
        assert!(dir.is_dir(), "a recorded folder is recreated");

        let other = ws.join(".atlas-ai/agent/agent-req-1-2-4");
        let fresh = output_dir(&other, &ws, None, "  ", now).unwrap();
        let fresh_name = fresh.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(
            fresh.parent().unwrap(),
            ws.join("slate-outputs/untitled-board")
        );
        assert!(fresh_name.starts_with("2026-09-23-conversation-"));
        assert_ne!(fresh_name, name);
        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn slugs_and_dates() {
        assert_eq!(slug("  Hello,  World -- 2 ", "x"), "hello-world-2");
        assert_eq!(slug("日本", "conversation"), "conversation");
        let long = slug(&"word ".repeat(20), "x");
        assert!(long.len() <= 40 && !long.ends_with('-'), "{long}");
        assert_eq!(slug(&"a".repeat(50), "x").len(), 40);
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(951_782_400), (2000, 2, 29));
        assert_eq!(civil_date(1_790_191_140), (2026, 9, 23));
        assert_eq!(civil_date(1_798_761_599), (2026, 12, 31));
        assert_eq!(short_id("agent-a").len(), 6);
        assert_ne!(short_id("agent-a"), short_id("agent-b"));
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
            image: None,
            output_dir: None,
            oneshot: false,
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
            image: None,
            output_dir: None,
            oneshot: false,
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
/// The model catalog a provider's portals choose from.
pub fn model_catalog(provider: &str) -> &str {
    if provider == "ollama" || provider.starts_with("ollama/") {
        "ollama"
    } else if provider == "codex-text" {
        // ChatGPT text through the sign-in offers the Codex model catalog.
        "codex"
    } else {
        provider
    }
}

/// An image engine Slate supervises in process: runs wait their turn on the
/// portal, 3D and video inputs are captured at run start, and Live reruns on
/// changes. ChatGPT (through Codex) and the OpenAI API run remotely but are
/// supervised the same way.
pub fn local_image_engine(provider: &str) -> bool {
    matches!(provider, "comfy" | "codex-image" | "openai-image")
}

pub fn model_label(name: &str) -> &str {
    // A checkpoint is named by its file; the extension carries no meaning.
    for ext in [".safetensors", ".ckpt"] {
        if name.len() > ext.len() && name.to_ascii_lowercase().ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
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
            ("DreamShaper8_LCM.safetensors", "DreamShaper8_LCM"),
            ("old.CKPT", "old"),
        ] {
            assert_eq!(super::model_label(id), expected);
        }
        assert_eq!(super::model_catalog("ollama/qwen3"), "ollama");
        assert_eq!(super::model_catalog("comfy"), "comfy");
        assert!(super::local_image_engine("comfy"));
        assert!(!super::local_image_engine("image-link"));
    }
}
