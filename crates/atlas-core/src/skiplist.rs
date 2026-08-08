//! The folder names a walk refuses to enter.
//!
//! Two kinds of skip, deliberately kept apart. Our own thumbnail cache is a
//! correctness invariant — indexing it would index the app's own output — so it
//! is not a preference. Everything else is a judgement about what counts as
//! *content*, and that judgement is the user's.
//!
//! The judgement matters more than it looks. A vendored asset library (a
//! Megascans `Downloaded` tree, say) carries a `Thumbs` folder per surface and a
//! full plugin source tree, which is thousands of directories holding a handful
//! of files each. On a share a single directory read costs a second or two, so
//! eight workers clear about four directories a second: the walk delivers the
//! real content in seconds and then spends minutes on scaffolding nobody asked
//! to see. The defaults below are seeds in that spirit, not policy — every one
//! of them is editable.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Never walked, whatever the preferences say.
const ALWAYS: [&str; 1] = [crate::thumbs::CACHE_DIR_NAME];

/// What a first run starts with: machinery rather than content, in the same
/// spirit as `node_modules`.
pub fn default_names() -> Vec<String> {
    [
        "$RECYCLE.BIN",
        "System Volume Information",
        ".git",
        ".svn",
        "node_modules",
        "__pycache__",
        ".venv",
        ".cache",
        ".vs",
        ".idea",
        "Thumbs",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SkipList {
    pub names: Vec<String>,
}

impl Default for SkipList {
    fn default() -> Self {
        Self {
            names: default_names(),
        }
    }
}

impl SkipList {
    fn path() -> PathBuf {
        crate::index::data_dir().join("scan-skip.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let path = Self::path();
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    /// Whether a directory of this name should be walked into. Case-insensitive
    /// because the reference platform's filesystem is.
    pub fn skips(&self, name: &str) -> bool {
        ALWAYS.iter().any(|s| name.eq_ignore_ascii_case(s))
            || self.names.iter().any(|s| name.eq_ignore_ascii_case(s))
    }
}

/// Trim, drop blanks, and collapse case-insensitive duplicates while keeping the
/// order the user typed.
fn tidy(names: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(names.len());
    for name in names {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if out.iter().any(|k| k.eq_ignore_ascii_case(name)) {
            continue;
        }
        out.push(name.to_string());
    }
    out
}

static CURRENT: RwLock<Option<Arc<SkipList>>> = RwLock::new(None);

/// The list in force, cheap enough to hold for the length of a walk. Take one
/// snapshot per scan rather than locking per directory entry.
pub fn effective() -> Arc<SkipList> {
    if let Some(list) = CURRENT.read().ok().and_then(|g| g.clone()) {
        return list;
    }
    let list = Arc::new(SkipList::load());
    if let Ok(mut g) = CURRENT.write() {
        *g = Some(list.clone());
    }
    list
}

/// Replace the list and persist it. Walks already running keep the snapshot they
/// started with; the next scan picks this up.
pub fn set(names: Vec<String>) {
    let list = SkipList { names: tidy(names) };
    list.save();
    if let Ok(mut g) = CURRENT.write() {
        *g = Some(Arc::new(list));
    }
}

/// One-off check for callers that are not in a hot loop.
pub fn skips(name: &str) -> bool {
    effective().skips(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dir_is_skipped_even_if_the_list_is_emptied() {
        let list = SkipList { names: Vec::new() };
        assert!(list.skips(crate::thumbs::CACHE_DIR_NAME));
    }

    #[test]
    fn matching_ignores_case() {
        let list = SkipList {
            names: vec!["Thumbs".into()],
        };
        assert!(list.skips("thumbs"));
        assert!(list.skips("THUMBS"));
        assert!(!list.skips("Thumbnails"));
    }

    #[test]
    fn defaults_survive_a_file_written_by_an_older_build() {
        let list: SkipList = serde_json::from_str("{}").unwrap();
        assert_eq!(list.names, default_names());
    }

    #[test]
    fn an_explicitly_empty_list_stays_empty() {
        // Distinct from a missing field: the user cleared it on purpose.
        let list: SkipList = serde_json::from_str(r#"{"names":[]}"#).unwrap();
        assert!(list.names.is_empty());
    }

    #[test]
    fn tidy_drops_blanks_and_case_duplicates_but_keeps_order() {
        let got = tidy(vec![
            "  Thumbs ".into(),
            "".into(),
            "thumbs".into(),
            ".git".into(),
        ]);
        assert_eq!(got, vec!["Thumbs".to_string(), ".git".to_string()]);
    }

    #[test]
    fn round_trips_through_json() {
        let list = SkipList {
            names: vec!["Thumbs".into(), "MegascansPlugin".into()],
        };
        let json = serde_json::to_string(&list).unwrap();
        assert_eq!(serde_json::from_str::<SkipList>(&json).unwrap(), list);
    }
}
