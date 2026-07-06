//! Persisted Slate UI settings.
//!
//! Stored next to the shared index DB (`slate-settings.json`, same folder as
//! `ai-config.json`) so every Slate window and future session sees the same
//! preferences. Loading tolerates missing/partial files (serde defaults);
//! saving is atomic (temp file + rename), mirroring `atlas_ai::AiConfig`.

use atlas_core::preview::{MAX_PX_DEFAULT, MAX_PX_MAX, MAX_PX_MIN};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Bounds and default for the preview memory budget (MB of decoded RGBA).
pub const BUDGET_MB_MIN: u32 = 256;
pub const BUDGET_MB_MAX: u32 = 8192;
pub const BUDGET_MB_DEFAULT: u32 = 1024;

/// Lazy full-resolution canvas previews (Advanced → Canvas previews).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PreviewSettings {
    /// Master switch: off = thumbnails only, exactly the pre-preview canvas.
    pub enabled: bool,
    /// Longest-edge cap for full-resolution decodes (px).
    pub max_px: u32,
    /// LRU memory budget for decoded previews (MB).
    pub budget_mb: u32,
}

impl Default for PreviewSettings {
    fn default() -> Self {
        PreviewSettings {
            enabled: true,
            max_px: MAX_PX_DEFAULT,
            budget_mb: BUDGET_MB_DEFAULT,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlateSettings {
    pub preview: PreviewSettings,
}

fn settings_path() -> PathBuf {
    atlas_core::index::data_dir().join("slate-settings.json")
}

impl SlateSettings {
    pub fn load() -> SlateSettings {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .map(SlateSettings::clamped)
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let path = settings_path();
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Hand-edited or stale files snap back into the supported ranges.
    fn clamped(mut self) -> SlateSettings {
        self.preview.max_px = self.preview.max_px.clamp(MAX_PX_MIN, MAX_PX_MAX);
        self.preview.budget_mb = self.preview.budget_mb.clamp(BUDGET_MB_MIN, BUDGET_MB_MAX);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_fills_defaults() {
        let s: SlateSettings = serde_json::from_str(r#"{"preview":{"max_px":4096}}"#).unwrap();
        assert_eq!(s.preview.max_px, 4096);
        assert!(s.preview.enabled);
        assert_eq!(s.preview.budget_mb, BUDGET_MB_DEFAULT);

        let s: SlateSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, SlateSettings::default());
    }

    #[test]
    fn clamp_repairs_out_of_range_values() {
        let s = SlateSettings {
            preview: PreviewSettings {
                enabled: true,
                max_px: 999_999,
                budget_mb: 1,
            },
        }
        .clamped();
        assert_eq!(s.preview.max_px, MAX_PX_MAX);
        assert_eq!(s.preview.budget_mb, BUDGET_MB_MIN);
    }

    #[test]
    fn roundtrip_preserves_values() {
        let s = SlateSettings {
            preview: PreviewSettings {
                enabled: false,
                max_px: 1024,
                budget_mb: 512,
            },
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: SlateSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }
}
