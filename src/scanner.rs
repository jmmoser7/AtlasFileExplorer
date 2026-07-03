//! Parallel streaming directory walker.
//!
//! N worker threads share a work queue of directories. File entries are
//! streamed to the UI in batches so cards appear from the first frame;
//! nothing waits for the scan to finish.

use crate::types::FileEntry;
use crossbeam_channel::Sender;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const WORKERS: usize = 8;
const BATCH: usize = 512;

pub enum ScanMsg {
    Batch(Vec<FileEntry>),
    Done { files: u64, elapsed_ms: u64 },
}

/// Messages are tagged with a generation so results from an abandoned scan
/// (user opened another folder mid-scan) can be discarded on the UI side.
pub struct ScanHandle {
    pub cancel: Arc<AtomicBool>,
    pub files_found: Arc<AtomicU64>,
}

struct Queue {
    dirs: Mutex<(Vec<PathBuf>, usize)>, // (pending, active worker count)
    cv: Condvar,
}

pub const SKIP_DIRS: [&str; 5] = [
    "$RECYCLE.BIN",
    "System Volume Information",
    ".git",
    "node_modules",
    crate::thumbs::CACHE_DIR_NAME, // never index our own shared cache
];

pub fn mtime_of(md: &std::fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn start_scan(root: PathBuf, generation: u64, tx: Sender<(u64, ScanMsg)>) -> ScanHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let files_found = Arc::new(AtomicU64::new(0));

    let handle = ScanHandle {
        cancel: cancel.clone(),
        files_found: files_found.clone(),
    };

    let queue = Arc::new(Queue {
        dirs: Mutex::new((vec![root.clone()], 0)),
        cv: Condvar::new(),
    });

    let started = Instant::now();
    let done_count = Arc::new(AtomicU64::new(0));

    for _ in 0..WORKERS {
        let queue = queue.clone();
        let tx = tx.clone();
        let root = root.clone();
        let cancel = cancel.clone();
        let files_found = files_found.clone();
        let done_count = done_count.clone();

        std::thread::spawn(move || {
            let mut batch: Vec<FileEntry> = Vec::with_capacity(BATCH);
            let mut last_flush = Instant::now();

            loop {
                let dir = {
                    let mut g = queue.dirs.lock().unwrap();
                    loop {
                        if cancel.load(Ordering::Relaxed) {
                            g.1 = usize::MAX; // poison: wake everyone, all exit
                            queue.cv.notify_all();
                            drop(g);
                            return;
                        }
                        if let Some(d) = g.0.pop() {
                            g.1 = g.1.saturating_add(1);
                            break Some(d);
                        }
                        if g.1 == 0 || g.1 == usize::MAX {
                            queue.cv.notify_all();
                            break None;
                        }
                        g = queue.cv.wait(g).unwrap();
                    }
                };

                let Some(dir) = dir else { break };

                if let Ok(rd) = std::fs::read_dir(&dir) {
                    let mut subdirs: Vec<PathBuf> = Vec::new();
                    for entry in rd.flatten() {
                        let Ok(ft) = entry.file_type() else { continue };
                        if ft.is_symlink() {
                            continue;
                        }
                        if ft.is_dir() {
                            let name = entry.file_name();
                            let name = name.to_string_lossy();
                            if SKIP_DIRS.iter().any(|s| name.eq_ignore_ascii_case(s)) {
                                continue;
                            }
                            subdirs.push(entry.path());
                        } else if ft.is_file() {
                            // On Windows this metadata comes from the directory
                            // read itself (FindFirstFile data) — no extra syscall.
                            let Ok(md) = entry.metadata() else { continue };
                            let size = md.len();
                            let mtime = mtime_of(&md);
                            if let Some(fe) =
                                FileEntry::from_abs(&root, entry.path(), size, mtime)
                            {
                                files_found.fetch_add(1, Ordering::Relaxed);
                                batch.push(fe);
                            }
                        }
                    }
                    if !subdirs.is_empty() {
                        let mut g = queue.dirs.lock().unwrap();
                        g.0.extend(subdirs);
                        queue.cv.notify_all();
                    }
                }

                if batch.len() >= BATCH || last_flush.elapsed().as_millis() > 30 {
                    if !batch.is_empty() {
                        let _ = tx.send((generation, ScanMsg::Batch(std::mem::take(&mut batch))));
                    }
                    last_flush = Instant::now();
                }

                {
                    let mut g = queue.dirs.lock().unwrap();
                    if g.1 != usize::MAX {
                        g.1 -= 1;
                    }
                    if g.0.is_empty() && g.1 == 0 {
                        queue.cv.notify_all();
                    }
                }
            }

            if !batch.is_empty() {
                let _ = tx.send((generation, ScanMsg::Batch(batch)));
            }

            // Last worker out reports completion.
            if done_count.fetch_add(1, Ordering::SeqCst) + 1 == WORKERS as u64
                && !cancel.load(Ordering::Relaxed)
            {
                let _ = tx.send((
                    generation,
                    ScanMsg::Done {
                        files: files_found.load(Ordering::Relaxed),
                        elapsed_ms: started.elapsed().as_millis() as u64,
                    },
                ));
            }
        });
    }

    handle
}

/// Stat a single path (used by the filesystem watcher for incremental updates).
pub fn stat_file(root: &std::path::Path, path: &std::path::Path) -> Option<FileEntry> {
    let md = std::fs::metadata(path).ok()?;
    if !md.is_file() {
        return None;
    }
    FileEntry::from_abs(root, path.to_path_buf(), md.len(), mtime_of(&md))
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    #[test]
    fn scan_streams_all_files() {
        let root = std::env::temp_dir().join(format!("nfa_scan_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::create_dir_all(root.join("c")).unwrap();
        std::fs::write(root.join("one.txt"), b"1").unwrap();
        std::fs::write(root.join("a/two.jpg"), b"22").unwrap();
        std::fs::write(root.join("a/b/three.mp4"), b"333").unwrap();
        std::fs::write(root.join("c/four.3dm"), b"4444").unwrap();

        let (tx, rx) = unbounded();
        let _h = start_scan(root.clone(), 7, tx);

        let mut got: Vec<FileEntry> = Vec::new();
        let done_files;
        loop {
            let (generation, msg) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
            assert_eq!(generation, 7);
            match msg {
                ScanMsg::Batch(b) => got.extend(b),
                ScanMsg::Done { files, .. } => {
                    done_files = files;
                    break;
                }
            }
        }
        assert_eq!(got.len(), 4);
        assert_eq!(done_files, 4);
        let mut rels: Vec<&str> = got.iter().map(|e| e.rel.as_str()).collect();
        rels.sort();
        assert_eq!(rels, vec!["a\\b\\three.mp4", "a\\two.jpg", "c\\four.3dm", "one.txt"]);
        let mp4 = got.iter().find(|e| e.ext == "mp4").unwrap();
        assert_eq!(mp4.size, 3);
        assert_eq!(mp4.family, crate::types::Family::Video);
        let rhino = got.iter().find(|e| e.ext == "3dm").unwrap();
        assert_eq!(rhino.family, crate::types::Family::Cad);
        let _ = std::fs::remove_dir_all(&root);
    }
}
