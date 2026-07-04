//! Overnight pre-warm: fill shared `.atlas-cache` repositories in the
//! background so cold folders open instantly on every machine.
//!
//! A pre-warm run is intentionally root-independent — its thumbnail requests
//! are tagged `PINNED_GENERATION` so they survive tab/root changes, and its
//! only output is the (shared) disk cache. UI lives in `ui/readouts.rs`
//! (bottom dashboard) and `ui/advanced.rs` (start/cancel controls).

use super::{wants_thumb, AtlasApp};
use crate::scanner;
use crate::thumbs::{cache_key, ThumbRequest};
use crate::types::FileEntry;
use crossbeam_channel::unbounded;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

/// One explicit pre-warm run (Advanced settings → "Pre-warm a folder…").
/// The atomics are written by the background discovery walk; everything else
/// is UI-thread bookkeeping. Drives the temporary bottom dashboard.
pub(in crate::app) struct PrewarmJob {
    /// Folder the user picked.
    pub(in crate::app) dir: PathBuf,
    pub(in crate::app) started: Instant,
    /// Thumbnail-able files discovered and queued so far.
    pub(in crate::app) queued: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Total source bytes of the queued files.
    pub(in crate::app) bytes_queued: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Shared `.atlas-cache` repositories created or reused by this run.
    pub(in crate::app) repos: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Discovery walk has finished (queued/bytes_queued are final).
    pub(in crate::app) walk_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Set by Cancel — the walk thread stops queueing and exits.
    pub(in crate::app) cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Thumbnails completed (cached or failed) so far.
    pub(in crate::app) done: usize,
    /// Source bytes behind the completed thumbnails.
    pub(in crate::app) bytes_done: u64,
    /// Rolling (time, done, bytes_done) samples for the speed readout.
    pub(in crate::app) samples: VecDeque<(Instant, usize, u64)>,
}

impl PrewarmJob {
    pub(in crate::app) fn queued_now(&self) -> usize {
        self.queued.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(in crate::app) fn walk_done_now(&self) -> bool {
        // Acquire pairs with the walk thread's Release store: seeing `true`
        // guarantees every queued/bytes increment is visible too, so the
        // completion check can't fire early on a stale count.
        self.walk_done.load(std::sync::atomic::Ordering::Acquire)
    }

    pub(in crate::app) fn remaining(&self) -> usize {
        self.queued_now().saturating_sub(self.done)
    }

    pub(in crate::app) fn complete(&self) -> bool {
        self.walk_done_now() && self.done >= self.queued_now()
    }

    /// Record a finished thumbnail and refresh the rolling speed window.
    pub(in crate::app) fn record_done(&mut self, src_bytes: u64) {
        self.done += 1;
        self.bytes_done += src_bytes;
        let now = Instant::now();
        self.samples.push_back((now, self.done, self.bytes_done));
        while let Some((t, _, _)) = self.samples.front() {
            if now.duration_since(*t).as_secs_f32() > 5.0 {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// (files/s, bytes/s) over the last few seconds of completions.
    pub(in crate::app) fn speed(&self) -> (f32, f64) {
        let (Some((t0, d0, b0)), Some((t1, d1, b1))) = (self.samples.front(), self.samples.back())
        else {
            return (0.0, 0.0);
        };
        let dt = t1.duration_since(*t0).as_secs_f32();
        if dt < 0.2 {
            return (0.0, 0.0);
        }
        ((d1 - d0) as f32 / dt, (b1 - b0) as f64 / dt as f64)
    }
}

/// Discovery walk behind `start_prewarm`, extracted so repository creation
/// is testable. Descends from `dir`, queueing every thumbnail-able file via
/// `queue`. Shared `.atlas-cache` repositories are created (and counted in
/// `repos`) both by walking *up* from `dir` (picked inside a project) and
/// while descending (picked a folder that contains projects); cache keys are
/// project-root-relative wherever a repository applies so every machine
/// agrees on them.
fn prewarm_walk(
    dir: PathBuf,
    queue: &dyn Fn(ThumbRequest),
    queued: &std::sync::atomic::AtomicUsize,
    bytes_queued: &std::sync::atomic::AtomicU64,
    repos: &std::sync::atomic::AtomicUsize,
    cancel: &std::sync::atomic::AtomicBool,
) {
    use std::sync::atomic::Ordering::Relaxed;
    // Per-subtree cache context: (key base, shared repository).
    type Ctx = (std::sync::Arc<PathBuf>, Option<std::sync::Arc<PathBuf>>);
    // Picked folder inside (or at) a project root: walk up.
    let root_ctx: Ctx = match crate::thumbs::discover_project_cache(&dir) {
        Some(pc) if crate::thumbs::create_shared_repo(&pc.shared_dir) => {
            repos.fetch_add(1, Relaxed);
            (
                std::sync::Arc::new(pc.project_root),
                Some(std::sync::Arc::new(pc.shared_dir)),
            )
        }
        _ => (std::sync::Arc::new(dir.clone()), None),
    };
    let mut stack: Vec<(PathBuf, Ctx)> = vec![(dir, root_ctx)];
    while let Some((d, mut ctx)) = stack.pop() {
        if cancel.load(Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        // Read the directory once, then (if not already inside a project)
        // check whether `d` is itself a project root so files below it land
        // in that project's repository.
        let mut subdirs = Vec::new();
        let mut files = Vec::new();
        for entry in rd.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if scanner::SKIP_DIRS
                    .iter()
                    .any(|s| name.eq_ignore_ascii_case(s))
                {
                    continue;
                }
                subdirs.push(entry.path());
            } else if ft.is_file() {
                files.push(entry);
            }
        }
        if ctx.1.is_none() {
            if let Some(shared) = crate::thumbs::project_anchor_under(&d) {
                if crate::thumbs::create_shared_repo(&shared) {
                    repos.fetch_add(1, Relaxed);
                    ctx = (
                        std::sync::Arc::new(d.clone()),
                        Some(std::sync::Arc::new(shared)),
                    );
                }
            }
        }
        for entry in files {
            if cancel.load(Relaxed) {
                break;
            }
            let Ok(md) = entry.metadata() else { continue };
            let mtime = scanner::mtime_of(&md);
            let ctime = crate::metadata::ctime_of(&md);
            let owner = crate::metadata::owner_short(&entry.path());
            let Some(fe) = FileEntry::from_abs(&ctx.0, entry.path(), md.len(), mtime, ctime, owner)
            else {
                continue;
            };
            if !wants_thumb(fe.family) {
                continue;
            }
            let key = cache_key(&fe.rel, fe.size, fe.mtime);
            queue(ThumbRequest {
                id: u32::MAX,
                generation: crate::thumbs::PINNED_GENERATION,
                path: fe.path,
                key,
                color_only: false,
                shared_dir: ctx.1.clone(),
                src_bytes: fe.size,
            });
            queued.fetch_add(1, Relaxed);
            bytes_queued.fetch_add(fe.size, Relaxed);
        }
        for sd in subdirs {
            stack.push((sd, ctx.clone()));
        }
    }
}

impl AtlasApp {
    pub(in crate::app) fn open_prewarm_dialog(&mut self) {
        if self.prewarm_picker_rx.is_some() {
            return;
        }
        let (tx, rx) = unbounded();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Choose a folder to pre-warm (runs quietly in background)")
                .pick_folder();
            let _ = tx.send(picked);
        });
        self.prewarm_picker_rx = Some(rx);
    }

    /// Walk `dir` on a background thread and queue every thumbnail-able file
    /// into the low-priority slow lane (user-adjustable concurrency, survives
    /// root changes). Shared `.atlas-cache` repositories are created both by
    /// walking *up* from the picked folder (picked a subfolder of a project)
    /// and while descending (picked a folder that contains projects), so keys
    /// stay project-root-relative and every project gets its repository.
    pub(in crate::app) fn start_prewarm(&mut self, dir: PathBuf) {
        if self.prewarm.is_some() {
            self.toast("A pre-warm is already running — cancel it first");
            return;
        }
        let pool = self.thumbs.clone();
        let job = PrewarmJob {
            dir: dir.clone(),
            started: Instant::now(),
            queued: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            bytes_queued: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            repos: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            walk_done: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            done: 0,
            bytes_done: 0,
            samples: VecDeque::new(),
        };
        let queued = job.queued.clone();
        let bytes_queued = job.bytes_queued.clone();
        let repos = job.repos.clone();
        let walk_done = job.walk_done.clone();
        let cancel = job.cancel.clone();
        self.prewarm = Some(job);
        if crate::thumbs::is_network_path(&dir) {
            self.thumbs.ensure_workers(24);
        }
        self.toast(format!("Pre-warming {} in the background", dir.display()));
        std::thread::spawn(move || {
            prewarm_walk(
                dir,
                &|req| pool.request_slow(req),
                &queued,
                &bytes_queued,
                &repos,
                &cancel,
            );
            walk_done.store(true, std::sync::atomic::Ordering::Release);
        });
    }

    /// Stop the active pre-warm: the discovery walk exits, queued jobs are
    /// dropped, and the handful already in-flight finish harmlessly (their
    /// results are ignored once the job is gone).
    pub(crate) fn cancel_prewarm(&mut self) {
        let Some(job) = self.prewarm.take() else {
            return;
        };
        job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        let dropped = self.thumbs.cancel_slow();
        self.toast(format!(
            "Pre-warm cancelled — {} thumbnails built, {} skipped",
            job.done, dropped
        ));
    }

    pub(crate) fn prewarm_remaining(&self) -> usize {
        self.prewarm.as_ref().map(|j| j.remaining()).unwrap_or(0)
    }

    /// Discard stale thumbnail results on a root change without losing
    /// pre-warm progress accounting (pinned results are generation-less).
    pub(in crate::app) fn flush_thumb_results(&mut self) {
        while let Ok(res) = self.thumbs.rx.try_recv() {
            if res.generation == crate::thumbs::PINNED_GENERATION {
                if let Some(job) = &mut self.prewarm {
                    job.record_done(res.src_bytes);
                }
            }
        }
    }
}

#[cfg(test)]
mod prewarm_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn make_project(root: &std::path::Path, name: &str) -> PathBuf {
        let project = root.join(name);
        let anchor = project
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA");
        std::fs::create_dir_all(&anchor).unwrap();
        project
    }

    fn run_walk(dir: PathBuf) -> (Vec<ThumbRequest>, usize, usize, u64) {
        let reqs = Mutex::new(Vec::new());
        let queued = AtomicUsize::new(0);
        let bytes = AtomicU64::new(0);
        let repos = AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        prewarm_walk(
            dir,
            &|r| reqs.lock().unwrap().push(r),
            &queued,
            &bytes,
            &repos,
            &cancel,
        );
        (
            reqs.into_inner().unwrap(),
            queued.load(Ordering::Relaxed),
            repos.load(Ordering::Relaxed),
            bytes.load(Ordering::Relaxed),
        )
    }

    #[test]
    fn prewarm_creates_repositories_for_projects_below_picked_folder() {
        let root = std::env::temp_dir().join(format!("nfa_pw_below_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let office = root.join("NYC");
        let p1 = make_project(&office, "26001 - Tower");
        let p2 = make_project(&office, "26002 - Museum");
        std::fs::write(p1.join("a.png"), b"x").unwrap();
        std::fs::write(p2.join("02 DESIGN").join("b.jpg"), b"xy").unwrap();
        // A file outside any project has no shared repository.
        std::fs::write(office.join("loose.png"), b"xyz").unwrap();

        let (reqs, queued, repos, bytes) = run_walk(office.clone());
        assert_eq!(queued, 3);
        assert_eq!(
            repos, 2,
            "one repository per project found while descending"
        );
        assert_eq!(bytes, 6);
        let cache1 = p1
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA")
            .join(crate::thumbs::CACHE_DIR_NAME);
        let cache2 = p2
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA")
            .join(crate::thumbs::CACHE_DIR_NAME);
        assert!(cache1.is_dir(), "repository created for project 1");
        assert!(cache2.is_dir(), "repository created for project 2");

        // Each queued file carries its own project's repository (or none),
        // and keys are project-root-relative so any machine agrees.
        for r in &reqs {
            let name = r.path.file_name().unwrap().to_string_lossy().into_owned();
            match name.as_str() {
                "a.png" => {
                    assert_eq!(r.shared_dir.as_deref(), Some(&cache1));
                    assert_eq!(r.key, cache_key("a.png", 1, mtime_of_file(&r.path)));
                }
                "b.jpg" => {
                    assert_eq!(r.shared_dir.as_deref(), Some(&cache2));
                    // `rel` (and therefore cache keys) are backslash-separated
                    // on every platform so machines agree on shared keys.
                    assert_eq!(
                        r.key,
                        cache_key("02 DESIGN\\b.jpg", 2, mtime_of_file(&r.path))
                    );
                }
                "loose.png" => assert!(r.shared_dir.is_none()),
                other => panic!("unexpected file {other}"),
            }
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prewarm_from_subfolder_finds_repository_above() {
        let root = std::env::temp_dir().join(format!("nfa_pw_above_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let project = make_project(&root, "26003 - Bridge");
        let sketches = project.join("02 DESIGN").join("01 SKETCHES");
        std::fs::create_dir_all(&sketches).unwrap();
        std::fs::write(sketches.join("c.png"), b"x").unwrap();

        let (reqs, queued, repos, _) = run_walk(sketches.clone());
        assert_eq!(queued, 1);
        assert_eq!(repos, 1, "repository discovered by walking up");
        let cache = project
            .join("02 DESIGN")
            .join("05 RESOURCES")
            .join("03 DATA")
            .join(crate::thumbs::CACHE_DIR_NAME);
        assert!(cache.is_dir());
        assert_eq!(reqs[0].shared_dir.as_deref(), Some(&cache));
        // Key is project-root-relative even though a subfolder was picked;
        // keys use backslashes on every platform so machines agree.
        let rel = "02 DESIGN\\01 SKETCHES\\c.png";
        assert_eq!(reqs[0].key, cache_key(rel, 1, mtime_of_file(&reqs[0].path)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prewarm_cancel_stops_the_walk() {
        let root = std::env::temp_dir().join(format!("nfa_pw_cancel_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("d.png"), b"x").unwrap();
        let reqs = Mutex::new(Vec::new());
        let queued = AtomicUsize::new(0);
        let bytes = AtomicU64::new(0);
        let repos = AtomicUsize::new(0);
        let cancel = AtomicBool::new(true); // cancelled before it starts
        prewarm_walk(
            root.clone(),
            &|r| reqs.lock().unwrap().push(r),
            &queued,
            &bytes,
            &repos,
            &cancel,
        );
        assert_eq!(queued.load(Ordering::Relaxed), 0);
        assert!(reqs.into_inner().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    fn mtime_of_file(p: &std::path::Path) -> i64 {
        scanner::mtime_of(&std::fs::metadata(p).unwrap())
    }
}
