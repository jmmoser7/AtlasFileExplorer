//! Agent provider identities and their optional application launchers.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{local_image_engine, PortalView};

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
        "cursor-cli" => ("Cursor", LaunchKind::None),
        "claude-code" => ("Claude Code", LaunchKind::None),
        "codex" => ("Codex", LaunchKind::Codex),
        "ollama" => ("Ollama", LaunchKind::None),
        "local" => ("Local agent", LaunchKind::None),
        "image-link" => ("Image link", LaunchKind::None),
        "comfy" => ("ComfyUI", LaunchKind::None),
        "codex-image" => ("ChatGPT", LaunchKind::None),
        "openai-image" => ("OpenAI API", LaunchKind::None),
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
