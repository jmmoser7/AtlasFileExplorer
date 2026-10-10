//! The one cache behind canvas text: entries bucketed by a hash, matched
//! exactly by the caller, and dropped once no recent pass has painted them.
//!
//! A board or a folder map can paint more labels in one frame than any fixed
//! entry count. A plain least-recently-used cap would then evict entries the
//! same frame still needs and reshape them every frame. Eviction therefore
//! keeps everything a recent pass used and runs only when the cache has
//! doubled since the last sweep, so its cost is amortized per insert.

use std::collections::HashMap;

/// Passes an entry may sit unused before a sweep may drop it.
const IDLE_PASSES: u64 = 2;

struct Entry<T> {
    value: T,
    used: u64,
    pass: u64,
}

pub(super) struct Lru<T> {
    buckets: HashMap<u64, Vec<Entry<T>>>,
    len: usize,
    clock: u64,
    builds: u64,
    soft_cap: usize,
    hard_cap: usize,
    next_sweep: usize,
}

impl<T> Lru<T> {
    /// Sweeps start above `soft_cap` entries. One pass that touches more than
    /// `hard_cap` keeps only its newest half.
    pub(super) fn new(soft_cap: usize, hard_cap: usize) -> Self {
        Self {
            buckets: HashMap::new(),
            len: 0,
            clock: 0,
            builds: 0,
            soft_cap,
            hard_cap: hard_cap.max(soft_cap),
            next_sweep: soft_cap,
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.len
    }

    /// Entries inserted since this cache was created.
    pub(super) fn builds(&self) -> u64 {
        self.builds
    }

    pub(super) fn get(&mut self, key: u64, pass: u64, matches: impl Fn(&T) -> bool) -> Option<&T> {
        self.clock = self.clock.wrapping_add(1);
        let used = self.clock;
        let entry = self
            .buckets
            .get_mut(&key)?
            .iter_mut()
            .find(|entry| matches(&entry.value))?;
        entry.used = used;
        entry.pass = pass;
        Some(&entry.value)
    }

    pub(super) fn insert(&mut self, key: u64, pass: u64, value: T) {
        self.builds += 1;
        self.clock = self.clock.wrapping_add(1);
        let used = self.clock;
        self.buckets
            .entry(key)
            .or_default()
            .push(Entry { value, used, pass });
        self.len += 1;
        if self.len > self.next_sweep {
            self.sweep(pass);
        }
    }

    fn sweep(&mut self, pass: u64) {
        self.retain(|entry| entry.pass.saturating_add(IDLE_PASSES) > pass);
        if self.len > self.hard_cap {
            let mut used: Vec<u64> = self.entries().map(|entry| entry.used).collect();
            let keep = self.hard_cap / 2;
            let cut = used.len() - keep.max(1);
            let (_, threshold, _) = used.select_nth_unstable(cut);
            let threshold = *threshold;
            self.retain(|entry| entry.used >= threshold);
        }
        self.next_sweep = (self.len * 2).clamp(self.soft_cap, self.hard_cap);
    }

    fn entries(&self) -> impl Iterator<Item = &Entry<T>> {
        self.buckets.values().flatten()
    }

    fn retain(&mut self, keep: impl Fn(&Entry<T>) -> bool) {
        self.buckets.retain(|_, slots| {
            slots.retain(&keep);
            !slots.is_empty()
        });
        self.len = self.buckets.values().map(Vec::len).sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busy_pass_keeps_everything_it_painted() {
        let mut lru = Lru::new(8, 1000);
        for key in 0..100 {
            lru.insert(key, 7, key);
        }
        assert_eq!(lru.len(), 100, "entries the current pass uses were dropped");
        for key in 0..100 {
            assert!(lru.get(key, 7, |v| *v == key).is_some());
        }
    }

    #[test]
    fn idle_entries_go_at_the_next_sweep() {
        let mut lru = Lru::new(8, 1000);
        for key in 0..20 {
            lru.insert(key, 1, key);
        }
        for key in 100..120 {
            lru.insert(key, 10, key);
        }
        assert!(lru.len() <= 20, "{} entries survived", lru.len());
        assert!(lru.get(119, 10, |v| *v == 119).is_some());
        assert!(lru.get(0, 10, |v| *v == 0).is_none());
    }

    #[test]
    fn one_pass_never_grows_past_the_hard_cap() {
        let mut lru = Lru::new(8, 64);
        for key in 0..1000 {
            lru.insert(key, 3, key);
            assert!(lru.len() <= 64);
        }
        assert!(
            lru.get(999, 3, |v| *v == 999).is_some(),
            "newest entry dropped"
        );
        assert_eq!(lru.builds(), 1000);
    }
}
