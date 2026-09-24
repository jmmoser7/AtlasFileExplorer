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
    // Ollama and ComfyUI are reached from media (the Agent squircle), not as
    // programs; `provider_by_id` still resolves cards that already use them.
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
pub fn comfy_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let _ = atlas_comfy::Client::start(&AtomicBool::new(false))?;
    Ok(atlas_comfy::checkpoints()?
        .into_iter()
        .map(|name| atlas_agent::AgentModel {
            id: name.clone(),
            name,
        })
        .collect())
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
pub fn cursor_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let value = crate::sidecar::query_cursor_models()?;
    let models = value
        .as_array()
        .ok_or("Cursor model catalog was not a list")?;
    Ok(models
        .iter()
        .filter_map(|model| {
            let id = model["id"].as_str()?;
            Some(atlas_agent::AgentModel {
                id: id.into(),
                name: model["name"].as_str().unwrap_or(id).into(),
            })
        })
        .collect())
}
pub fn codex_projects() -> Vec<(String, PathBuf)> {
    codex_executable()
        .and_then(|exe| atlas_codex::Client::projects(&exe, &std::env::current_dir().ok()?).ok())
        .unwrap_or_default()
}

pub struct CodexLink {
    tx: Sender<AgentRequest>,
    cancel: Arc<AtomicBool>,
    preview: atlas_comfy::PreviewSink,
}

enum Engine {
    Codex(atlas_codex::Client),
    Ollama(atlas_ollama::Client),
    Comfy(atlas_comfy::Client),
    OpenAi(atlas_openai::Client),
}

/// Pictures stay on this machine; the synced link folder holds only the
/// manifest. One folder per generator session, per engine.
fn image_output_dir(provider: &str, link: &Path) -> (PathBuf, String) {
    let tag = link
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    (
        atlas_core::index::data_dir().join(provider).join(&tag),
        tag,
    )
}

/// Credential Manager slot for the person's OpenAI API key.
pub use atlas_openai::KEY_SLOT as OPENAI_KEY_SLOT;

/// GPT Image models the stored key may call, newest first. Worker-only: it
/// reads Credential Manager and asks the API.
pub fn openai_image_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).ok_or("Add an OpenAI API key.")?;
    Ok(atlas_openai::models(&key)?
        .into_iter()
        .map(|(id, name)| atlas_agent::AgentModel { id, name })
        .collect())
}

/// OpenAI text models the stored key may call, newest first. Worker-only.
pub fn openai_text_models() -> Result<Vec<atlas_agent::AgentModel>, String> {
    let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).ok_or("Add an OpenAI API key.")?;
    Ok(atlas_openai::text_models(&key)?
        .into_iter()
        .map(|(id, name)| atlas_agent::AgentModel { id, name })
        .collect())
}

/// Every text model a text agent offers, as (provider, model, label): local
/// models first (the default needs no account, Art. I.4), then ChatGPT
/// through the Codex sign-in, then OpenAI API models.
pub fn text_models(
    local: &[atlas_agent::AgentModel],
    codex: Option<&[atlas_agent::AgentModel]>,
    openai: bool,
    api: &[atlas_agent::AgentModel],
) -> Vec<(String, String, String)> {
    let mut models = vec![("ollama".to_string(), String::new(), "Local · Auto".to_string())];
    models.extend(local.iter().map(|m| {
        (
            "ollama".to_string(),
            m.id.clone(),
            format!("Local · {}", crate::agent::model_label(&m.name)),
        )
    }));
    if let Some(codex) = codex {
        models.push((
            "codex-text".into(),
            String::new(),
            "ChatGPT · your sign-in".into(),
        ));
        models.extend(codex.iter().map(|m| {
            (
                "codex-text".to_string(),
                m.id.clone(),
                format!("ChatGPT · {}", crate::agent::model_label(&m.name)),
            )
        }));
    }
    if openai && api.is_empty() {
        // The key's catalog has not arrived yet.
        models.push((
            "openai-text".into(),
            atlas_openai::DEFAULT_TEXT_MODEL.into(),
            format!("OpenAI · {}", atlas_openai::DEFAULT_TEXT_MODEL),
        ));
    } else if openai {
        models.extend(api.iter().map(|m| {
            (
                "openai-text".to_string(),
                m.id.clone(),
                format!("OpenAI · {}", m.name),
            )
        }));
    } else {
        models.push((
            "openai-text".into(),
            atlas_openai::DEFAULT_TEXT_MODEL.into(),
            "OpenAI · add API key".into(),
        ));
    }
    models
}

/// Every image model the image agent offers, as (provider, model, label). An
/// empty model is that provider's default. ComfyUI checkpoints and the GPT
/// Image models a key may call come from their catalogs; before the key's
/// catalog arrives the known GPT Image models stand in.
pub fn image_models(
    comfy: &[atlas_agent::AgentModel],
    codex: bool,
    openai: bool,
    gpt: &[atlas_agent::AgentModel],
) -> Vec<(String, String, String)> {
    let mut models = vec![("comfy".to_string(), String::new(), "ComfyUI · Auto".to_string())];
    models.extend(
        comfy
            .iter()
            .map(|m| ("comfy".to_string(), m.id.clone(), format!("ComfyUI · {}", m.name))),
    );
    if codex {
        models.push((
            "codex-image".into(),
            String::new(),
            "ChatGPT · your sign-in".into(),
        ));
    }
    let known: Vec<(String, String)> = if gpt.is_empty() {
        atlas_openai::MODELS
            .iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect()
    } else {
        gpt.iter().map(|m| (m.id.clone(), m.name.clone())).collect()
    };
    models.extend(known.into_iter().map(|(id, name)| {
        let label = if openai {
            format!("{name} · API key")
        } else {
            format!("{name} · add API key")
        };
        ("openai-image".to_string(), id, label)
    }));
    models
}
impl Engine {
    fn start(
        provider: &str,
        dir: &Path,
        cwd: &Path,
        cancel: &AtomicBool,
        preview: &atlas_comfy::PreviewSink,
    ) -> Result<Self, String> {
        if provider == "ollama" || provider.starts_with("ollama/") {
            return atlas_ollama::Client::start(provider.strip_prefix("ollama/"), cancel)
                .map(Self::Ollama);
        }
        let (images, tag) = image_output_dir(provider, dir);
        if provider == "comfy" {
            let mut client = atlas_comfy::Client::start(cancel)?;
            client.set_output_dir(images, &tag);
            client.set_preview_sink(preview.clone());
            return Ok(Self::Comfy(client));
        }
        if provider == "openai-image" || provider == "openai-text" {
            let key = atlas_core::secrets::load(atlas_openai::KEY_SLOT)
                .ok_or("Add an OpenAI API key to use OpenAI models.")?;
            return Ok(Self::OpenAi(atlas_openai::Client::new(key, images)));
        }
        let executable =
            codex_executable().ok_or("Install Codex or set CODEX_BIN, then send again.")?;
        let previous = std::fs::read_to_string(dir.join("codex-thread.txt")).ok();
        if provider == "codex-image" || provider == "codex-text" {
            // Codex runs in the output folder, away from the person's
            // projects; every run starts its own thread.
            std::fs::create_dir_all(&images).map_err(|e| e.to_string())?;
            let mut c = atlas_codex::Client::start(&executable, &images, None)?;
            if provider == "codex-image" {
                c.set_image_dir(images);
            }
            return Ok(Self::Codex(c));
        }
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
            Self::Comfy(c) => c.run(request, session, stop, changed),
            Self::OpenAi(c) => c.run(request, session, stop, changed),
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
        let preview = atlas_comfy::PreviewSink::default();
        let frames = preview.clone();
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
                if request.oneshot {
                    session.turns.clear();
                }
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
                        for image in input.images.iter().chain(&input.depth) {
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
                    } else if provider == "comfy" {
                        "comfy-request.json"
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
                        client = Some(Engine::start(&provider, &dir, &cwd, &stop, &frames)?);
                    }
                    if !request.history.is_empty() && !dir.join("checkpoint.json").exists() {
                        atomic_write_json(&dir.join("checkpoint.json"), &request.history)
                            .map_err(|e| e.to_string())?;
                    }
                    if let Some(Engine::Codex(c)) = client.as_mut() {
                        c.set_full_access(
                            crate::access::session_of(&dir)
                                .is_some_and(|s| crate::access::granted(&s)),
                        );
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
        Self {
            tx,
            cancel,
            preview,
        }
    }
    /// The newest intermediate frame of the running image request, if any.
    pub fn preview(&self) -> Option<atlas_agent::ImagePreview> {
        self.preview.lock().ok()?.clone()
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

#[cfg(test)]
mod live {
    use super::*;

    /// One short text reply on the person's OpenAI API key.
    #[test]
    #[ignore = "spends one short OpenAI text reply on the stored key"]
    fn live_openai_text_through_the_stored_key() {
        let models = openai_text_models().unwrap();
        eprintln!(
            "text models: {:?}",
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
        );
        let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).unwrap();
        let mut client = atlas_openai::Client::new(key, std::env::temp_dir());
        let request: AgentRequest = serde_json::from_value(serde_json::json!({
            "id": "live-text", "prompt": "Name three colors, comma separated.", "at": 0,
            "oneshot": true, "model": models.first().map(|m| m.id.clone()),
            "inputs": {"revision": "1", "context": [], "wired": []}
        }))
        .unwrap();
        let mut session: AgentSession =
            serde_json::from_str(r#"{"provider":"openai-text"}"#).unwrap();
        let started = std::time::Instant::now();
        client
            .run(&request, &mut session, &AtomicBool::new(false), |_| {})
            .unwrap();
        eprintln!(
            "{:?} in {:?}: {}",
            request.model,
            started.elapsed(),
            session.turns.last().unwrap().text
        );
    }

    /// Spends one picture on the person's OpenAI API key. The key is read from
    /// Credential Manager inside the adapter and never printed.
    #[test]
    #[ignore = "spends one GPT Image picture on the stored OpenAI API key"]
    fn live_gpt_image_through_the_stored_key() {
        let models = openai_image_models().unwrap();
        eprintln!(
            "models: {:?}",
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>()
        );
        let dir = std::env::temp_dir().join(format!("atlas-openai-live-{}", std::process::id()));
        let key = atlas_core::secrets::load(OPENAI_KEY_SLOT).unwrap();
        let mut client = atlas_openai::Client::new(key, dir.clone());
        let request: AgentRequest = serde_json::from_value(serde_json::json!({
            "id": "live-gpt", "prompt": "a small green cube on a plain white background", "at": 0,
            "image": {"count": 1, "aspect": "square"},
            "inputs": {"revision": "1", "context": [], "wired": []}
        }))
        .unwrap();
        let mut session: AgentSession =
            serde_json::from_str(r#"{"provider":"openai-image"}"#).unwrap();
        let started = std::time::Instant::now();
        let result = client.run(&request, &mut session, &AtomicBool::new(false), |_| {});
        eprintln!("{result:?} in {:?}", started.elapsed());
        result.unwrap();
        assert_eq!(session.bundle.images.len(), 1);
        eprintln!("{}", session.bundle.images[0].source);
    }
}
