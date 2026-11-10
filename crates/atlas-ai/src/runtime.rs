//! Process supervision. Vendor protocol stays in the renderer-free leaf crate.
mod artifact;
mod artifact_broker;
mod engine;
mod submission;
use engine::Engine;
mod image_history;
mod models;
use crate::agent::{atomic_write_json, AgentRequest, AgentSession, AgentStatus};
pub use artifact::{artifact_provider, validate_artifact_submission};
use crossbeam_channel::{bounded, Sender};
pub use models::{
    codex_executable, codex_models, codex_projects, comfy_models, cursor_models, image_models,
    local_models, openai_image_models, openai_text_models, text_models,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub fn linear_provider(provider: &str) -> bool {
    matches!(provider, "codex" | "cursor")
}

/// Explicit subscription routes; the historical Cursor SDK route is separate.
pub fn subscription_provider(provider: &str) -> Option<atlas_subscription::Provider> {
    (crate::packs::leaf(provider) == Some("atlas-subscription"))
        .then(|| atlas_subscription::provider(provider))
        .flatten()
}

pub fn subscription_models(provider: &str) -> Result<Vec<atlas_agent::AgentModel>, String> {
    atlas_subscription::models(subscription_provider(provider).ok_or("Unknown subscription route")?)
}
/// The existing Comfy Generate graph accepts ratio-driven latent dimensions.
/// Other adapters currently expose only preset sizes, not arbitrary ratios.
pub fn custom_image_aspect_supported(provider: &str, source_transform: bool) -> bool {
    provider == "comfy" && !source_transform
}
pub fn image_seed_supported(provider: &str) -> bool {
    provider == "comfy"
}
pub fn validate_image_parameters(provider: &str, request: &AgentRequest) -> Result<(), String> {
    let Some(image) = &request.image else {
        return Ok(());
    };
    image.aspect.validate()?;
    if image.seed.is_some() && !image_seed_supported(provider) {
        return Err(
            "This image provider does not expose a repeatable seed; unlock Seed before sending"
                .into(),
        );
    }
    let source_transform = request
        .inputs
        .on(atlas_agent::InputSlot::Media)
        .any(|item| item.depth.is_some() || item.images.iter().any(|image| !image.is_empty()));
    if image.aspect.custom().is_some() && !custom_image_aspect_supported(provider, source_transform)
    {
        return Err("Custom aspect requires Comfy generation without a source image or model; choose a supported preset".into());
    }
    if provider == "comfy" {
        image.aspect.validate_comfy(source_transform)?;
    }
    Ok(())
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
pub struct CodexLink {
    tx: Sender<(
        AgentRequest,
        Option<crate::access::Snapshot>,
        Option<atlas_agent::helper::HelperLaunch>,
    )>,
    steer_tx: std::sync::mpsc::SyncSender<atlas_codex::SteerCommand>,
    cancel: Arc<AtomicBool>,
    cancel_reason: Arc<std::sync::Mutex<Option<String>>>,
    current_request: Arc<std::sync::Mutex<String>>,
    preview: atlas_comfy::PreviewSink,
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
    for image in session
        .bundle
        .images
        .iter()
        .filter(|image| image.request == request.id)
    {
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
        Self::start_beside(dir, cwd, provider, None, None)
    }
    /// `workbook` is the `.slate` path when it has been saved. Generators then
    /// write under `assets/generated`. `None` keeps the data-directory folder.
    /// `board_image_workspace` is the host's established AI workspace, not the
    /// agent's project cwd. Without it, board image transport stays unavailable.
    pub fn start_beside(
        dir: PathBuf,
        cwd: PathBuf,
        provider: String,
        workbook: Option<PathBuf>,
        board_image_workspace: Option<PathBuf>,
    ) -> Self {
        let (tx, rx) = bounded::<(
            AgentRequest,
            Option<crate::access::Snapshot>,
            Option<atlas_agent::helper::HelperLaunch>,
        )>(1);
        let (steer_tx, steer_rx) = std::sync::mpsc::sync_channel::<atlas_codex::SteerCommand>(16);
        let cancel = Arc::new(AtomicBool::new(false));
        let stop = cancel.clone();
        let cancel_reason = Arc::new(std::sync::Mutex::new(None::<String>));
        let worker_reason = cancel_reason.clone();
        let current_request = Arc::new(std::sync::Mutex::new(String::new()));
        let worker_request = current_request.clone();
        let preview = atlas_comfy::PreviewSink::default();
        let frames = preview.clone();
        std::thread::spawn(move || {
            let mut client: Option<Engine> = None;
            let mut session = std::fs::read(dir.join("session.json"))
                .ok()
                .and_then(|v| serde_json::from_slice::<AgentSession>(&v).ok())
                .unwrap_or(AgentSession {
                    activity: Vec::new(),
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
            while let Ok((mut request, access_snapshot, host_tools)) = rx.recv() {
                image_history::prepare_request(&mut request);
                session.request = request.id.clone();
                session.activity.clear();
                let retained_image_turns = image_history::begin(&request, &mut session);

                let result = (|| -> Result<(), String> {
                    validate_artifact_submission(&request, &provider)?;
                    if host_tools.is_some() && subscription_provider(&provider).is_none() {
                        return Err("This helper route requires its subscription transport".into());
                    }
                    validate_image_parameters(&provider, &request)?;
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    session.status = AgentStatus::Thinking;
                    session.updated_at = crate::context::now_secs();
                    image_history::write_public(
                        &dir.join("session.json"),
                        &session,
                        retained_image_turns.as_ref(),
                    )
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
                    let _artifact_broker = match host_tools.as_ref() {
                        Some(atlas_agent::helper::HelperLaunch::Artifact(launch)) => {
                            Some(artifact_broker::ArtifactBroker::start(
                                launch,
                                &request,
                                &dir,
                                stop.clone(),
                                worker_request.clone(),
                            )?)
                        }
                        _ => None,
                    };
                    if client.is_none() {
                        client = Some(Engine::start(
                            &provider,
                            &dir,
                            &cwd,
                            &stop,
                            &frames,
                            workbook.as_deref(),
                            board_image_workspace.as_deref(),
                        )?);
                    }
                    if !request.history.is_empty() && !dir.join("checkpoint.json").exists() {
                        atomic_write_json(&dir.join("checkpoint.json"), &request.history)
                            .map_err(|e| e.to_string())?;
                    }
                    if let Some(Engine::Codex(c)) = client.as_mut() {
                        let session = crate::access::session_of(&dir);
                        let session = session.as_deref();
                        c.set_full_access(request_full_access(
                            session,
                            &request,
                            access_snapshot.as_ref(),
                        ));
                    }
                    // Persist before submission: a crash never silently replays a paid turn.
                    std::fs::write(&ledger, &request.id).map_err(|e| e.to_string())?;
                    if image_history::record_submitted_user(
                        &request,
                        &mut session,
                        matches!(client.as_ref(), Some(Engine::Codex(_))),
                    ) {
                        image_history::write_public(
                            &dir.join("session.json"),
                            &session,
                            retained_image_turns.as_ref(),
                        )
                        .map_err(|e| e.to_string())?;
                    }
                    client.as_mut().unwrap().run(
                        &request,
                        &mut session,
                        &stop,
                        &steer_rx,
                        host_tools.as_ref(),
                        |state| {
                            let _ = image_history::write_public(
                                &dir.join("session.json"),
                                state,
                                retained_image_turns.as_ref(),
                            );
                        },
                    )?;
                    record_image_sidecars(&session, &request);
                    Ok(())
                })();
                let result = result.map_err(|error| {
                    if stop.load(Ordering::Relaxed) {
                        worker_reason
                            .lock()
                            .ok()
                            .and_then(|reason| reason.clone())
                            .unwrap_or(error)
                    } else {
                        error
                    }
                });
                if let Some(retained) = retained_image_turns {
                    session.turns = retained.turns;
                    session.conversation = retained.conversation;
                }
                if let Err(error) = result {
                    session.approval = None;
                    session.status = AgentStatus::Error(error);
                    session.updated_at = crate::context::now_secs();
                    client = None;
                    let _ = atomic_write_json(&dir.join("session.json"), &session);
                } else if request.image.is_some() && request.image_history.is_some() {
                    let _ = atomic_write_json(&dir.join("session.json"), &session);
                }
            }
        });
        Self {
            tx,
            steer_tx,
            cancel,
            cancel_reason,
            current_request,
            preview,
        }
    }
    /// The newest intermediate frame of the running image request, if any.
    pub fn preview(&self) -> Option<atlas_agent::ImagePreview> {
        self.preview.lock().ok()?.clone()
    }
    pub fn stop(&self) {
        if let Ok(mut current) = self.current_request.lock() {
            current.clear();
        }
        if let Ok(mut reason) = self.cancel_reason.lock() {
            *reason = None;
        }
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Host expiry/revocation is a failure cause, distinct from a human Stop.
    pub fn stop_with_reason(&self, reason: String) {
        if let Ok(mut current) = self.current_request.lock() {
            current.clear();
        }
        if let Ok(mut cause) = self.cancel_reason.lock() {
            *cause = Some(reason);
        }
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Queue text for the active Codex turn. Provider acceptance is reported
    /// later in `AgentSession.activity` as `codex-steer:<id>:accepted|error`.
    pub fn steer(&self, expected_request_id: &str, text: &str) -> Result<String, String> {
        if expected_request_id.is_empty() {
            return Err("A request id is required to steer Codex.".into());
        }
        if text.trim().is_empty() {
            return Err("Steering text is empty.".into());
        }
        if text.len() > 8 * 1024 {
            return Err("Steering text exceeds 8 KiB.".into());
        }
        let id = format!("codex-steer-{}", atlas_agent::request_id());
        let command = atlas_codex::SteerCommand {
            id: id.clone(),
            expected_request_id: expected_request_id.into(),
            text: text.into(),
        };
        self.steer_tx
            .try_send(command)
            .map_err(|_| "The Codex steering queue is full or closed.".to_string())?;
        Ok(id)
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
pub fn request_full_access(
    session: Option<&str>,
    request: &AgentRequest,
    observed: Option<&crate::access::Snapshot>,
) -> bool {
    let Some(session) = session else {
        return false;
    };
    let loaded;
    let snapshot = if let Some(observed) = observed {
        observed
    } else {
        loaded = crate::access::load_snapshot_in(&crate::access::store_path());
        &loaded
    };
    turn_full_access(
        snapshot.granted_for_request(session, request),
        snapshot.relay_granted(session),
        request,
    )
}

pub fn turn_full_access(granted: bool, relay_granted: bool, request: &AgentRequest) -> bool {
    request.policy.is_default() && granted && (request.relayed_from.is_none() || relay_granted)
}

#[cfg(test)]
mod chat_live;
#[cfg(test)]
mod live;
#[cfg(test)]
mod tests;
