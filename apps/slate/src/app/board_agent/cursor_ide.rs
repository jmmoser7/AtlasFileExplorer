//! Cursor IDE pump and failure labels.

use super::*;

impl SlateApp {
    pub(super) fn pump_cursor_ide(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.agents.ide_rx {
            match rx.try_recv() {
                Ok(status) => {
                    self.agents.ide = status;
                    self.agents.ide_rx = None;
                    self.agents.ide_inflight = false;
                    self.agents.ide_next = Some(Instant::now() + Duration::from_secs(2));
                    ctx.request_repaint();
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {}
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.agents.ide_rx = None;
                    self.agents.ide_inflight = false;
                }
            }
        }
        if self.agents.ide_inflight {
            return;
        }
        if self.agents.ide_next.is_some_and(|t| Instant::now() < t) {
            return;
        }
        let (tx, rx) = unbounded();
        self.agents.ide_rx = Some(rx);
        self.agents.ide_inflight = true;
        std::thread::spawn(move || {
            let _ = tx.send(atlas_ai::launch::probe_cursor_ide());
        });
    }
}
pub(super) fn recover_label(action: &AgentRecover) -> &'static str {
    match action {
        AgentRecover::OpenUrl { label, .. } | AgentRecover::OpenPath { label, .. } => label,
        AgentRecover::PasteKey => "Paste key",
        AgentRecover::PickWorkspace => "Choose workspace",
    }
}

pub(super) fn classify_agent_failure(reason: String) -> (String, Vec<AgentRecover>) {
    let raw = reason.trim().to_string();
    let lower = raw.to_ascii_lowercase();
    let setup = atlas_ai::sidecar::setup_doc().map(|path| AgentRecover::OpenPath {
        label: "Setup steps",
        path,
    });
    if lower.starts_with("could not install the cursor sidecar packages") {
        // The failed download and the manual steps are the next step.
        (raw, setup.into_iter().collect())
    } else if lower.contains("codex") {
        (
            raw,
            vec![AgentRecover::OpenUrl {
                label: "Codex setup",
                url: "https://learn.chatgpt.com/docs/auth",
            }],
        )
    } else if lower.contains("api key") || lower.contains("cursor_api_key") {
        let mut actions = vec![
            AgentRecover::OpenUrl {
                label: "Get a key",
                url: atlas_ai::cursor_key::DASHBOARD_URL,
            },
            AgentRecover::PasteKey,
        ];
        if let Some(setup) = setup {
            actions.push(setup);
        }
        (
            "Cursor needs an API key to reach an agent. Get one, then paste it here — you do not have to set a system environment variable.".into(),
            actions,
        )
    } else if lower.contains("workspace") {
        (
            "Choose an AI workspace folder so this portal has a place to write the agent link."
                .into(),
            vec![AgentRecover::PickWorkspace],
        )
    } else if looks_like_missing_node(&lower) {
        let mut actions = vec![AgentRecover::OpenUrl {
            label: "Download Node.js",
            url: "https://nodejs.org/en/download",
        }];
        if let Some(setup) = setup {
            actions.push(setup);
        }
        let text = if raw.contains("Looked in:") {
            raw
        } else {
            "Slate could not see node.exe. It looks in Program Files and on PATH. \
Set ATLAS_NODE to your node.exe if it is installed somewhere else."
                .into()
        };
        (text, actions)
    } else {
        let mut actions = Vec::new();
        if let Some(setup) = setup {
            actions.push(setup);
        }
        (raw, actions)
    }
}

fn looks_like_missing_node(lower: &str) -> bool {
    if lower.contains("npm") {
        return false;
    }
    lower.contains("could not see node")
        || lower.contains("looked in:")
        || lower.contains("node.js was not found")
        || lower.contains("node was not found")
        || (lower.contains("atlas_node") && lower.contains("not"))
}

pub(super) fn await_new_reply(turns: &[AgentTurn], req_at: u64) -> bool {
    turns
        .iter()
        .any(|t| t.role == "assistant" && t.at >= req_at)
}

/// What the model menu calls a portal's unset model.
pub(super) fn model_default(agent: &slate_doc::scene::AgentPortalRef) -> &'static str {
    if agent.view == atlas_ai::agent::PortalView::Text {
        "Auto"
    } else {
        model_fallback(&agent.provider)
    }
}

pub(super) fn model_fallback(provider: &str) -> &'static str {
    if provider == "cursor" {
        "Auto"
    } else if provider.starts_with("ollama") {
        "Choose model"
    } else if atlas_ai::agent::local_image_engine(provider) {
        "Auto"
    } else {
        "Default"
    }
}

pub(super) fn agent_status_chip(
    provider: &str,
    ide: CursorIdeStatus,
    sidecar: Option<&AgentStatus>,
    awaiting: Option<&AgentAwait>,
    accent: Color32,
    sub: Color32,
    danger: Color32,
) -> (Color32, &'static str, bool) {
    match awaiting {
        Some(AgentAwait::Failed { .. }) => {
            return (danger, "Unreachable", false);
        }
        Some(AgentAwait::Responding { .. }) => {
            return (accent, "Responding", true);
        }
        Some(AgentAwait::Sent { .. } | AgentAwait::Thinking { .. }) => {
            return (accent, "Thinking", true);
        }
        None => {}
    }
    let (color, label) = agent_live_chip(provider, ide, sidecar, accent, sub, danger);
    let pulse = matches!(sidecar, Some(AgentStatus::Thinking));
    (color, label, pulse)
}

fn agent_live_chip(
    provider: &str,
    ide: CursorIdeStatus,
    sidecar: Option<&AgentStatus>,
    accent: Color32,
    sub: Color32,
    danger: Color32,
) -> (Color32, &'static str) {
    let _ = (provider, ide);
    match sidecar {
        Some(AgentStatus::Thinking) => (accent, "Working"),
        Some(AgentStatus::Idle) => (sub, "Ready"),
        Some(AgentStatus::Error(_)) => (danger, "Error"),
        _ => (sub.gamma_multiply(0.75), "Not connected"),
    }
}

pub(super) fn chat_key(path: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

/// The provider's own recent projects. Tests never ask a real provider.
pub(super) fn collect_agent_project_recents(provider: &str) -> Vec<RecentEntry> {
    if cfg!(test) {
        return Vec::new();
    }
    let projects = if provider == "codex" {
        atlas_ai::runtime::codex_projects()
    } else {
        atlas_ai::cursor_recents::discover()
            .into_iter()
            .map(|p| (String::new(), p))
            .collect()
    };
    projects
        .into_iter()
        .map(|(name, path)| {
            let mut entry = atlas_ai::projects::entry(path, 0);
            if !name.is_empty() {
                entry.title = name;
            }
            entry
        })
        .collect()
}

pub(super) fn fit_frame_height(width: f32, image_w: u32, image_h: u32) -> f32 {
    let aspect = image_h as f32 / image_w.max(1) as f32;
    (width * aspect).clamp(160.0, 1400.0)
}
