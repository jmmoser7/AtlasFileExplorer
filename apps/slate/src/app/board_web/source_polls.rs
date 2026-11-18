//! Local metadata probes share a bounded pool. Only a changed source wakes the board.

use super::*;
use std::sync::{Arc, Condvar, Mutex};

const MAX_SOURCE_POLLS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceStamp {
    pub(super) exists: bool,
    pub(super) mtime: Option<SystemTime>,
}

struct Entry {
    id: u64,
    due: Option<Instant>,
    stamp: Option<SourceStamp>,
}

#[derive(Default)]
struct Schedule {
    entries: HashMap<PathBuf, Entry>,
    stopped: bool,
}

type Shared = Arc<(Mutex<Schedule>, Condvar)>;

pub(super) struct SourcePolls {
    requested: HashSet<PathBuf>,
    watching: HashSet<PathBuf>,
    schedule: Shared,
    next_id: u64,
    workers: usize,
    // Snapshot changed observations once per board frame, shared by all placements.
    stamps: HashMap<PathBuf, SourceStamp>,
    changed: crossbeam_channel::Receiver<()>,
    sender: crossbeam_channel::Sender<()>,
}

impl Default for SourcePolls {
    fn default() -> Self {
        let (sender, changed) = crossbeam_channel::bounded(1);
        Self {
            requested: HashSet::new(),
            watching: HashSet::new(),
            schedule: Shared::default(),
            next_id: 0,
            workers: 0,
            stamps: HashMap::new(),
            changed,
            sender,
        }
    }
}

impl SourcePolls {
    pub(super) fn drain(&mut self) {
        self.requested.clear();
        if self.changed.try_recv().is_ok() {
            // Changes coalesce while the board is asleep. One frame observes
            // the latest stamp of every registered path, even after re-admission.
            let schedule = self.schedule.0.lock().unwrap();
            for (path, entry) in &schedule.entries {
                if let Some(stamp) = entry.stamp {
                    self.stamps.insert(path.clone(), stamp);
                }
            }
        }
    }

    pub(super) fn request(&mut self, path: PathBuf) -> Option<SourceStamp> {
        let stamp = self.stamps.get(&path).copied();
        self.requested.insert(path);
        stamp
    }

    pub(super) fn finish(&mut self, ctx: &egui::Context) {
        if self.watching != self.requested {
            let mut schedule = self.schedule.0.lock().unwrap();
            for path in self.watching.difference(&self.requested) {
                schedule.entries.remove(path);
                self.stamps.remove(path);
            }
            for path in self.requested.difference(&self.watching) {
                self.next_id += 1;
                schedule.entries.insert(
                    path.clone(),
                    Entry {
                        id: self.next_id,
                        due: Some(Instant::now()),
                        stamp: None,
                    },
                );
            }
            std::mem::swap(&mut self.requested, &mut self.watching);
            self.schedule.1.notify_all();
        }
        let target = self.watching.len().min(MAX_SOURCE_POLLS);
        while self.workers < target {
            let sender = self.sender.clone();
            let schedule = self.schedule.clone();
            let wake = ctx.clone();
            if std::thread::Builder::new()
                .name("slate-web-source".into())
                .spawn(move || watch_sources(schedule, sender, wake))
                .is_err()
            {
                ctx.request_repaint_after(Duration::from_secs_f32(POLL_SECS));
                break;
            }
            self.workers += 1;
        }
    }
}

impl Drop for SourcePolls {
    fn drop(&mut self) {
        self.schedule.0.lock().unwrap().stopped = true;
        self.schedule.1.notify_all();
    }
}

fn watch_sources(shared: Shared, changed: crossbeam_channel::Sender<()>, ctx: egui::Context) {
    let (schedule, wake) = &*shared;
    let mut state = schedule.lock().unwrap();
    loop {
        if state.stopped {
            return;
        }
        let next = state
            .entries
            .iter_mut()
            .filter_map(|(path, entry)| Some((entry.due?, path, entry)))
            .min_by_key(|(due, _, _)| *due);
        let Some((due, path, entry)) = next else {
            state = wake.wait(state).unwrap();
            continue;
        };
        let delay = due.saturating_duration_since(Instant::now());
        if !delay.is_zero() {
            state = wake.wait_timeout(state, delay).unwrap().0;
            continue;
        }
        let path = path.clone();
        let id = entry.id;
        entry.due = None;
        drop(state);
        // Never hold the queue lock across filesystem or egui work.
        let metadata = std::fs::metadata(&path).ok();
        let stamp = SourceStamp {
            exists: metadata.is_some(),
            mtime: metadata.and_then(|m| m.modified().ok()),
        };
        state = schedule.lock().unwrap();
        if state.stopped {
            return;
        }
        if let Some(entry) = state.entries.get_mut(&path).filter(|entry| entry.id == id) {
            entry.due = Some(Instant::now() + Duration::from_secs_f32(POLL_SECS));
            if entry.stamp.replace(stamp) != Some(stamp) {
                drop(state);
                if changed.try_send(()).is_ok() {
                    ctx.request_repaint();
                }
                state = schedule.lock().unwrap();
            }
        }
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
            polls.request(base.join("shared.html"));
        }
        for index in 0..100 {
            polls.request(base.join(format!("source-{index}.html")));
        }
        polls.finish(&ctx);
        assert_eq!(polls.workers, MAX_SOURCE_POLLS);
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end && polls.stamps.len() < 101 {
            polls.drain();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(polls.stamps.len(), 101);
    }

    #[test]
    fn static_file_stays_quiet_and_external_edits_wake_without_a_board_frame() {
        let path = std::env::temp_dir().join(format!(
            "slate-quiet-web-source-{}.html",
            std::process::id()
        ));
        std::fs::write(&path, "first").unwrap();
        let mut polls = SourcePolls::default();
        let ctx = egui::Context::default();
        polls.request(path.clone());
        polls.finish(&ctx);
        let observe = |polls: &mut SourcePolls| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while polls.changed.is_empty() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                !polls.changed.is_empty(),
                "file change did not wake the board"
            );
            polls.drain();
            polls.stamps[&path].exists
        };
        assert!(observe(&mut polls));
        assert!(polls
            .changed
            .recv_timeout(Duration::from_secs_f32(POLL_SECS * 2.5))
            .is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(!observe(&mut polls));
        std::fs::write(&path, "replacement").unwrap();
        assert!(observe(&mut polls));
        // The last placement left the worklist. Its later removal is silent.
        polls.finish(&ctx);
        std::fs::remove_file(&path).unwrap();
        assert!(polls
            .changed
            .recv_timeout(Duration::from_secs_f32(POLL_SECS * 1.2))
            .is_err());
    }
}
