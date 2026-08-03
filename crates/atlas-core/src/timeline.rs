//! Pure time-axis logic for the unified activity timeline.
//!
//! The renderer (`atlas-shell::timeline`) owns pixels; everything here is
//! UI-free so it tests on any platform (Art. I): the picked-interval set, the
//! granularity ladder that drives semantic zoom, the sorted timestamp index
//! the graph buckets against, and the two morph curves that carry the grid
//! from a GitHub 7×N block to a per-second strip.

use std::collections::BTreeMap;

use crate::types::{day_index, day_start, SECS_PER_DAY, SECS_PER_HOUR, SECS_PER_MINUTE};

/// Sunday on or before the given day index (GitHub row alignment).
pub fn sunday_on_or_before(day: i64) -> i64 {
    day - (day + 4).rem_euclid(7)
}

/// Saturday on or after the given day index.
pub fn saturday_on_or_after(day: i64) -> i64 {
    day + (6 - (day + 4).rem_euclid(7))
}

/// Weekday row for a day index, 0 = Sunday … 6 = Saturday.
pub fn weekday_row(day: i64) -> usize {
    (day + 4).rem_euclid(7) as usize
}

/// Hermite ease used by both morph curves: flat at each end, no corner.
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Normalized progress of `value` across `[from, to]`, eased. `from` may be
/// greater than `to` (the zoom curves run from wide spans down to narrow).
pub fn morph(value: f64, from: f64, to: f64) -> f32 {
    if (from - to).abs() < f64::EPSILON {
        return if value <= to { 1.0 } else { 0.0 };
    }
    let raw = ((from - value) / (from - to)) as f32;
    smoothstep(raw)
}

/// Bucket sizes the strip can quantize to, ascending. Every sub-day entry
/// divides 86 400 so buckets align to UTC midnight rather than drifting.
pub const GRAINS: [i64; 11] = [
    1,
    5,
    15,
    SECS_PER_MINUTE,
    5 * SECS_PER_MINUTE,
    15 * SECS_PER_MINUTE,
    SECS_PER_HOUR,
    6 * SECS_PER_HOUR,
    12 * SECS_PER_HOUR,
    SECS_PER_DAY,
    7 * SECS_PER_DAY,
];

/// Smallest grain whose buckets are at least `min_bucket_px` wide.
pub fn grain_for(visible_secs: f64, width_px: f32, min_bucket_px: f32) -> i64 {
    if width_px <= 0.0 || visible_secs <= 0.0 {
        return GRAINS[0];
    }
    let need = min_bucket_px as f64 * visible_secs / width_px as f64;
    GRAINS
        .iter()
        .copied()
        .find(|&g| g as f64 >= need)
        .unwrap_or(GRAINS[GRAINS.len() - 1])
}

/// Start of the bucket containing `t` at `grain`.
pub fn bucket_start(t: i64, grain: i64) -> i64 {
    if grain <= 0 {
        return t;
    }
    if grain == 7 * SECS_PER_DAY {
        return sunday_on_or_before(day_index(t)) * SECS_PER_DAY;
    }
    if grain == SECS_PER_DAY {
        return day_start(t);
    }
    t.div_euclid(grain) * grain
}

/// GitHub's five-step ramp position for a bucket count.
pub fn heat_level(count: u32, max: u32) -> u8 {
    if count == 0 {
        return 0;
    }
    if max <= 1 {
        return 1;
    }
    let ratio = count as f32 / max as f32;
    if ratio <= 0.25 {
        1
    } else if ratio <= 0.5 {
        2
    } else if ratio <= 0.75 {
        3
    } else {
        4
    }
}

/// A set of picked time intervals, kept normalized: disjoint, ascending, and
/// merged when they touch. Half-open (`[lo, hi)`) so adjacent buckets join
/// without overlapping, which is what makes Ctrl-click at one zoom level and
/// Ctrl-drag at another compose instead of fighting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimePicks {
    spans: BTreeMap<i64, i64>,
}

impl TimePicks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    pub fn clear(&mut self) {
        self.spans.clear();
    }

    /// Disjoint intervals in ascending order.
    pub fn intervals(&self) -> impl Iterator<Item = (i64, i64)> + '_ {
        self.spans.iter().map(|(&lo, &hi)| (lo, hi))
    }

    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// Outer bounds of everything picked.
    pub fn bounds(&self) -> Option<(i64, i64)> {
        let (&lo, _) = self.spans.iter().next()?;
        let (_, &hi) = self.spans.iter().next_back()?;
        Some((lo, hi))
    }

    pub fn contains(&self, t: i64) -> bool {
        self.spans
            .range(..=t)
            .next_back()
            .is_some_and(|(_, &hi)| t < hi)
    }

    /// True when any picked interval intersects `[lo, hi)`.
    pub fn overlaps(&self, lo: i64, hi: i64) -> bool {
        if hi <= lo {
            return false;
        }
        self.spans
            .range(..hi)
            .next_back()
            .is_some_and(|(_, &end)| end > lo)
    }

    /// True when a single picked interval covers all of `[lo, hi)`.
    pub fn covers(&self, lo: i64, hi: i64) -> bool {
        if hi <= lo {
            return false;
        }
        self.spans
            .range(..=lo)
            .next_back()
            .is_some_and(|(_, &end)| end >= hi)
    }

    pub fn insert(&mut self, lo: i64, hi: i64) {
        if hi <= lo {
            return;
        }
        let (mut lo, mut hi) = (lo, hi);
        // Absorb every interval that overlaps or merely touches the new one.
        let doomed: Vec<i64> = self
            .spans
            .range(..=hi)
            .filter(|(_, &end)| end >= lo)
            .map(|(&start, _)| start)
            .collect();
        for start in doomed {
            if let Some(end) = self.spans.remove(&start) {
                lo = lo.min(start);
                hi = hi.max(end);
            }
        }
        self.spans.insert(lo, hi);
    }

    pub fn remove(&mut self, lo: i64, hi: i64) {
        if hi <= lo {
            return;
        }
        let overlapping: Vec<(i64, i64)> = self
            .spans
            .range(..hi)
            .filter(|(_, &end)| end > lo)
            .map(|(&start, &end)| (start, end))
            .collect();
        for (start, end) in overlapping {
            self.spans.remove(&start);
            // Whatever of the old interval survives on either side.
            if start < lo {
                self.spans.insert(start, lo);
            }
            if end > hi {
                self.spans.insert(hi, end);
            }
        }
    }

    /// Ctrl-click semantics: a fully picked bucket is removed, anything else
    /// is added. Returns `true` when the bucket ended up picked.
    pub fn toggle(&mut self, lo: i64, hi: i64) -> bool {
        if self.covers(lo, hi) {
            self.remove(lo, hi);
            false
        } else {
            self.insert(lo, hi);
            true
        }
    }
}

/// Sorted file timestamps plus the per-grain maxima the color ramp needs.
///
/// Built once per data revision. Bucket counts come from binary search, so a
/// frame paints a few hundred buckets without touching the whole vector
/// (Art. II: no O(files) work in a paint path).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActivityIndex {
    stamps: Vec<i64>,
    max_by_grain: BTreeMap<i64, u32>,
}

impl ActivityIndex {
    /// Non-positive timestamps are dropped: a zero mtime is missing data, not
    /// activity in 1970.
    pub fn from_timestamps(timestamps: impl IntoIterator<Item = i64>) -> Self {
        let mut stamps: Vec<i64> = timestamps.into_iter().filter(|&t| t > 0).collect();
        stamps.sort_unstable();
        let mut index = Self {
            stamps,
            max_by_grain: BTreeMap::new(),
        };
        index.build_maxima();
        index
    }

    fn build_maxima(&mut self) {
        for grain in GRAINS {
            let mut max = 0u32;
            let mut run_bucket = i64::MIN;
            let mut run = 0u32;
            for &t in &self.stamps {
                let bucket = bucket_start(t, grain);
                if bucket == run_bucket {
                    run += 1;
                } else {
                    max = max.max(run);
                    run_bucket = bucket;
                    run = 1;
                }
            }
            self.max_by_grain.insert(grain, max.max(run));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stamps.is_empty()
    }

    pub fn total(&self) -> u32 {
        self.stamps.len() as u32
    }

    /// First and last timestamp, or `None` when there is no data.
    pub fn span(&self) -> Option<(i64, i64)> {
        Some((*self.stamps.first()?, *self.stamps.last()?))
    }

    /// Inclusive day-index range covering the data, Sunday/Saturday aligned so
    /// the grid layer starts and ends on whole weeks.
    pub fn week_aligned_days(&self) -> Option<(i64, i64)> {
        let (lo, hi) = self.span()?;
        Some((
            sunday_on_or_before(day_index(lo)),
            saturday_on_or_after(day_index(hi)),
        ))
    }

    /// Files timestamped in `[lo, hi)`.
    pub fn count_in(&self, lo: i64, hi: i64) -> u32 {
        if hi <= lo {
            return 0;
        }
        let start = self.stamps.partition_point(|&t| t < lo);
        let end = self.stamps.partition_point(|&t| t < hi);
        (end - start) as u32
    }

    /// Individual timestamps in `[lo, hi)` — the per-file dashes at deep zoom.
    /// Reuses the caller's buffer so panning does not allocate per frame.
    pub fn stamps_in(&self, lo: i64, hi: i64, out: &mut Vec<i64>) {
        out.clear();
        if hi <= lo {
            return;
        }
        let start = self.stamps.partition_point(|&t| t < lo);
        let end = self.stamps.partition_point(|&t| t < hi);
        out.extend_from_slice(&self.stamps[start..end]);
    }

    /// Busiest bucket at `grain`, for ramp normalization. Stable while
    /// panning, unlike a visible-window maximum.
    pub fn max_bucket(&self, grain: i64) -> u32 {
        self.max_by_grain.get(&grain).copied().unwrap_or_else(|| {
            // A grain outside the ladder: fall back to the nearest known one.
            self.max_by_grain
                .range(..=grain)
                .next_back()
                .map(|(_, &m)| m)
                .unwrap_or(0)
        })
    }

    pub fn count_on_day(&self, day: i64) -> u32 {
        let lo = day * SECS_PER_DAY;
        self.count_in(lo, lo + SECS_PER_DAY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_alignment_matches_github_rows() {
        // Epoch day 0 is a Thursday, so its week starts 4 days earlier.
        assert_eq!(sunday_on_or_before(0), -4);
        assert_eq!(saturday_on_or_after(0), 2);
        assert_eq!(weekday_row(0), 4);
        assert_eq!(weekday_row(-4), 0);
        assert_eq!(weekday_row(2), 6);
    }

    #[test]
    fn morph_runs_from_wide_span_to_narrow() {
        let (begin, full) = (31.0, 7.0);
        assert_eq!(morph(40.0, begin, full), 0.0);
        assert_eq!(morph(31.0, begin, full), 0.0);
        assert_eq!(morph(7.0, begin, full), 1.0);
        assert_eq!(morph(1.0, begin, full), 1.0);
        let mid = morph(19.0, begin, full);
        assert!((mid - 0.5).abs() < 0.01, "midpoint eased to {mid}");
    }

    #[test]
    fn grain_grows_with_the_visible_span() {
        // A day across 700 px at 8 px/bucket needs ≥ 987 s, so hourly wins.
        assert_eq!(grain_for(SECS_PER_DAY as f64, 700.0, 8.0), SECS_PER_HOUR);
        // Twice the rail resolves one step finer.
        assert_eq!(
            grain_for(SECS_PER_DAY as f64, 1400.0, 8.0),
            15 * SECS_PER_MINUTE
        );
        assert_eq!(
            grain_for(365.0 * SECS_PER_DAY as f64, 700.0, 8.0),
            7 * SECS_PER_DAY
        );
        assert_eq!(grain_for(60.0, 700.0, 8.0), 1);
    }

    #[test]
    fn buckets_align_to_midnight_and_sunday() {
        let noon = day_start(20_000) + 12 * SECS_PER_HOUR;
        assert_eq!(bucket_start(noon, SECS_PER_DAY), day_start(noon));
        assert_eq!(bucket_start(noon, SECS_PER_HOUR), noon);
        assert_eq!(
            bucket_start(noon + 61, SECS_PER_MINUTE),
            noon + SECS_PER_MINUTE
        );
        let week = bucket_start(noon, 7 * SECS_PER_DAY);
        assert_eq!(weekday_row(day_index(week)), 0, "week starts on Sunday");
    }

    #[test]
    fn picks_merge_touching_intervals() {
        let mut picks = TimePicks::new();
        picks.insert(0, 100);
        picks.insert(100, 200);
        assert_eq!(picks.len(), 1, "adjacent half-open spans merge");
        assert_eq!(picks.bounds(), Some((0, 200)));
        picks.insert(500, 600);
        assert_eq!(picks.len(), 2);
        assert_eq!(picks.bounds(), Some((0, 600)));
    }

    #[test]
    fn picks_split_when_a_middle_bucket_is_removed() {
        let mut picks = TimePicks::new();
        picks.insert(0, 300);
        picks.remove(100, 200);
        let spans: Vec<(i64, i64)> = picks.intervals().collect();
        assert_eq!(spans, vec![(0, 100), (200, 300)]);
        assert!(picks.contains(50));
        assert!(!picks.contains(150));
        assert!(picks.contains(250));
    }

    #[test]
    fn picks_are_half_open_at_the_edges() {
        let mut picks = TimePicks::new();
        picks.insert(10, 20);
        assert!(picks.contains(10));
        assert!(picks.contains(19));
        assert!(!picks.contains(20), "upper bound is exclusive");
        assert!(!picks.contains(9));
    }

    #[test]
    fn toggle_deselects_a_covered_bucket() {
        let mut picks = TimePicks::new();
        assert!(picks.toggle(0, 100), "first toggle picks");
        assert!(!picks.toggle(0, 100), "second toggle releases");
        assert!(picks.is_empty());
        // Toggling a sub-bucket of a wider pick carves a hole.
        picks.insert(0, 300);
        assert!(!picks.toggle(100, 200));
        assert_eq!(picks.len(), 2);
    }

    #[test]
    fn index_counts_buckets_by_binary_search() {
        let day = 20_000;
        let base = day * SECS_PER_DAY;
        let index =
            ActivityIndex::from_timestamps([base + 10, base + 20, base + SECS_PER_DAY + 5, 0, -7]);
        assert_eq!(index.total(), 3, "non-positive stamps are dropped");
        assert_eq!(index.count_on_day(day), 2);
        assert_eq!(index.count_on_day(day + 1), 1);
        assert_eq!(index.count_in(base, base + 15), 1);
        assert_eq!(index.span(), Some((base + 10, base + SECS_PER_DAY + 5)));
    }

    #[test]
    fn index_maxima_are_per_grain() {
        let base = 20_000 * SECS_PER_DAY;
        // Three files in one minute, spread across two days.
        let index = ActivityIndex::from_timestamps([base, base + 1, base + 2, base + SECS_PER_DAY]);
        assert_eq!(index.max_bucket(SECS_PER_DAY), 3);
        assert_eq!(index.max_bucket(SECS_PER_MINUTE), 3);
        assert_eq!(index.max_bucket(1), 1, "one file per second at most");
    }

    #[test]
    fn index_stamps_in_window_reuses_the_buffer() {
        let base = 20_000 * SECS_PER_DAY;
        let index = ActivityIndex::from_timestamps([base, base + 5, base + 900]);
        let mut out = vec![999; 8];
        index.stamps_in(base, base + 10, &mut out);
        assert_eq!(out, vec![base, base + 5]);
        index.stamps_in(base + 10_000, base + 20_000, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn empty_index_is_honest() {
        let index = ActivityIndex::from_timestamps([]);
        assert!(index.is_empty());
        assert_eq!(index.span(), None);
        assert_eq!(index.week_aligned_days(), None);
        assert_eq!(index.max_bucket(SECS_PER_DAY), 0);
    }

    #[test]
    fn heat_level_spans_the_ramp() {
        assert_eq!(heat_level(0, 10), 0);
        assert_eq!(heat_level(1, 10), 1);
        assert_eq!(heat_level(5, 10), 2);
        assert_eq!(heat_level(7, 10), 3);
        assert_eq!(heat_level(10, 10), 4);
    }
}
