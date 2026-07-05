//! Filesystem watcher: keeps the in-memory entries and the SQLite index live
//! while the app is open, so re-entering a folder never needs a rescan.

use crossbeam_channel::{unbounded, Receiver};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;

pub enum FsChange {
    Upsert(PathBuf),
    Remove(PathBuf),
    /// Something happened we can't attribute precisely; caller may rescan.
    Rescan,
}

pub struct FsWatch {
    _watcher: RecommendedWatcher,
    pub rx: Receiver<FsChange>,
}

/// Writes into the shared project cache land inside the watched root; they
/// must be ignored or warming would trigger rescans in a feedback loop.
fn in_own_cache(p: &std::path::Path) -> bool {
    p.components().any(|c| {
        c.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(crate::thumbs::CACHE_DIR_NAME)
    })
}

pub fn watch(root: PathBuf) -> Option<FsWatch> {
    let (tx, rx) = unbounded::<FsChange>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        use notify::EventKind::*;
        match event.kind {
            Create(_) | Modify(_) => {
                for p in event.paths {
                    if in_own_cache(&p) {
                        continue;
                    }
                    let _ = tx.send(FsChange::Upsert(p));
                }
            }
            Remove(_) => {
                for p in event.paths {
                    if in_own_cache(&p) {
                        continue;
                    }
                    let _ = tx.send(FsChange::Remove(p));
                }
            }
            _ => {
                let _ = tx.send(FsChange::Rescan);
            }
        }
    })
    .ok()?;
    watcher.watch(&root, RecursiveMode::Recursive).ok()?;
    Some(FsWatch {
        _watcher: watcher,
        rx,
    })
}
