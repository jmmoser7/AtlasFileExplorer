//! Shared AI configuration, persisted once for the whole ecosystem.
//!
//! Stored next to the index DB (`%LOCALAPPDATA%\NativeFileAtlas\ai-config.json`)
//! so File Atlas and every Slate instance see the same AI workspace.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Subfolder of the AI workspace owned by the apps (context beacons, docs).
pub const LINK_DIR: &str = ".atlas-ai";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AiConfig {
    /// The user-established AI workspace: Cursor's default working directory
    /// when launched from Atlas or Slate, and home of the live-link files.
    pub workspace_dir: Option<PathBuf>,
}

fn config_path() -> PathBuf {
    atlas_core::index::data_dir().join("ai-config.json")
}

impl AiConfig {
    pub fn load() -> AiConfig {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let path = config_path();
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// The workspace, but only when it still exists on disk.
    pub fn valid_workspace(&self) -> Option<&Path> {
        self.workspace_dir.as_deref().filter(|p| p.is_dir())
    }

    /// Establish (or move) the AI workspace: creates the folder, the
    /// `.atlas-ai` link directory with a README, and a `.cursor/mcp.json`
    /// scaffold when absent — this is the "live link to the Cursor
    /// repository" future MCP servers plug into.
    ///
    /// Does not persist; call [`AiConfig::save`] afterwards (kept separate so
    /// tests never touch the real per-user config file).
    pub fn set_workspace(&mut self, dir: PathBuf) -> std::io::Result<()> {
        std::fs::create_dir_all(&dir)?;
        let link = dir.join(LINK_DIR);
        std::fs::create_dir_all(&link)?;
        let readme = link.join("README.md");
        if !readme.exists() {
            let _ = std::fs::write(
                &readme,
                "# Atlas AI link\n\n\
                 This folder is maintained by File Atlas and Slate. Each app keeps a\n\
                 live context file here (`atlas-context.json`, `slate-context.json`)\n\
                 describing what is currently open and selected, so Cursor — and the\n\
                 MCP servers that will ship with the AI integration — can see the\n\
                 files being previewed and act on them (auto-tagging, classification,\n\
                 presentation generation).\n\n\
                 The files are overwritten continuously; don't edit them by hand.\n",
            );
        }
        // Scaffold Cursor's MCP config only when the user has none.
        let cursor_dir = dir.join(".cursor");
        let mcp = cursor_dir.join("mcp.json");
        if !mcp.exists() {
            let _ = std::fs::create_dir_all(&cursor_dir);
            let _ = std::fs::write(
                &mcp,
                "{\n  \"mcpServers\": {\n    \
                 \"//\": \"Atlas/Slate MCP servers land here in a future release.\"\n  }\n}\n",
            );
        }
        self.workspace_dir = Some(dir);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "atlas_ai_test_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn set_workspace_scaffolds_link_and_mcp() {
        let dir = temp_dir("scaffold");
        let mut cfg = AiConfig::default();
        cfg.set_workspace(dir.clone()).unwrap();
        assert!(dir.join(LINK_DIR).join("README.md").is_file());
        assert!(dir.join(".cursor").join("mcp.json").is_file());
        assert_eq!(cfg.valid_workspace(), Some(dir.as_path()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_mcp_config_is_not_clobbered() {
        let dir = temp_dir("noclobber");
        std::fs::create_dir_all(dir.join(".cursor")).unwrap();
        std::fs::write(dir.join(".cursor/mcp.json"), "{\"custom\":true}").unwrap();
        let mut cfg = AiConfig::default();
        cfg.set_workspace(dir.clone()).unwrap();
        let kept = std::fs::read_to_string(dir.join(".cursor/mcp.json")).unwrap();
        assert_eq!(kept, "{\"custom\":true}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_workspace_is_invalid() {
        let cfg = AiConfig {
            workspace_dir: Some(PathBuf::from("/definitely/not/here")),
        };
        assert_eq!(cfg.valid_workspace(), None);
    }
}
