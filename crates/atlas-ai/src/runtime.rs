//! Process supervision. Vendor protocol stays in the renderer-free leaf crate.
use crate::agent::{atomic_write_json, AgentRequest, AgentSession, AgentStatus};
use crossbeam_channel::{bounded, Sender};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Installed programs plus explicitly configured file-link adapters. Discovery is
/// performed off the UI thread; unavailable future engines never appear as ready.
pub fn discover_programs(workspace: Option<&Path>) -> Vec<crate::agent::AgentProvider> {
    let mut programs = Vec::new();
    if crate::launch::cursor_available() {
        programs.push(crate::agent::provider_by_id("cursor"));
    }
    if codex_executable().is_some() {
        programs.push(crate::agent::provider_by_id("codex"));
    }
    if atlas_ollama::executable().is_some() {
        programs.push(crate::agent::provider_by_id("ollama"));
    }
    if let Some(ws) = workspace {
        if let Ok(bytes) = std::fs::read(ws.join(".atlas-ai/programs.json")) {
            if let Ok(custom) = serde_json::from_slice::<Vec<crate::agent::AgentProvider>>(&bytes) {
                for mut p in custom {
                    if !p.id.is_empty()
                        && !p.id.starts_with("ollama/")
                        && !programs.iter().any(|v| v.id == p.id)
                    {
                        p.launch = crate::agent::LaunchKind::None;
                        programs.push(p);
                    }
                }
            }
        }
    }
    programs
}

pub fn codex_executable() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_BIN")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(path);
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in ["codex.exe", "codex"] {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("OpenAI/Codex/bin");
    let mut entries: Vec<_> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path().join("codex.exe"))
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    entries.pop()
}

pub fn linear_provider(provider: &str) -> bool {
    matches!(provider, "codex" | "cursor")
}
pub fn conversations(provider: &str, cwd: &Path) -> Result<Vec<atlas_agent::Conversation>, String> {
    match provider {
        "codex" => {
            atlas_codex::Client::catalog(&codex_executable().ok_or("Codex is not installed")?, cwd)
        }
        "cursor" => serde_json::from_value(crate::sidecar::query_cursor(cwd, None)?)
            .map_err(|e| e.to_string()),
        _ => Ok(vec![]),
    }
}
pub fn read_conversation(provider: &str, cwd: &Path, id: &str) -> Result<AgentSession, String> {
    match provider {
        "codex" => atlas_codex::Client::read(
            &codex_executable().ok_or("Codex is not installed")?,
            cwd,
            id,
        ),
        "cursor" => serde_json::from_value(crate::sidecar::query_cursor(cwd, Some(id))?)
            .map_err(|e| e.to_string()),
        _ => Err("This provider does not expose saved conversations".into()),
    }
}
pub fn local_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let _ = atlas_ollama::Client::start(None, &AtomicBool::new(false))?;
    Ok(atlas_ollama::models()?
        .into_iter()
        .filter(|m| !m.contains("cloud"))
        .map(|name| atlas_agent::AgentModel {
            id: name.clone(),
            name,
        })
        .collect())
}
pub fn codex_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    atlas_codex::Client::models(
        &codex_executable().ok_or("Codex is not installed")?,
        &std::env::current_dir().map_err(|e| e.to_string())?,
    )
}
pub fn codex_projects() -> Vec<(String, PathBuf)> {
    codex_executable()
        .and_then(|exe| atlas_codex::Client::projects(&exe, &std::env::current_dir().ok()?).ok())
        .unwrap_or_default()
}

pub struct CodexLink {
    tx: Sender<AgentRequest>,
    cancel: Arc<AtomicBool>,
}

enum Engine {
    Codex(atlas_codex::Client),
    Ollama(atlas_ollama::Client),
}
impl Engine {
    fn start(provider: &str, dir: &Path, cwd: &Path, cancel: &AtomicBool) -> Result<Self, String> {
        if provider == "ollama" || provider.starts_with("ollama/") {
            return atlas_ollama::Client::start(provider.strip_prefix("ollama/"), cancel)
                .map(Self::Ollama);
        }
        let executable =
            codex_executable().ok_or("Install Codex or set CODEX_BIN, then send again.")?;
        let previous = std::fs::read_to_string(dir.join("codex-thread.txt")).ok();
        let mut c = atlas_codex::Client::start(&executable, cwd, previous.as_deref())?;
        c.set_approval_dir(dir);
        std::fs::write(dir.join("codex-thread.txt"), c.thread_id()).map_err(|e| e.to_string())?;
        Ok(Self::Codex(c))
    }
    fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        stop: &AtomicBool,
        changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        match self {
            Self::Codex(c) => c.run(request, session, stop, changed),
            Self::Ollama(c) => c.run(request, session, stop, changed),
        }
    }
}
impl Drop for CodexLink {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
impl CodexLink {
    pub fn start(dir: PathBuf, cwd: PathBuf) -> Self {
        Self::start_provider(dir, cwd, "codex".into())
    }
    pub fn start_provider(dir: PathBuf, cwd: PathBuf, provider: String) -> Self {
        let (tx, rx) = bounded::<AgentRequest>(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let mut client: Option<Engine> = None;
            let mut session = std::fs::read(dir.join("session.json"))
                .ok()
                .and_then(|v| serde_json::from_slice::<AgentSession>(&v).ok())
                .unwrap_or(AgentSession {
                    approval: None,
                    conversation: String::new(),
                    artifacts: Vec::new(),
                    request: String::new(),
                    status: AgentStatus::Idle,
                    provider: provider.clone(),
                    turns: vec![],
                    updated_at: 0,
                    bundle: Default::default(),
                });
            while let Ok(request) = rx.recv() {
                session.request = request.id.clone();
                if session.turns.is_empty() {
                    session.turns = request.history.clone();
                }

                let result = (|| -> Result<(), String> {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    session.status = AgentStatus::Thinking;
                    session.updated_at = crate::context::now_secs();
                    atomic_write_json(&dir.join("session.json"), &session)
                        .map_err(|e| e.to_string())?;
                    for input in request.inputs.context.iter().chain(&request.inputs.wired) {
                        for image in &input.images {
                            let path = Path::new(image);
                            if atlas_core::cloud::is_dehydrated(path) {
                                return Err("Make the connected image available locally before sending it to an agent.".into());
                            }
                            if !std::fs::metadata(path)
                                .map(|m| m.is_file())
                                .unwrap_or(false)
                            {
                                return Err(format!(
                                    "Connected image is unavailable: {}",
                                    path.display()
                                ));
                            }
                        }
                    }
                    let ledger = dir.join(if provider == "codex" {
                        "codex-request.json"
                    } else {
                        "local-request.json"
                    });
                    if std::fs::read_to_string(&ledger).ok().as_deref() == Some(&request.id) {
                        return Err(
                            "This request was already submitted. Send a new request to retry."
                                .into(),
                        );
                    }
                    if client.is_none() {
                        client = Some(Engine::start(&provider, &dir, &cwd, &stop)?);
                    }
                    if !request.history.is_empty() && !dir.join("checkpoint.json").exists() {
                        atomic_write_json(&dir.join("checkpoint.json"), &request.history)
                            .map_err(|e| e.to_string())?;
                    }
                    // Persist before submission: a crash never silently replays a paid turn.
                    std::fs::write(&ledger, &request.id).map_err(|e| e.to_string())?;
                    client
                        .as_mut()
                        .unwrap()
                        .run(&request, &mut session, &stop, |state| {
                            let _ = atomic_write_json(&dir.join("session.json"), state);
                        })
                })();
                if let Err(error) = result {
                    session.approval = None;
                    session.status = AgentStatus::Error(error);
                    session.updated_at = crate::context::now_secs();
                    client = None;
                    let _ = atomic_write_json(&dir.join("session.json"), &session);
                }
            }
        });
        Self { tx, cancel }
    }
    pub fn send(&self, request: AgentRequest) -> Result<(), String> {
        self.cancel.store(false, Ordering::Relaxed);
        self.tx
            .try_send(request)
            .map_err(|_| "The agent is busy. Wait or stop the current response.".into())
    }
    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub fn launch_codex(workspace: &Path) -> Result<(), String> {
    // A desktop executable is machine configuration, never a guessed URI scheme.
    let exe=std::env::var_os("CODEX_DESKTOP").map(PathBuf::from)
        .filter(|p|p.is_file()).ok_or("Open Codex from Windows. Set CODEX_DESKTOP to its desktop executable to enable this shortcut.")?;
    let mut command = std::process::Command::new(exe);
    command.current_dir(workspace);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.spawn().map(|_| ()).map_err(|e| e.to_string())
}
