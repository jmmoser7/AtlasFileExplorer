//! Test hook for filesystem probes on paths the UI must not touch every frame.
//!
//! Counts per thread: the rule is "nothing on the frame loop", and workers may
//! stat freely. Frame-budget tests reset the counter after warm-up and assert
//! it stays put on their own thread, so tests running in parallel don't mix.

use std::cell::Cell;

thread_local! {
    static PROBES: Cell<u64> = const { Cell::new(0) };
}

/// Record one filesystem probe (stat, attribute read, or directory create).
#[inline]
pub fn note() {
    PROBES.with(|p| p.set(p.get() + 1));
}

/// Probes recorded on the calling thread since the last [`reset`].
pub fn count() -> u64 {
    PROBES.with(Cell::get)
}

pub fn reset() {
    PROBES.with(|p| p.set(0));
}
