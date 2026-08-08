//! Deferred *folder* owner labels.
//!
//! The same lesson as [`crate::owners`], one level up, learned twice. A folder's
//! date and owner were once resolved inside `Tree::build` — filesystem I/O in a
//! layout function that runs on the frame path and re-runs on every rebuild of a
//! streaming scan. On a slow share a single folder took seconds, so a rebuild
//! could hold the UI thread for minutes with the canvas already on screen and no
//! way to know why.
//!
//! Moving it off-thread fixed the freeze but not the cost: measured at 4.5–5.8
//! seconds per folder on a high-latency share, sweeping every folder in a tree
//! spends hours of network time on subtitles. So the two halves are now sourced
//! according to what they cost:
//!
//! - **Creation date** comes free from the scan itself
//!   ([`crate::scanner::ScanMsg::Dirs`]) — it is already in the directory listing
//!   that discovered the folder.
//! - **Owner** lives in the security descriptor and cannot be had for free, so
//!   this pass resolves it only for folders on screen at the zoom level that
//!   actually renders it.
//!
//! Nothing about layout, hit-testing, filtering, or export reads either field.
//! Values are keyed by the directory's `rel`, so the cache survives every later
//! rebuild.

use crossbeam_channel::Sender;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Fewer workers than [`crate::owners`] on purpose. This pass runs *while* a
/// scan is discovering files, and on a share both are spending network round
/// trips, not CPU. Eight of these threads measurably starved discovery — about
/// 110 ms per directory, held for the whole length of a scan — so the pass is
/// deliberately narrow and only ever asked about folders already on screen.
const WORKERS: usize = 4;
const BATCH: usize = 16;

/// What a folder card's subtitle can show beyond its own name and counts.
///
/// The two fields arrive from different places (see the module docs): `ctime`
/// from the scan, `owner` from this pass. Either may be absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirMeta {
    /// Folder creation time (unix seconds), 0 when unknown.
    pub ctime: i64,
    /// Short owner/creator label, empty when unknown.
    pub owner: String,
}

impl DirMeta {
    pub fn is_empty(&self) -> bool {
        self.ctime == 0 && self.owner.is_empty()
    }
}

/// Resolved folder metadata, keyed by directory `rel` (`""` for the root).
pub type DirMetaMap = HashMap<String, DirMeta>;

pub enum DirMetaMsg {
    /// `(rel, meta)` for directories that resolved to something.
    Batch(Vec<(String, DirMeta)>),
    Done,
}

pub struct DirMetaHandle {
    cancel: Arc<AtomicBool>,
}

impl DirMetaHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for DirMetaHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Resolve owners for `dirs`, streaming `(rel, meta)` batches tagged with
/// `generation` so a root change discards them.
///
/// Dropping the returned handle cancels the pass.
pub fn start_dir_meta_pass(
    dirs: Vec<(String, PathBuf)>,
    generation: u64,
    tx: Sender<(u64, DirMetaMsg)>,
) -> DirMetaHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = DirMetaHandle {
        cancel: cancel.clone(),
    };
    if dirs.is_empty() {
        let _ = tx.send((generation, DirMetaMsg::Done));
        return handle;
    }

    let dirs = Arc::new(dirs);
    let next = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let workers = WORKERS.min(dirs.len());

    for _ in 0..workers {
        let dirs = dirs.clone();
        let next = next.clone();
        let finished = finished.clone();
        let cancel = cancel.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut batch: Vec<(String, DirMeta)> = Vec::with_capacity(BATCH);
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((rel, path)) = dirs.get(i) else {
                    break;
                };
                let meta = DirMeta {
                    ctime: 0,
                    owner: crate::metadata::owner_short(path),
                };
                if !meta.is_empty() {
                    batch.push((rel.clone(), meta));
                }
                // Folders are far fewer than files and each one is a card the
                // user may already be looking at, so flush on a small batch.
                if batch.len() >= BATCH {
                    let _ = tx.send((generation, DirMetaMsg::Batch(std::mem::take(&mut batch))));
                    batch.reserve(BATCH);
                }
            }
            if !batch.is_empty() {
                let _ = tx.send((generation, DirMetaMsg::Batch(batch)));
            }
            if finished.fetch_add(1, Ordering::SeqCst) + 1 == workers
                && !cancel.load(Ordering::Relaxed)
            {
                let _ = tx.send((generation, DirMetaMsg::Done));
            }
        });
    }

    handle
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use std::time::Duration;

    fn collect(rx: &crossbeam_channel::Receiver<(u64, DirMetaMsg)>) -> Vec<(String, DirMeta)> {
        let mut got = Vec::new();
        while let (_, DirMetaMsg::Batch(b)) = rx.recv_timeout(Duration::from_secs(20)).unwrap() {
            got.extend(b);
        }
        got
    }

    #[test]
    fn every_directory_is_visited_exactly_once() {
        let root = std::env::temp_dir().join(format!("atlas_dirmeta_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let n = 40;
        let dirs: Vec<(String, PathBuf)> = (0..n)
            .map(|i| {
                let rel = format!("d{i}");
                let p = root.join(&rel);
                std::fs::create_dir_all(&p).unwrap();
                (rel, p)
            })
            .collect();

        let (tx, rx) = unbounded();
        let _h = start_dir_meta_pass(dirs, 5, tx);
        let got = collect(&rx);

        let mut rels: Vec<&str> = got.iter().map(|(r, _)| r.as_str()).collect();
        rels.sort_unstable();
        let seen = rels.len();
        rels.dedup();
        assert_eq!(rels.len(), seen, "a directory was resolved twice");
        // Owner resolves on Windows; the non-Windows stub yields none, so only
        // the "no duplicates, terminates" half of this is portable.
        #[cfg(windows)]
        {
            assert_eq!(got.len(), n, "every directory should resolve an owner");
            assert!(got.iter().all(|(_, m)| !m.owner.is_empty()));
        }
        // The date is the scan's job now, never this pass's.
        assert!(got.iter().all(|(_, m)| m.ctime == 0));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_input_still_reports_done() {
        let (tx, rx) = unbounded();
        let _h = start_dir_meta_pass(Vec::new(), 9, tx);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            (9, DirMetaMsg::Done)
        ));
    }

    #[test]
    fn missing_directories_do_not_stall_the_pass() {
        let dirs = (0..12)
            .map(|i| (format!("gone{i}"), PathBuf::from(format!("Z:\\nope\\{i}"))))
            .collect();
        let (tx, rx) = unbounded();
        let _h = start_dir_meta_pass(dirs, 1, tx);
        let _ = collect(&rx);
    }
}
