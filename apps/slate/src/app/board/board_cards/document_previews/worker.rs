//! Bounded single worker for the shared document parsers, never a thread per card.

use super::{Prepared, Source};
use slate_doc::ItemId;
use std::{
    collections::{HashMap, VecDeque},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    time::{Duration, Instant},
};

const PENDING: usize = 32;
const ADMIT_PER_FRAME: usize = 4;
const COMPLETE_PER_FRAME: usize = 4;
const CHANNEL: usize = 2;
const KEEP: usize = 64;
pub(super) const CACHE_BYTES: usize = 16 * 1024 * 1024;
type Slot = (u64, ItemId);

#[derive(Clone)]
struct Work {
    source: Source,
    generation: u64,
}

pub(super) struct Made {
    pub(super) source: Source,
    generation: u64,
    pub(super) data: Prepared,
}

#[derive(Default)]
pub(crate) struct CardPreviews {
    jobs: Option<SyncSender<Work>>,
    done: Option<Receiver<Made>>,
    queue: VecDeque<Work>,
    wanted: HashMap<Slot, Work>,
    lru: VecDeque<Slot>,
    evicted: Vec<Slot>,
    generation: u64,
    admitted: usize,
    cache_changed: bool,
    retry_after: Option<Instant>,
    #[cfg(test)]
    injected: VecDeque<Made>,
}

impl CardPreviews {
    fn ensure_worker(&mut self) {
        if self.jobs.is_some() {
            return;
        }
        let (jobs, rx) = mpsc::sync_channel::<Work>(CHANNEL);
        let (tx, done) = mpsc::sync_channel(CHANNEL);
        std::thread::spawn(move || {
            while let Ok(work) = rx.recv() {
                let data = Prepared::read(&work.source.path);
                if tx
                    .send(Made {
                        source: work.source,
                        generation: work.generation,
                        data,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.jobs = Some(jobs);
        self.done = Some(done);
    }

    pub(super) fn begin_frame(&mut self) {
        self.admitted = 0;
    }

    pub(super) fn request_matches(&self, tab: &crate::app::SlateTab, item: ItemId) -> bool {
        self.wanted
            .get(&(tab.id, item))
            .is_some_and(|work| work.source.matches(tab))
    }

    pub(super) fn can_admit(&self) -> bool {
        self.admitted < ADMIT_PER_FRAME
            && self.wanted.len() < PENDING
            && self.retry_after.is_none_or(|retry| Instant::now() >= retry)
    }

    pub(super) fn retire(&mut self, valid: impl Fn(&Source) -> bool) {
        self.wanted.retain(|_, work| valid(&work.source));
        self.queue.retain(|work| {
            self.wanted
                .get(&(work.source.tab, work.source.item))
                .is_some_and(|wanted| wanted.generation == work.generation)
        });
    }

    pub(super) fn request(&mut self, source: Source) {
        let slot = (source.tab, source.item);
        if self
            .wanted
            .get(&slot)
            .is_some_and(|work| work.source == source)
        {
            return;
        }
        if !self.can_admit() {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let work = Work {
            source,
            generation: self.generation,
        };
        self.queue
            .retain(|old| (old.source.tab, old.source.item) != slot);
        self.wanted.insert(slot, work.clone());
        self.queue.push_back(work);
        self.admitted += 1;
    }

    pub(super) fn flush(&mut self) {
        if self.retry_after.is_some_and(|retry| Instant::now() < retry) {
            return;
        }
        self.retry_after = None;
        if self.queue.is_empty() {
            return;
        }
        self.ensure_worker();
        for _ in 0..ADMIT_PER_FRAME {
            let Some(work) = self.queue.pop_front() else {
                break;
            };
            match self.jobs.as_ref().unwrap().try_send(work) {
                Ok(()) => {}
                Err(TrySendError::Full(work)) => {
                    self.queue.push_front(work);
                    break;
                }
                Err(TrySendError::Disconnected(work)) => {
                    self.queue.push_front(work);
                    self.reset_worker(&[]);
                    break;
                }
            }
        }
    }

    pub(super) fn receive(&mut self) -> Vec<Made> {
        let mut made = Vec::new();
        #[cfg(test)]
        while made.len() < COMPLETE_PER_FRAME {
            let Some(result) = self.injected.pop_front() else {
                break;
            };
            made.push(result);
        }
        let mut disconnected = false;
        while made.len() < COMPLETE_PER_FRAME {
            let Some(rx) = &self.done else {
                break;
            };
            match rx.try_recv() {
                Ok(result) => made.push(result),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            self.reset_worker(&made);
        }
        made
    }

    fn reset_worker(&mut self, completed: &[Made]) {
        self.jobs = None;
        self.done = None;
        // Submitted requests lost with the worker are retried on demand after
        // a short backoff. Keep only unsubmitted work and actual completions;
        // a dead worker must never leave permanent pending slots behind.
        self.wanted.retain(|_, work| {
            self.queue
                .iter()
                .any(|queued| queued.generation == work.generation)
                || completed
                    .iter()
                    .any(|made| made.generation == work.generation)
        });
        self.retry_after = Some(Instant::now() + Duration::from_secs(2));
    }

    pub(super) fn accept(&mut self, made: &Made) -> bool {
        let slot = (made.source.tab, made.source.item);
        if !self
            .wanted
            .get(&slot)
            .is_some_and(|work| work.generation == made.generation && work.source == made.source)
        {
            return false;
        }
        self.wanted.remove(&slot);
        self.touch(slot.0, slot.1);
        self.cache_changed = true;
        true
    }

    pub(super) fn forget(&mut self, tab: u64, item: ItemId) {
        self.wanted.remove(&(tab, item));
        self.queue
            .retain(|work| (work.source.tab, work.source.item) != (tab, item));
    }

    pub(super) fn forget_source(&mut self, path: &std::path::Path) {
        self.wanted
            .retain(|_, work| !atlas_core::workbook_assets::same_path(&work.source.path, path));
        self.queue
            .retain(|work| !atlas_core::workbook_assets::same_path(&work.source.path, path));
    }

    pub(super) fn touch(&mut self, tab: u64, item: ItemId) {
        let slot = (tab, item);
        self.lru.retain(|old| *old != slot);
        self.lru.push_back(slot);
        if self.lru.len() > KEEP {
            self.evicted.push(self.lru.pop_front().unwrap());
            self.cache_changed = true;
        }
    }

    pub(super) fn changed(&mut self) -> bool {
        std::mem::take(&mut self.cache_changed)
    }
    pub(super) fn mark_changed(&mut self) {
        self.cache_changed = true;
    }
    pub(super) fn residents(&self) -> VecDeque<Slot> {
        self.lru.clone()
    }
    pub(super) fn forget_resident(&mut self, tab: u64, item: ItemId) {
        self.lru.retain(|slot| *slot != (tab, item));
    }
    pub(super) fn take_evicted(&mut self) -> Vec<Slot> {
        std::mem::take(&mut self.evicted)
    }
    pub(super) fn busy(&self) -> bool {
        !self.wanted.is_empty()
    }

    pub(super) fn next_wake(&self) -> Option<Duration> {
        self.retry_after
            .map(|retry| retry.saturating_duration_since(Instant::now()))
            .or_else(|| self.busy().then_some(Duration::from_millis(50)))
    }

    #[cfg(test)]
    pub(super) fn inject(&mut self, source: Source, snippet: &str) {
        let generation = self
            .wanted
            .get(&(source.tab, source.item))
            .unwrap()
            .generation;
        self.injected.push_back(Made {
            source,
            generation,
            data: Prepared::snippet(Some(snippet.to_string())),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(tab: u64, item: u64) -> Source {
        Source::for_item(
            &{
                let mut tab_data = crate::app::SlateTab::empty();
                tab_data.id = tab;
                tab_data
                    .doc
                    .add_item("sample.txt".into(), "sample.txt", 0, 0, "txt");
                tab_data
            },
            ItemId(item),
        )
        .unwrap()
    }

    #[test]
    fn admission_pending_and_residency_are_bounded() {
        let mut previews = CardPreviews::default();
        for frame in 0..100 {
            previews.begin_frame();
            for item in 0..100 {
                let mut input = source(1, 1);
                input.item = ItemId(frame * 100 + item);
                previews.request(input);
            }
            assert!(previews.wanted.len() <= PENDING);
            assert!(previews.queue.len() <= PENDING);
            assert!(previews.admitted <= ADMIT_PER_FRAME);
        }
        for item in 0..KEEP + 1 {
            previews.touch(1, ItemId(item as u64));
        }
        assert_eq!(previews.lru.len(), KEEP);
        assert_eq!(previews.take_evicted(), [(1, ItemId(0))]);
    }

    #[test]
    fn stale_generation_and_same_item_in_other_tabs_do_not_alias() {
        let mut previews = CardPreviews::default();
        let a = source(11, 1);
        let b = source(12, 1);
        previews.request(a.clone());
        let old = Made {
            source: a.clone(),
            generation: previews.wanted[&(11, ItemId(1))].generation,
            data: Prepared::default(),
        };
        previews.request(b.clone());
        previews.forget(11, ItemId(1));
        previews.request(a);
        assert!(!previews.accept(&old));
        assert_eq!(previews.wanted.len(), 2);
        assert!(previews.wanted.contains_key(&(12, ItemId(1))));
    }

    #[test]
    fn a_disconnected_worker_releases_submitted_slots_and_retries_after_backoff() {
        let mut previews = CardPreviews::default();
        let source = source(11, 1);
        previews.request(source.clone());
        previews.queue.pop_front().unwrap(); // Already submitted, no longer queued.
        let (job_tx, job_rx) = mpsc::sync_channel(CHANNEL);
        let (done_tx, done_rx) = mpsc::sync_channel(CHANNEL);
        drop(job_rx);
        drop(done_tx);
        previews.jobs = Some(job_tx);
        previews.done = Some(done_rx);
        assert!(previews.receive().is_empty());
        assert!(previews.jobs.is_none() && previews.done.is_none());
        assert!(!previews.busy());
        assert!(
            !previews.can_admit(),
            "failure backoff prevents a tight respawn loop"
        );
        assert!(previews.next_wake().is_some());
        previews.retry_after = Some(Instant::now() - Duration::from_millis(1));
        previews.request(source);
        assert!(previews.busy());
        assert_eq!(previews.queue.len(), 1);
    }

    #[test]
    fn disconnection_preserves_results_received_before_worker_exit() {
        let mut previews = CardPreviews::default();
        let source = source(11, 1);
        previews.request(source.clone());
        let work = previews.queue.pop_front().unwrap();
        let (tx, rx) = mpsc::sync_channel(CHANNEL);
        tx.send(Made {
            source,
            generation: work.generation,
            data: Prepared::snippet(Some("completed words".into())),
        })
        .ok()
        .unwrap();
        drop(tx);
        previews.done = Some(rx);
        let made = previews.receive();
        assert_eq!(made.len(), 1);
        assert!(previews.accept(&made[0]));
        assert!(!previews.busy());
    }
}
