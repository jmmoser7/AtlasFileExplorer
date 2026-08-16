//! Parallel streaming directory walker.
//!
//! N worker threads share a work queue of directories. File entries are
//! streamed to the UI in batches so cards appear from the first frame;
//! nothing waits for the scan to finish.
//!
//! The queue is **breadth-first**, which is a decision about what the user sees
//! first rather than about total throughput. Taking directories from the back
//! made the walk dive down whichever branch it happened to open, so a share
//! whose deep corners are slow could spend minutes down one of them while whole
//! shallow levels sat undiscovered. Level by level, the shape of the folder
//! arrives first and pathological depth is charged last.

use crate::metadata::ctime_of;
use crate::types::FileEntry;
use crossbeam_channel::Sender;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const WORKERS: usize = 8;
const BATCH: usize = 512;

pub enum ScanMsg {
    Batch(Vec<FileEntry>),
    /// Folder creation times, `(rel, ctime)`, harvested from the directory
    /// listing that discovered them.
    ///
    /// Free: on Windows a `DirEntry`'s metadata comes from the `FindFirstFile`
    /// data the walk already read, so this costs no extra round trip. Asking for
    /// it separately did — about five seconds per folder on a slow share, for a
    /// date on a folder card.
    Dirs(Vec<(String, i64)>),
    Done {
        files: u64,
        elapsed_ms: u64,
    },
}

/// Messages are tagged with a generation so results from an abandoned scan
/// (user opened another folder mid-scan) can be discarded on the UI side.
pub struct ScanHandle {
    pub cancel: Arc<AtomicBool>,
    pub files_found: Arc<AtomicU64>,
}

struct Queue {
    dirs: Mutex<(VecDeque<PathBuf>, usize)>, // (pending, active worker count)
    cv: Condvar,
}

pub use crate::metadata::mtime_of;

pub fn start_scan(root: PathBuf, generation: u64, tx: Sender<(u64, ScanMsg)>) -> ScanHandle {
    start_scan_seeds(root.clone(), vec![root], generation, tx)
}

/// Walk only `seeds` (and their descendants), emitting paths relative to
/// `root`. Used when the user multi-selects sibling folders so the canvas
/// maps those branches without scanning the rest of the parent.
pub fn start_scan_seeds(
    root: PathBuf,
    seeds: Vec<PathBuf>,
    generation: u64,
    tx: Sender<(u64, ScanMsg)>,
) -> ScanHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let files_found = Arc::new(AtomicU64::new(0));

    let handle = ScanHandle {
        cancel: cancel.clone(),
        files_found: files_found.clone(),
    };

    let seeds = if seeds.is_empty() {
        vec![root.clone()]
    } else {
        seeds
    };

    let queue = Arc::new(Queue {
        dirs: Mutex::new((VecDeque::from(seeds), 0)),
        cv: Condvar::new(),
    });

    // One snapshot for the whole walk: editing the list mid-scan must not make
    // the tree inconsistent with itself, and this keeps the check off the lock.
    let skip = crate::skiplist::effective();

    let started = Instant::now();
    let done_count = Arc::new(AtomicU64::new(0));

    for _ in 0..WORKERS {
        let queue = queue.clone();
        let tx = tx.clone();
        let root = root.clone();
        let cancel = cancel.clone();
        let files_found = files_found.clone();
        let done_count = done_count.clone();
        let skip = skip.clone();

        std::thread::spawn(move || {
            let mut batch: Vec<FileEntry> = Vec::with_capacity(BATCH);
            let mut dir_batch: Vec<(String, i64)> = Vec::new();
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
                        if let Some(d) = g.0.pop_front() {
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
                            if skip.skips(&name) {
                                continue;
                            }
                            let path = entry.path();
                            // Same directory-read data as the files below, so the
                            // folder's date is already in hand here.
                            if let Ok(md) = entry.metadata() {
                                if let Some(rel) = rel_of(&root, &path) {
                                    dir_batch.push((rel, ctime_of(&md)));
                                }
                            }
                            subdirs.push(path);
                        } else if ft.is_file() {
                            // On Windows this metadata comes from the directory
                            // read itself (FindFirstFile data) — no extra syscall.
                            let Ok(md) = entry.metadata() else { continue };
                            let size = md.len();
                            let mtime = mtime_of(&md);
                            let ctime = ctime_of(&md);
                            // Owner is deliberately absent: it lives in the
                            // security descriptor, so it costs a round trip per
                            // file and was the dominant cost of discovery on a
                            // share. `crate::owners` fills it in afterwards.
                            if let Some(fe) = FileEntry::from_abs(
                                &root,
                                entry.path(),
                                size,
                                mtime,
                                ctime,
                                String::new(),
                            ) {
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
                    if !dir_batch.is_empty() {
                        let _ =
                            tx.send((generation, ScanMsg::Dirs(std::mem::take(&mut dir_batch))));
                    }
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

            if !dir_batch.is_empty() {
                let _ = tx.send((generation, ScanMsg::Dirs(dir_batch)));
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

/// A directory's path relative to the scan root, in the backslash-separated form
/// every `rel` uses (tree building, cache keys, and the index all assume it).
fn rel_of(root: &std::path::Path, path: &std::path::Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?.to_string_lossy().into_owned();
    #[cfg(not(windows))]
    let rel = rel.replace('/', "\\");
    if rel.is_empty() {
        return None;
    }
    Some(rel)
}

/// Stat a single path (used by the filesystem watcher for incremental updates).
///
/// Owner is left empty on purpose. `owner_short` is a security-descriptor
/// round trip — fine once in a while, fatal when the UI drains a burst of
/// watcher events on a share (each call measured at several seconds). The
/// deferred owner pass fills the label the same way discovery does.
pub fn stat_file(root: &std::path::Path, path: &std::path::Path) -> Option<FileEntry> {
    let md = std::fs::metadata(path).ok()?;
    if !md.is_file() {
        return None;
    }
    let mtime = mtime_of(&md);
    let ctime = ctime_of(&md);
    FileEntry::from_abs(
        root,
        path.to_path_buf(),
        md.len(),
        mtime,
        ctime,
        String::new(),
    )
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
        let mut dirs: Vec<(String, i64)> = Vec::new();
        let done_files;
        loop {
            let (generation, msg) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
            assert_eq!(generation, 7);
            match msg {
                ScanMsg::Batch(b) => got.extend(b),
                ScanMsg::Dirs(d) => dirs.extend(d),
                ScanMsg::Done { files, .. } => {
                    done_files = files;
                    break;
                }
            }
        }
        assert_eq!(got.len(), 4);
        assert_eq!(done_files, 4);
        // Folder dates ride along with discovery rather than costing a stat
        // apiece: every directory the walk entered reports one.
        let mut dir_rels: Vec<&str> = dirs.iter().map(|(r, _)| r.as_str()).collect();
        dir_rels.sort_unstable();
        assert_eq!(dir_rels, vec!["a", "a\\b", "c"]);
        assert!(
            dirs.iter().all(|(_, c)| *c > 0),
            "each folder should carry a creation time from the listing"
        );
        let mut rels: Vec<&str> = got.iter().map(|e| e.rel.as_str()).collect();
        rels.sort();
        assert_eq!(
            rels,
            vec!["a\\b\\three.mp4", "a\\two.jpg", "c\\four.3dm", "one.txt"]
        );
        let mp4 = got.iter().find(|e| e.ext == "mp4").unwrap();
        assert_eq!(mp4.size, 3);
        assert_eq!(mp4.family, crate::types::Family::Video);
        let rhino = got.iter().find(|e| e.ext == "3dm").unwrap();
        assert_eq!(rhino.family, crate::types::Family::Cad);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn watcher_stat_does_not_pay_for_owner() {
        let root = std::env::temp_dir().join(format!("nfa_stat_owner_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("a.txt");
        std::fs::write(&path, b"hi").unwrap();
        let fe = stat_file(&root, &path).expect("stat");
        assert!(
            fe.owner.is_empty(),
            "watcher stats must not call owner_short — that lookup is seconds \
             per file on a share and runs on the UI thread"
        );
        assert_eq!(fe.size, 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The one skip that is not a preference: a cache folder's contents must
    /// never reach the index, whatever the user's list says.
    #[test]
    fn scan_never_enters_our_own_cache() {
        let root = std::env::temp_dir().join(format!("nfa_scan_cache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join(crate::thumbs::CACHE_DIR_NAME);
        std::fs::create_dir_all(cache.join("deep")).unwrap();
        std::fs::write(root.join("keep.png"), b"1").unwrap();
        std::fs::write(cache.join("thumb.jpg"), b"2").unwrap();
        std::fs::write(cache.join("deep/thumb.jpg"), b"3").unwrap();

        let (tx, rx) = unbounded();
        let _h = start_scan(root.clone(), 11, tx);

        let mut got: Vec<FileEntry> = Vec::new();
        let mut dirs: Vec<String> = Vec::new();
        loop {
            let (_, msg) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
            match msg {
                ScanMsg::Batch(b) => got.extend(b),
                ScanMsg::Dirs(d) => dirs.extend(d.into_iter().map(|(r, _)| r)),
                ScanMsg::Done { .. } => break,
            }
        }
        let rels: Vec<&str> = got.iter().map(|e| e.rel.as_str()).collect();
        assert_eq!(rels, vec!["keep.png"]);
        assert!(dirs.is_empty(), "the cache folder is not even reported");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_seeds_skip_unselected_siblings() {
        let root = std::env::temp_dir().join(format!("nfa_scan_seeds_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        std::fs::create_dir_all(root.join("c")).unwrap();
        std::fs::write(root.join("a/one.txt"), b"1").unwrap();
        std::fs::write(root.join("b/two.txt"), b"22").unwrap();
        std::fs::write(root.join("c/three.txt"), b"333").unwrap();

        let (tx, rx) = unbounded();
        let _h = start_scan_seeds(root.clone(), vec![root.join("a"), root.join("c")], 3, tx);

        let mut got: Vec<FileEntry> = Vec::new();
        loop {
            let (generation, msg) = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
            assert_eq!(generation, 3);
            match msg {
                ScanMsg::Batch(b) => got.extend(b),
                ScanMsg::Dirs(_) => {}
                ScanMsg::Done { .. } => break,
            }
        }
        let mut rels: Vec<&str> = got.iter().map(|e| e.rel.as_str()).collect();
        rels.sort();
        assert_eq!(rels, vec!["a\\one.txt", "c\\three.txt"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
