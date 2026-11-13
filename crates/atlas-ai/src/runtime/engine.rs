//! Existing provider supervision, separated from the request channel.
use super::*;

pub(super) enum Engine {
    Codex(Box<atlas_codex::Client>),
    Subscription(Box<atlas_subscription::Client>),
    Ollama(atlas_ollama::Client),
    Comfy(atlas_comfy::Client),
    OpenAi(atlas_openai::Client),
}

impl Engine {
    pub(super) fn start_staged_files(request: &AgentRequest) -> Result<Self, String> {
        let dir = atlas_codex::staged_files_dir(request)?;
        let executable = codex_executable().ok_or("Install Codex before authoring a definition")?;
        // The app-server startup cwd is a writable Windows sandbox root as
        // well as the turn cwd. Start a separate process for each revision.
        let client = atlas_codex::Client::start(&executable, dir, None)?;
        Ok(Self::Codex(Box::new(client)))
    }

    pub(super) fn start(
        provider: &str,
        dir: &Path,
        cwd: &Path,
        cancel: &AtomicBool,
        preview: &atlas_comfy::PreviewSink,
        workbook: Option<&Path>,
        board_image_workspace: Option<&Path>,
    ) -> Result<Self, String> {
        let leaf = crate::packs::leaf(provider);
        if leaf == Some("atlas-subscription") {
            let provider =
                atlas_subscription::provider(provider).ok_or("Unknown subscription provider")?;
            use sha2::Digest;
            let identity = format!(
                "{:x}",
                sha2::Sha256::digest(dir.to_string_lossy().as_bytes())
            );
            let isolated = atlas_core::index::data_dir()
                .join("subscription-text")
                .join(identity);
            std::fs::create_dir_all(&isolated).map_err(|e| e.to_string())?;
            let mut client = atlas_subscription::Client::start(provider, dir, &isolated)?;
            client.set_cloud_guard(atlas_core::cloud::is_dehydrated);
            return Ok(Self::Subscription(Box::new(client)));
        }
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
        if leaf != Some("atlas-codex") {
            return Err("This provider does not expose a supervised generator transport.".into());
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
            return Ok(Self::Codex(Box::new(c)));
        }
        let mut c = atlas_codex::Client::start(&executable, cwd, previous.as_deref())?;
        c.set_approval_dir(dir);
        c.set_board_cloud_guard(atlas_core::cloud::is_dehydrated);
        if let Some(workspace) = board_image_workspace {
            c.set_board_image_workspace(workspace);
        }
        std::fs::write(dir.join("codex-thread.txt"), c.thread_id()).map_err(|e| e.to_string())?;
        Ok(Self::Codex(Box::new(c)))
    }
    pub(super) fn run(
        &mut self,
        request: &AgentRequest,
        session: &mut AgentSession,
        stop: &AtomicBool,
        steering: &std::sync::mpsc::Receiver<atlas_codex::SteerCommand>,
        tools: Option<&atlas_agent::helper::HelperLaunch>,
        changed: impl FnMut(&AgentSession),
    ) -> Result<(), String> {
        match self {
            Self::Codex(c) => c.run_with_steering(request, session, stop, Some(steering), changed),
            Self::Subscription(c) => c.run_with_helper(request, session, stop, tools, changed),
            Self::Ollama(c) => c.run(request, session, stop, changed),
            Self::Comfy(c) => c.run(request, session, stop, changed),
            Self::OpenAi(c) => c.run(request, session, stop, changed),
        }
    }
}
