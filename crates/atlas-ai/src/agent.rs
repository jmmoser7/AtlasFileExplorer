//! File-based live link for local agent portals.
//!
//! This module is deliberately vendor-free and scene-model-free. Slate writes
//! context and prompt requests here; any local sidecar can read them and write
//! session state back. Cursor is only one provider resolved by name.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::LINK_DIR;
mod file_link;
mod worker;
use file_link::FileAgentLink;

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

/// Prepared delivery through the source's existing worker. Process startup may
/// retain this handle until an old watcher has relinquished its authority.
#[derive(Clone)]
pub struct AgentRequestSender {
    tx: crossbeam_channel::Sender<LinkWork>,
    dir: PathBuf,
    request: AgentRequest,
}

impl AgentRequestSender {
    pub fn send(&self) -> std::io::Result<()> {
        self.tx
            .try_send(LinkWork::Request(self.dir.clone(), self.request.clone()))
            .map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::WouldBlock,
                    "Agent link is busy. Try again.",
                )
            })
    }
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
    /// The text of a `place.json` the link's worker found. It is offered
    /// again on later reads until the file is removed.
    pub fn take_place(&mut self, dir: &Path) -> Option<String> {
        self.links.get_mut(dir)?.take_place()
    }
    pub fn send(&mut self, dir: &Path, request: &AgentRequest) -> std::io::Result<()> {
        self.prepare_request(dir, request).send()
    }
    pub fn prepare_request(&mut self, dir: &Path, request: &AgentRequest) -> AgentRequestSender {
        self.links
            .entry(dir.into())
            .or_default()
            .prepare_request_in(dir, request)
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
    place: std::sync::Arc<std::sync::Mutex<Option<String>>>,
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
        let place = std::sync::Arc::new(std::sync::Mutex::new(None));
        let place_out = place.clone();
        std::thread::spawn(move || worker::run(rx, result, outputs_out, place_out));
        Self {
            tx,
            latest,
            next_read: None,
            next_write: None,
            streaming: false,
            awaited: None,
            outputs,
            place,
            roots: None,
        }
    }
    pub fn take_place(&mut self) -> Option<String> {
        self.place.try_lock().ok()?.take()
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
        self.prepare_request_in(dir, req).send()
    }
    fn prepare_request_in(&mut self, dir: &Path, req: &AgentRequest) -> AgentRequestSender {
        self.streaming = true;
        self.awaited = Some(req.id.clone());
        self.next_read = None;
        AgentRequestSender {
            tx: self.tx.clone(),
            dir: dir.into(),
            request: req.clone(),
        }
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

/// An agent's request to place something on the board, read on the link's
/// worker. A cloud placeholder is never read.
fn read_place_request(dir: &Path) -> Option<String> {
    let request = dir.join("place.json");
    if !request.is_file() || atlas_core::cloud::is_dehydrated(&request) {
        return None;
    }
    std::fs::read_to_string(request).ok()
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

/// Cache a background reload in the link folder by [`reloaded`]: the file is
/// rewritten only when the provider added messages, so its watchers see no
/// change otherwise. Returns what the conversation now shows. Worker only.
pub fn store_reloaded(dir: &Path, fetched: AgentSession) -> Result<AgentSession, String> {
    let path = dir.join("session.json");
    let shown = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AgentSession>(&bytes).ok());
    let next = match shown {
        Some(shown) => match reloaded(&shown, fetched) {
            Some(next) => next,
            None => return Ok(shown),
        },
        None => fetched,
    };
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    atomic_write_json(&path, &next).map_err(|e| e.to_string())?;
    Ok(next)
}

#[cfg(test)]
mod tests;

mod models;
pub use models::{local_image_engine, model_catalog, model_label};
