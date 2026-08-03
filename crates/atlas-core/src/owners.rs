//! Deferred file-owner resolution.
//!
//! Owner is the one field that cannot come out of a directory read: Windows
//! keeps it in the file's security descriptor, so learning it costs a query
//! *per file* — 0.08 ms on a local disk after SID caching, and a full
//! request/response on an SMB share. Resolving it inline made discovery wait on
//! tens of thousands of round trips before the canvas could draw.
//!
//! So the scanner leaves `owner` empty and this pass fills it in afterwards.
//! Nothing about layout, filtering, or thumbnails depends on it: owner drives a
//! filter facet and a zoomed-in readout, both of which are honest about being
//! incomplete while the pass runs. Revisits pay nothing, because the index
//! already stored what this pass learned last time.

use crossbeam_channel::Sender;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Latency-bound, not CPU-bound: on a share these threads spend their lives
/// waiting, so more of them than cores is the point.
const WORKERS: usize = 8;
const BATCH: usize = 256;

pub enum OwnerMsg {
    /// `(rel, owner)` for files whose owner resolved to something.
    Batch(Vec<(String, String)>),
    Done,
}

pub struct OwnerHandle {
    cancel: Arc<AtomicBool>,
}

impl OwnerHandle {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Drop for OwnerHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Resolve owners for `files`, streaming `(rel, owner)` batches tagged with
/// `generation` so a root change discards them.
///
/// Dropping the returned handle cancels the pass.
pub fn start_owner_pass(
    files: Vec<(String, PathBuf)>,
    generation: u64,
    tx: Sender<(u64, OwnerMsg)>,
) -> OwnerHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = OwnerHandle {
        cancel: cancel.clone(),
    };
    if files.is_empty() {
        let _ = tx.send((generation, OwnerMsg::Done));
        return handle;
    }

    let files = Arc::new(files);
    let next = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let workers = WORKERS.min(files.len());

    for _ in 0..workers {
        let files = files.clone();
        let next = next.clone();
        let finished = finished.clone();
        let cancel = cancel.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut batch: Vec<(String, String)> = Vec::with_capacity(BATCH);
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((rel, path)) = files.get(i) else {
                    break;
                };
                let owner = crate::metadata::owner_short(path);
                if !owner.is_empty() {
                    batch.push((rel.clone(), owner));
                }
                if batch.len() >= BATCH {
                    let _ = tx.send((generation, OwnerMsg::Batch(std::mem::take(&mut batch))));
                    batch.reserve(BATCH);
                }
            }
            if !batch.is_empty() {
                let _ = tx.send((generation, OwnerMsg::Batch(batch)));
            }
            if finished.fetch_add(1, Ordering::SeqCst) + 1 == workers
                && !cancel.load(Ordering::Relaxed)
            {
                let _ = tx.send((generation, OwnerMsg::Done));
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

    fn collect(rx: &crossbeam_channel::Receiver<(u64, OwnerMsg)>) -> Vec<(String, String)> {
        let mut got = Vec::new();
        while let (_, OwnerMsg::Batch(b)) = rx.recv_timeout(Duration::from_secs(20)).unwrap() {
            got.extend(b);
        }
        got
    }

    #[test]
    fn every_file_is_visited_exactly_once() {
        let dir = std::env::temp_dir().join(format!("atlas_owners_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let n = 300;
        let files: Vec<(String, PathBuf)> = (0..n)
            .map(|i| {
                let rel = format!("f{i}.txt");
                let p = dir.join(&rel);
                std::fs::write(&p, b"x").unwrap();
                (rel, p)
            })
            .collect();

        let (tx, rx) = unbounded();
        let _h = start_owner_pass(files, 11, tx);
        let got = collect(&rx);

        // On Windows every file has an owner; elsewhere the stub yields none,
        // so only the "no duplicates, terminates" half of this is portable.
        let mut rels: Vec<&str> = got.iter().map(|(r, _)| r.as_str()).collect();
        rels.sort_unstable();
        let unique = rels.len();
        rels.dedup();
        assert_eq!(rels.len(), unique, "a file was resolved twice");
        #[cfg(windows)]
        {
            assert_eq!(got.len(), n, "every file should resolve on Windows");
            assert!(got.iter().all(|(_, o)| !o.is_empty()));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_input_still_reports_done() {
        let (tx, rx) = unbounded();
        let _h = start_owner_pass(Vec::new(), 4, tx);
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            (4, OwnerMsg::Done)
        ));
    }

    #[test]
    fn missing_files_do_not_stall_the_pass() {
        let files = (0..20)
            .map(|i| (format!("gone{i}"), PathBuf::from(format!("Z:\\nope\\{i}"))))
            .collect();
        let (tx, rx) = unbounded();
        let _h = start_owner_pass(files, 1, tx);
        // The only requirement is that it terminates.
        let _ = collect(&rx);
    }
}
