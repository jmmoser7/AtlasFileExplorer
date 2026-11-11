//! Local metadata probes follow the source path shared by its placements.

use super::*;

const MAX_SOURCE_POLLS: usize = 4;

#[derive(Clone, Copy)]
pub(super) struct SourceStamp {
    pub(super) exists: bool,
    pub(super) mtime: Option<SystemTime>,
}

struct SourceEntry {
    last_used: Instant,
    pending: bool,
    completed: Option<(Instant, SourceStamp)>,
}

pub(super) struct SourcePolls {
    entries: HashMap<PathBuf, SourceEntry>,
    finished: crossbeam_channel::Receiver<(PathBuf, SourceStamp)>,
    ready: crossbeam_channel::Sender<(PathBuf, SourceStamp)>,
    pending: usize,
    last_prune: Instant,
}

impl Default for SourcePolls {
    fn default() -> Self {
        let (ready, finished) = crossbeam_channel::unbounded();
        Self {
            entries: HashMap::new(),
            finished,
            ready,
            pending: 0,
            last_prune: Instant::now(),
        }
    }
}

impl SourcePolls {
    pub(super) fn drain(&mut self) {
        let now = Instant::now();
        while let Ok((path, stamp)) = self.finished.try_recv() {
            self.pending -= 1;
            if let Some(entry) = self.entries.get_mut(&path) {
                entry.pending = false;
                entry.last_used = now;
                // Slow sources must yield the freed worker slot to other
                // paths before they can become due for another refresh.
                entry.completed = Some((now, stamp));
            }
        }
        if self.last_prune.elapsed() >= Duration::from_secs(30) {
            self.entries.retain(|_, entry| {
                entry.pending || entry.last_used.elapsed() < Duration::from_secs(30)
            });
            self.last_prune = now;
        }
    }

    pub(super) fn request(&mut self, ctx: &egui::Context, path: PathBuf) -> Option<SourceStamp> {
        let now = Instant::now();
        if let Some(entry) = self.entries.get_mut(&path) {
            entry.last_used = now;
            if entry.pending
                || entry
                    .completed
                    .is_some_and(|(at, _)| at.elapsed().as_secs_f32() < POLL_SECS)
            {
                return entry.completed.map(|(_, stamp)| stamp);
            }
        }
        let completed = self.entries.get(&path).and_then(|entry| entry.completed);
        let stamp = completed.map(|(_, stamp)| stamp);
        if self.pending >= MAX_SOURCE_POLLS {
            return stamp;
        }
        let ready = self.ready.clone();
        let worker_path = path.clone();
        let ctx = ctx.clone();
        if std::thread::Builder::new()
            .name("slate-web-source".into())
            .spawn(move || {
                let metadata = std::fs::metadata(&worker_path).ok();
                let stamp = SourceStamp {
                    exists: metadata.is_some(),
                    mtime: metadata.and_then(|value| value.modified().ok()),
                };
                if ready.send((worker_path, stamp)).is_ok() {
                    ctx.request_repaint();
                }
            })
            .is_ok()
        {
            self.pending += 1;
            self.entries.insert(
                path,
                SourceEntry {
                    last_used: now,
                    pending: true,
                    completed,
                },
            );
        }
        stamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_share_a_probe_and_distinct_sources_respect_the_limit() {
        let mut polls = SourcePolls::default();
        let ctx = egui::Context::default();
        let base = std::env::temp_dir().join(format!("slate-source-polls-{}", std::process::id()));
        for _ in 0..2_400 {
            let _ = polls.request(&ctx, base.join("shared.html"));
        }
        assert_eq!(polls.pending, 1);
        for index in 0..100 {
            let _ = polls.request(&ctx, base.join(format!("source-{index}.html")));
        }
        assert_eq!(polls.pending, MAX_SOURCE_POLLS);
        assert_eq!(polls.entries.len(), MAX_SOURCE_POLLS);

        let finished = (0..MAX_SOURCE_POLLS)
            .map(|_| polls.finished.recv_timeout(Duration::from_secs(5)).unwrap())
            .collect::<Vec<_>>();
        std::thread::sleep(Duration::from_secs_f32(POLL_SECS) + Duration::from_millis(25));
        for result in &finished {
            polls.ready.send(result.clone()).unwrap();
        }
        polls.drain();
        for (path, _) in finished {
            assert!(polls.request(&ctx, path).is_some());
        }
        assert_eq!(polls.pending, 0, "a slow probe must yield its slot");
        let later = base.join("source-99.html");
        assert!(!polls.entries.contains_key(&later));
        assert!(polls.request(&ctx, later).is_none());
        assert_eq!(polls.pending, 1);
    }
}
