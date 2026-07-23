//! Lightweight persisted chrome preferences shared by Atlas ecosystem apps.
//!
//! Stored next to the index DB as `{app_key}-chrome.json` so each binary keeps
//! its own dock placement default while still using the same schema.

use crate::dock::DockSide;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChromePrefs {
    pub dock_side: DockSide,
    /// Dock panels pinned as persistent tool palettes (e.g. Tags), restored
    /// on launch via `floating_dock`'s `restore_pins`.
    pub pinned_panels: Vec<String>,
    /// Canvas minimap pinned open (toggled by `M`; shared overlay chrome).
    pub minimap: bool,
}

impl ChromePrefs {
    pub fn default_for(side: DockSide) -> Self {
        Self {
            dock_side: side,
            pinned_panels: Vec::new(),
            minimap: false,
        }
    }

    fn path(app_key: &str) -> PathBuf {
        atlas_core::index::data_dir().join(format!("{app_key}-chrome.json"))
    }

    pub fn load(app_key: &str, fallback: DockSide) -> Self {
        std::fs::read_to_string(Self::path(app_key))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Self::default_for(fallback))
    }

    pub fn save(&self, app_key: &str) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let path = Self::path(app_key);
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
}

impl Default for ChromePrefs {
    fn default() -> Self {
        Self {
            dock_side: DockSide::LeftCenter,
            pinned_panels: Vec::new(),
            minimap: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_prefs_without_pins_still_load() {
        let prefs: ChromePrefs = serde_json::from_str(r#"{"dock_side":"bottom_center"}"#).unwrap();
        assert_eq!(prefs.dock_side, DockSide::BottomCenter);
        assert!(prefs.pinned_panels.is_empty());
    }

    #[test]
    fn pins_round_trip() {
        let prefs = ChromePrefs {
            dock_side: DockSide::LeftCenter,
            pinned_panels: vec!["tags".into(), "tool.curve".into()],
            minimap: true,
        };
        let json = serde_json::to_string(&prefs).unwrap();
        assert_eq!(serde_json::from_str::<ChromePrefs>(&json).unwrap(), prefs);
    }
}
