use crate::item::SlateItem;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

/// Whether the linked source file still exists on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Ok,
    Missing,
    /// Not checked yet, or the filesystem could not answer (for example,
    /// an unavailable share or permission error). Never evidence of deletion.
    Unknown,
}

/// Checks a linked file synchronously. This may block on a network filesystem;
/// UI consumers must use [`LinkHealthCache`] instead.
pub fn link_status(item: &SlateItem) -> LinkStatus {
    probe_path(&item.path)
}

fn classify_exists(result: io::Result<bool>) -> LinkStatus {
    match result {
        Ok(true) => LinkStatus::Ok,
        Ok(false) => LinkStatus::Missing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => LinkStatus::Missing,
        Err(_) => LinkStatus::Unknown,
    }
}

fn probe_path(path: &Path) -> LinkStatus {
    classify_exists(path.try_exists())
}

const WORKERS: usize = 2;
const MAX_OUTSTANDING: usize = 16;
const REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// Counts item references, so two items sharing a path both contribute.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LinkHealthCounts {
    pub ok: usize,
    pub missing: usize,
    pub unknown: usize,
}

impl LinkHealthCounts {
    fn bucket(&mut self, status: LinkStatus) -> &mut usize {
        match status {
            LinkStatus::Ok => &mut self.ok,
            LinkStatus::Missing => &mut self.missing,
            LinkStatus::Unknown => &mut self.unknown,
        }
    }
}

struct Entry {
    status: LinkStatus,
    references: usize,
    next_probe: Instant,
}

struct Probe {
    generation: u64,
    path: PathBuf,
}

struct ProbeResult {
    probe: Probe,
    status: LinkStatus,
}

/// Background link checks owned by one document's UI state.
///
/// All public methods perform memory/channel work only, never filesystem I/O.
/// Two persistent workers and bounded channels isolate slow network metadata
/// calls. Dropping the cache disconnects the channels without joining workers;
/// an OS call already in progress is allowed to finish on its worker.
///
/// Call `sync_paths` when the document's linked paths change, `tick` during
/// updates, and use the cached status/counts when painting. Keep an occasional
/// update (at least every 30 seconds) even when `tick` returns false, so cached
/// results refresh. A positive return means outstanding work needs polling.
pub struct LinkHealthCache {
    entries: HashMap<PathBuf, Entry>,
    due: BinaryHeap<Reverse<(Instant, PathBuf)>>,
    generation: u64,
    counts: LinkHealthCounts,
    jobs: Option<mpsc::SyncSender<Probe>>,
    results: Option<mpsc::Receiver<ProbeResult>>,
    probe: Arc<dyn Fn(&Path) -> LinkStatus + Send + Sync>,
    outstanding: usize,
}

impl Default for LinkHealthCache {
    fn default() -> Self {
        Self::with_probe(Arc::new(probe_path))
    }
}

impl LinkHealthCache {
    fn with_probe(probe: Arc<dyn Fn(&Path) -> LinkStatus + Send + Sync>) -> Self {
        Self {
            entries: HashMap::new(),
            due: BinaryHeap::new(),
            generation: 0,
            counts: LinkHealthCounts::default(),
            jobs: None,
            results: None,
            probe,
            outstanding: 0,
        }
    }

    fn ensure_workers(&mut self) {
        if self.jobs.is_some() {
            return;
        }
        let (jobs, incoming) = mpsc::sync_channel::<Probe>(MAX_OUTSTANDING);
        let (completed, results) = mpsc::sync_channel(MAX_OUTSTANDING);
        let incoming = Arc::new(Mutex::new(incoming));
        for _ in 0..WORKERS {
            let incoming = incoming.clone();
            let completed = completed.clone();
            let probe = self.probe.clone();
            std::thread::spawn(move || loop {
                // Release the receiver lock before touching the filesystem:
                // one blocked share must not serialize both workers.
                let job = incoming.lock().unwrap().recv();
                let Ok(job) = job else { break };
                let status = probe(&job.path);
                if completed.send(ProbeResult { probe: job, status }).is_err() {
                    break;
                }
            });
        }
        self.jobs = Some(jobs);
        self.results = Some(results);
    }

    /// Replace the tracked document paths. Unchanged paths retain their cached
    /// answer; removed paths and all results from the previous generation are
    /// discarded. Path comparison is lexical and never canonicalizes on disk.
    pub fn sync_paths<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) {
        let now = Instant::now();
        let mut old = std::mem::take(&mut self.entries);
        self.generation = self.generation.wrapping_add(1);
        self.due.clear();
        self.counts = LinkHealthCounts::default();
        for path in paths {
            let entry = self.entries.entry(path.to_path_buf()).or_insert_with(|| {
                let mut entry = old.remove(path).unwrap_or(Entry {
                    status: LinkStatus::Unknown,
                    references: 0,
                    next_probe: now,
                });
                entry.references = 0;
                entry
            });
            entry.references += 1;
            *self.counts.bucket(entry.status) += 1;
        }
        // Exactly one deadline per current path; old in-flight work is bounded
        // by MAX_OUTSTANDING and cannot grow retained document state.
        self.due = self
            .entries
            .iter()
            .map(|(path, entry)| Reverse((entry.next_probe, path.clone())))
            .collect();
    }

    pub fn status(&self, path: &Path) -> LinkStatus {
        self.entries
            .get(path)
            .map_or(LinkStatus::Unknown, |entry| entry.status)
    }

    pub fn counts(&self) -> LinkHealthCounts {
        self.counts
    }

    /// Drain and schedule at most sixteen checks each. Never waits for a worker.
    pub fn tick(&mut self) -> bool {
        self.tick_at(Instant::now())
    }

    fn tick_at(&mut self, now: Instant) -> bool {
        if self.entries.is_empty() && self.outstanding == 0 {
            return false;
        }
        self.ensure_workers();
        for _ in 0..MAX_OUTSTANDING {
            let Ok(result) = self.results.as_ref().unwrap().try_recv() else {
                break;
            };
            self.outstanding -= 1;
            if result.probe.generation != self.generation {
                continue;
            }
            if let Some(entry) = self.entries.get_mut(&result.probe.path) {
                *self.counts.bucket(entry.status) -= entry.references;
                entry.status = result.status;
                *self.counts.bucket(entry.status) += entry.references;
                entry.next_probe = now + REFRESH_INTERVAL;
                self.due
                    .push(Reverse((entry.next_probe, result.probe.path)));
            }
        }
        for _ in 0..MAX_OUTSTANDING {
            if self.outstanding >= MAX_OUTSTANDING
                || self.due.peek().is_none_or(|Reverse((at, _))| *at > now)
            {
                break;
            }
            let Reverse((at, path)) = self.due.pop().unwrap();
            let job = Probe {
                generation: self.generation,
                path,
            };
            match self.jobs.as_ref().unwrap().try_send(job) {
                Ok(()) => self.outstanding += 1,
                Err(mpsc::TrySendError::Full(job) | mpsc::TrySendError::Disconnected(job)) => {
                    self.due.push(Reverse((at, job.path)));
                    break;
                }
            }
        }
        self.outstanding > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Condvar;

    #[derive(Default)]
    struct Gate(Mutex<bool>, Condvar);

    impl Gate {
        fn wait(&self) {
            let guard = self.0.lock().unwrap();
            drop(self.1.wait_while(guard, |open| !*open).unwrap());
        }

        fn open(&self) {
            *self.0.lock().unwrap() = true;
            self.1.notify_all();
        }
    }

    fn settle(cache: &mut LinkHealthCache, now: Instant) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while cache.tick_at(now) {
            assert!(Instant::now() < deadline, "probe results did not arrive");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn absence_and_unavailable_filesystems_have_distinct_statuses() {
        assert_eq!(classify_exists(Ok(true)), LinkStatus::Ok);
        assert_eq!(classify_exists(Ok(false)), LinkStatus::Missing);
        assert_eq!(
            classify_exists(Err(io::ErrorKind::NotFound.into())),
            LinkStatus::Missing
        );
        for kind in [
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::TimedOut,
            io::ErrorKind::NotConnected,
        ] {
            assert_eq!(classify_exists(Err(kind.into())), LinkStatus::Unknown);
        }
    }

    #[test]
    fn empty_documents_do_not_start_workers() {
        let mut cache = LinkHealthCache::default();
        cache.sync_paths(std::iter::empty());
        assert!(!cache.tick());
        assert!(cache.jobs.is_none());
        assert_eq!(cache.status(Path::new("untracked")), LinkStatus::Unknown);
        assert_eq!(cache.counts(), LinkHealthCounts::default());
    }

    #[test]
    fn blocked_probes_do_not_block_tick_or_drop_and_work_is_bounded() {
        let gate = Arc::new(Gate::default());
        let worker_gate = gate.clone();
        let (started, starts) = mpsc::channel();
        let (progress, observed) = mpsc::channel();
        let runner = std::thread::spawn(move || {
            let mut cache = LinkHealthCache::with_probe(Arc::new(move |_| {
                started.send(()).unwrap();
                worker_gate.wait();
                LinkStatus::Ok
            }));
            let paths: Vec<PathBuf> = (0..100).map(|i| format!("virtual-{i}").into()).collect();
            cache.sync_paths(paths.iter().map(PathBuf::as_path));
            assert!(cache.jobs.is_none(), "sync must not start filesystem work");
            for _ in 0..100 {
                assert!(cache.tick());
                assert_eq!(cache.outstanding, MAX_OUTSTANDING);
                assert_eq!(cache.due.len(), 100 - MAX_OUTSTANDING);
            }
            progress.send("tick returned").unwrap();
            drop(cache);
            progress.send("drop returned").unwrap();
        });
        let tick = observed.recv_timeout(Duration::from_secs(3));
        let dropped = observed.recv_timeout(Duration::from_secs(3));
        let first = starts.recv_timeout(Duration::from_secs(3));
        let second = starts.recv_timeout(Duration::from_secs(3));
        let extra = starts.try_recv();
        // Always release blocked workers before reporting a failed assertion.
        gate.open();
        runner.join().unwrap();
        assert_eq!(tick.unwrap(), "tick returned");
        assert_eq!(dropped.unwrap(), "drop returned");
        assert!(first.is_ok() && second.is_ok());
        assert!(
            matches!(extra, Err(mpsc::TryRecvError::Empty)),
            "only two workers may enter the probe"
        );
    }

    #[test]
    fn removed_and_relinked_generations_cannot_publish_old_results() {
        let old_gate = Arc::new(Gate::default());
        let new_gate = Arc::new(Gate::default());
        let old_worker_gate = old_gate.clone();
        let new_worker_gate = new_gate.clone();
        let calls = AtomicUsize::new(0);
        let (started, starts) = mpsc::channel();
        let mut cache = LinkHealthCache::with_probe(Arc::new(move |_| {
            let call = calls.fetch_add(1, Ordering::SeqCst);
            started.send(call).unwrap();
            if call < 2 {
                old_worker_gate.wait();
                LinkStatus::Missing
            } else {
                new_worker_gate.wait();
                LinkStatus::Ok
            }
        }));
        cache.sync_paths([Path::new("removed"), Path::new("shared")]);
        cache.tick();
        for _ in 0..2 {
            starts.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        cache.sync_paths([Path::new("shared"), Path::new("replacement")]);
        cache.tick();
        old_gate.open();
        for _ in 0..2 {
            starts.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        cache.tick();
        let stale_counts = cache.counts();
        let removed = cache.status(Path::new("removed"));
        new_gate.open();
        settle(&mut cache, Instant::now());
        assert_eq!(
            stale_counts,
            LinkHealthCounts {
                unknown: 2,
                ..Default::default()
            }
        );
        assert_eq!(removed, LinkStatus::Unknown);
        assert_eq!(
            cache.counts(),
            LinkHealthCounts {
                ok: 2,
                ..Default::default()
            }
        );
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(cache.due.len(), 2);
    }

    #[test]
    fn duplicate_item_references_count_once_per_item_but_probe_once_per_path() {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        let mut cache = LinkHealthCache::with_probe(Arc::new(move |path| {
            worker_calls.fetch_add(1, Ordering::SeqCst);
            if path == Path::new("missing") {
                LinkStatus::Missing
            } else {
                LinkStatus::Ok
            }
        }));
        cache.sync_paths([Path::new("ok"), Path::new("ok"), Path::new("missing")]);
        assert_eq!(cache.counts().unknown, 3);
        settle(&mut cache, Instant::now());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            cache.counts(),
            LinkHealthCounts {
                ok: 2,
                missing: 1,
                unknown: 0
            }
        );
        cache.sync_paths([Path::new("ok")]);
        assert!(!cache.tick());
        assert_eq!(
            cache.counts(),
            LinkHealthCounts {
                ok: 1,
                ..Default::default()
            }
        );
        cache.sync_paths(std::iter::empty());
        assert_eq!(cache.counts(), LinkHealthCounts::default());
        assert!(cache.entries.is_empty() && cache.due.is_empty());
    }

    #[test]
    fn refresh_retries_unknown_without_rechecking_on_every_frame() {
        let calls = Arc::new(AtomicUsize::new(0));
        let worker_calls = calls.clone();
        let mut cache = LinkHealthCache::with_probe(Arc::new(move |_| {
            if worker_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                LinkStatus::Unknown
            } else {
                LinkStatus::Ok
            }
        }));
        cache.sync_paths([Path::new("temporarily-unavailable")]);
        let now = Instant::now();
        settle(&mut cache, now);
        assert_eq!(cache.counts().unknown, 1);
        for _ in 0..100 {
            assert!(!cache.tick_at(now + REFRESH_INTERVAL - Duration::from_millis(1)));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        settle(&mut cache, now + REFRESH_INTERVAL);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(cache.counts().ok, 1);
    }
}
