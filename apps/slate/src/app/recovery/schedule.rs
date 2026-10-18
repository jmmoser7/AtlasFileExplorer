//! Fair, bounded snapshot scheduling. No document traversal or I/O here.
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct Tracked {
    key: u64,
    dirty_since: Option<Instant>,
    captured: Option<Instant>,
    has_capture: bool,
    settle_due: Option<Instant>,
    settle_in_flight: bool,
}

#[derive(Default)]
pub(super) struct Schedule {
    tabs: HashMap<u64, Tracked>,
    next_key: u64,
    cursor: usize,
}

impl Schedule {
    pub fn observe(&mut self, id: u64, dirty: bool, now: Instant) {
        if !dirty && !self.tabs.contains_key(&id) {
            return;
        }
        let tracked = self.tabs.entry(id).or_insert_with(|| {
            self.next_key += 1;
            Tracked {
                key: self.next_key,
                dirty_since: None,
                captured: None,
                has_capture: false,
                settle_due: None,
                settle_in_flight: false,
            }
        });
        if dirty {
            if tracked.dirty_since.is_none() {
                tracked.captured = None;
                tracked.settle_due = None;
                tracked.settle_in_flight = false;
            }
            tracked.dirty_since.get_or_insert(now);
        } else {
            if tracked.dirty_since.take().is_some() && tracked.has_capture {
                tracked.settle_due = Some(now);
            }
        }
    }

    /// Only the caller's tab order matters; inactive and lease-lost tabs count.
    pub fn next(&mut self, ids: &[u64], now: Instant, interval: Duration) -> Option<(usize, u64)> {
        for offset in 0..ids.len() {
            let index = (self.cursor + offset) % ids.len();
            let Some(tab) = self.tabs.get(&ids[index]) else {
                continue;
            };
            let due = if let Some(due) = tab.settle_due {
                due
            } else if let Some(dirty_since) = tab.dirty_since {
                tab.captured.map_or(dirty_since, |at| at + interval)
            } else {
                continue;
            };
            if now >= due {
                self.cursor = (index + 1) % ids.len();
                return Some((index, tab.key));
            }
        }
        None
    }

    pub fn captured(&mut self, id: u64, now: Instant) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.captured = Some(now);
            tab.has_capture = true;
            if tab.dirty_since.is_none() && tab.settle_due.take().is_some() {
                tab.settle_in_flight = true;
            }
        }
    }

    pub fn completed(&mut self, key: u64, success: bool, now: Instant) {
        if let Some(tab) = self.tabs.values_mut().find(|tab| tab.key == key) {
            if std::mem::take(&mut tab.settle_in_flight) && !success && tab.dirty_since.is_none() {
                tab.settle_due = Some(now + Duration::from_secs(30));
            }
        }
    }

    pub fn interrupted(&mut self, now: Instant) {
        for tab in self.tabs.values_mut() {
            if std::mem::take(&mut tab.settle_in_flight) && tab.dirty_since.is_none() {
                tab.settle_due = Some(now + Duration::from_secs(30));
            }
        }
    }

    pub fn key(&self, id: u64) -> Option<u64> {
        self.tabs.get(&id).map(|tab| tab.key)
    }

    pub fn contains_key(&self, key: u64) -> bool {
        self.tabs.values().any(|tab| tab.key == key)
    }

    /// Document replacement removes identity before another document uses its tab.
    pub fn forget(&mut self, id: u64) -> Option<u64> {
        self.tabs.remove(&id).map(|tab| tab.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_capture_and_continuously_dirty_revisions_have_deadlines() {
        let mut s = Schedule::default();
        let now = Instant::now();
        let cadence = Duration::from_secs(10);
        s.observe(5, true, now);
        assert!(s.next(&[5], now, cadence).is_some());
        s.captured(5, now);
        s.observe(5, true, now + Duration::from_secs(2));
        assert!(s
            .next(&[5], now + Duration::from_secs(9), cadence)
            .is_none());
        assert!(s
            .next(&[5], now + Duration::from_secs(10), cadence)
            .is_some());
    }

    #[test]
    fn dirty_tabs_are_served_round_robin_without_active_or_lease_filter() {
        let mut s = Schedule::default();
        let now = Instant::now();
        for id in [7, 9, 11] {
            s.observe(id, true, now);
        }
        let later = now + Duration::from_secs(2);
        for index in 0..3 {
            let (found, _) = s.next(&[7, 9, 11], later, Duration::from_secs(10)).unwrap();
            assert_eq!(found, index);
            s.captured([7, 9, 11][index], later);
        }
    }

    #[test]
    fn replacement_in_same_tab_rotates_storage_key() {
        let mut s = Schedule::default();
        let now = Instant::now();
        s.observe(1, true, now);
        let old = s.forget(1).unwrap();
        s.observe(1, true, now);
        assert_ne!(s.forget(1).unwrap(), old);
    }

    #[test]
    fn captured_dirty_to_clean_settles_once_and_retries_failed_baseline() {
        let mut schedule = Schedule::default();
        let now = Instant::now();
        let interval = Duration::from_secs(10);
        schedule.observe(1, true, now);
        let (_, key) = schedule.next(&[1], now, interval).unwrap();
        schedule.captured(1, now);
        let cancelled = now + Duration::from_secs(1);
        schedule.observe(1, false, cancelled);
        // The earlier dirty snapshot may finish after cancellation.
        schedule.completed(key, true, cancelled);
        assert!(schedule.next(&[1], cancelled, interval).is_some());
        schedule.captured(1, cancelled);
        schedule.completed(key, false, cancelled);
        assert!(schedule
            .next(&[1], cancelled + Duration::from_secs(1), interval)
            .is_none());
        let retry = cancelled + Duration::from_secs(30);
        assert!(schedule.next(&[1], retry, interval).is_some());
        schedule.captured(1, retry);
        schedule.completed(key, true, retry);
        assert!(schedule
            .next(&[1], retry + Duration::from_secs(600), interval)
            .is_none());
    }
}
