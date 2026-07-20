//! Recently opened folders / workbooks (MRU), shared by File Atlas and Slate.
//!
//! Persisted next to the index DB as `{app_key}-recents.json`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECENTS: usize = 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentEntry {
    pub path: PathBuf,
    /// Display title (folder / workbook stem).
    pub title: String,
    /// Unix seconds when last opened.
    pub opened_at: u64,
    /// Optional baked cover image under the home-covers cache.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecentList {
    pub entries: Vec<RecentEntry>,
}

impl RecentList {
    fn path(app_key: &str) -> PathBuf {
        atlas_core::index::data_dir().join(format!("{app_key}-recents.json"))
    }

    pub fn load(app_key: &str) -> Self {
        std::fs::read_to_string(Self::path(app_key))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
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

    /// Insert or bump `path` to the front. Returns the entry (with cover path
    /// preserved when the path was already known).
    pub fn record(&mut self, path: PathBuf, title: impl Into<String>) -> &RecentEntry {
        let title = title.into();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let canon = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let existing_cover = self
            .entries
            .iter()
            .find(|e| paths_equal(&e.path, &canon) || paths_equal(&e.path, &path))
            .and_then(|e| e.cover.clone());
        self.entries
            .retain(|e| !paths_equal(&e.path, &canon) && !paths_equal(&e.path, &path));
        self.entries.insert(
            0,
            RecentEntry {
                path: canon,
                title,
                opened_at: now,
                cover: existing_cover,
            },
        );
        if self.entries.len() > MAX_RECENTS {
            self.entries.truncate(MAX_RECENTS);
        }
        &self.entries[0]
    }

    pub fn set_cover(&mut self, path: &Path, cover: PathBuf) {
        if let Some(e) = self.entries.iter_mut().find(|e| paths_equal(&e.path, path)) {
            e.cover = Some(cover);
        }
    }

    pub fn remove_missing(&mut self) {
        self.entries.retain(|e| e.path.exists());
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(aa), Ok(bb)) => aa == bb,
        _ => false,
    }
}

/// Directory for baked Cover Flow cover PNGs.
pub fn covers_dir() -> PathBuf {
    let dir = atlas_core::index::data_dir().join("home-covers");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Stable cover filename for a filesystem path.
pub fn cover_cache_path(for_path: &Path) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    for_path.hash(&mut h);
    covers_dir().join(format!("{:016x}.png", h.finish()))
}
