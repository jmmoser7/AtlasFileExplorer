//! File Atlas edit-mode preferences.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct EditPrefs {
    #[serde(default)]
    pub suppress_delete_confirm: bool,
}

impl EditPrefs {
    fn path() -> std::path::PathBuf {
        atlas_core::index::data_dir().join("file-atlas-edit-prefs.json")
    }

    pub(crate) fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), json);
        }
    }
}
