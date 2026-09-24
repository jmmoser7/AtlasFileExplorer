//! Test hook for filesystem probes on paths the UI must not touch every frame.
//!
//! Production calls are a relaxed atomic increment. Frame-budget tests reset
//! the counter after warm-up and assert it stays put.

use std::sync::atomic::{AtomicU64, Ordering};

static PROBES: AtomicU64 = AtomicU64::new(0);

/// Record one filesystem probe (stat, attribute read, or directory create).
#[inline]
pub fn note() {
    PROBES.fetch_add(1, Ordering::Relaxed);
}

pub fn count() -> u64 {
    PROBES.load(Ordering::Relaxed)
}

pub fn reset() {
    PROBES.store(0, Ordering::Relaxed);
}
