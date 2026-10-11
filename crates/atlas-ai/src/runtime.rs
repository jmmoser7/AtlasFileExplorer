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

/// Where a generator writes pictures. A saved workbook files them under
/// `assets/generated/<provider>/<yyyy-mm>` so they travel with the `.slate`.
/// An unsaved workbook keeps one folder per session under the Atlas data
/// directory until Save or Collect assets copies the ones placed on the board.
pub(crate) fn image_output_dir(
    workbook: Option<&Path>,
    provider: &str,
    link: &Path,
) -> (PathBuf, String) {
    let tag = link
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let month = atlas_core::workbook_assets::year_month(crate::context::now_secs());
    (
        atlas_core::workbook_assets::generated_output_dir(
            workbook,
            &atlas_core::index::data_dir(),
            provider,
            &tag,
            &month,
        ),
        tag,
    )
}

fn record_image_sidecars(session: &AgentSession, request: &AgentRequest) {
    let Some(params) = request.image.as_ref() else {
        return;
    };
    let created = crate::context::now_secs();
    for image in &session.bundle.images {
        let path = Path::new(&image.source);
        if image.source.is_empty() {
            continue;
        }
        let mut sidecar_params = std::collections::BTreeMap::new();
        if !image.task.is_empty() {
            sidecar_params.insert("task".into(), image.task.clone());
        }
        sidecar_params.insert("aspect".into(), params.aspect.label().into());
        if params.count > 1 {
            sidecar_params.insert("count".into(), params.count.to_string());
        }
        let sidecar = atlas_core::workbook_assets::GeneratedSidecar {
            prompt: image.prompt.clone(),
            model: image.model.clone(),
            seed: params.seed,
            params: sidecar_params,
            created,
            source_run: request.id.clone(),
        };
        let _ = atlas_core::workbook_assets::write_generated_sidecar(path, &sidecar);
    }
}

/// Credential Manager slot for the person's OpenAI API key.
pub use atlas_openai::KEY_SLOT as OPENAI_KEY_SLOT;
/// GPT Image models that stand in until the key's catalog arrives.
pub use atlas_openai::{DEFAULT_TEXT_MODEL as OPENAI_TEXT_MODEL, MODELS as OPENAI_IMAGE_MODELS};

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

impl Engine {
    fn start(
        provider: &str,
        dir: &Path,
        cwd: &Path,
        cancel: &AtomicBool,
        preview: &atlas_comfy::PreviewSink,
        workbook: Option<&Path>,
    ) -> Result<Self, String> {
        let leaf = crate::packs::leaf(provider);
        if leaf == Some("atlas-ollama") {
            return atlas_ollama::Client::start(provider.strip_prefix("ollama/"), cancel)
                .map(Self::Ollama);
        }
        let (images, tag) = image_output_dir(workbook, provider, dir);
        if leaf == Some("atlas-comfy") {
            let mut client = atlas_comfy::Client::start(cancel)?;
            client.set_output_dir(images, &tag);
            client.set_preview_sink(preview.clone());
            return Ok(Self::Comfy(client));
        }
        if leaf == Some("atlas-openai") {
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
        Self::start_beside(dir, cwd, provider, None)
    }
    /// `workbook` is the `.slate` path when it has been saved. Generators then
    /// write under `assets/generated`. `None` keeps the data-directory folder.
    pub fn start_beside(
        dir: PathBuf,
        cwd: PathBuf,
        provider: String,
        workbook: Option<PathBuf>,
    ) -> Self {
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
                    usage: None,
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
                        client = Some(Engine::start(
                            &provider,
                            &dir,
                            &cwd,
                            &stop,
                            &frames,
                            workbook.as_deref(),
                        )?);
                    }
                    if !request.history.is_empty() && !dir.join("checkpoint.json").exists() {
                        atomic_write_json(&dir.join("checkpoint.json"), &request.history)
                            .map_err(|e| e.to_string())?;
                    }
                    if let Some(Engine::Codex(c)) = client.as_mut() {
                        let session = crate::access::session_of(&dir);
                        let session = session.as_deref();
                        c.set_full_access(turn_full_access(
                            session.is_some_and(crate::access::granted),
                            session.is_some_and(crate::access::relay_granted),
                            &request,
                        ));
                    }
                    // Persist before submission: a crash never silently replays a paid turn.
                    std::fs::write(&ledger, &request.id).map_err(|e| e.to_string())?;
                    client
                        .as_mut()
                        .unwrap()
                        .run(&request, &mut session, &stop, |state| {
                            let _ = atomic_write_json(&dir.join("session.json"), state);
                        })?;
                    record_image_sidecars(&session, &request);
                    Ok(())
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

/// Whether one turn runs under the person's full-access grant. A message
/// another agent wrote also needs the relay grant; a read-only turn never has it.
pub fn turn_full_access(granted: bool, relay_granted: bool, request: &AgentRequest) -> bool {
    request.policy.is_default() && granted && (request.relayed_from.is_none() || relay_granted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_agent::TurnPolicy;

    #[test]
    fn a_saved_workbook_receives_generated_images_beside_it() {
        let workbook = Path::new("/boards/deck/Board.slate");
        let link = Path::new("/links/session-a");
        let (dir, _) = image_output_dir(Some(workbook), "openai-image", link);
        let text = dir.to_string_lossy().replace('\\', "/");
        assert!(
            text.contains("/boards/deck/assets/generated/openai-image/"),
            "{text}"
        );
        let (unsaved, tag) = image_output_dir(None, "openai-image", link);
        assert_eq!(tag, "session-a");
        assert!(unsaved.ends_with(Path::new("openai-image").join("session-a")));
    }

    #[test]
    fn relayed_turns_need_the_relay_grant_and_read_only_never_gets_full_access() {
        let own = AgentRequest::default();
        let relayed = AgentRequest {
            relayed_from: Some("Codex · reviewer".into()),
            ..Default::default()
        };
        let read_only = AgentRequest {
            policy: TurnPolicy::ReadOnly,
            ..Default::default()
        };
        assert!(turn_full_access(true, false, &own));
        assert!(!turn_full_access(false, true, &own));
        assert!(!turn_full_access(true, false, &relayed));
        assert!(turn_full_access(true, true, &relayed));
        assert!(!turn_full_access(false, true, &relayed));
        for (granted, relay) in [(true, true), (true, false), (false, false)] {
            assert!(!turn_full_access(granted, relay, &read_only));
            let relayed_read_only = AgentRequest {
                relayed_from: Some("Cursor · builder".into()),
                ..read_only.clone()
            };
            assert!(!turn_full_access(granted, relay, &relayed_read_only));
        }
    }
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
