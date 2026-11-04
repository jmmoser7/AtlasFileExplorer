//! Same QPC clock domain as WGC SystemRelativeTime, normalized to100ns.
//! Windows-only ignored-test support; no dependency or feature change.
use std::sync::OnceLock;

#[link(name = "kernel32")]
unsafe extern "system" {
    #[link_name = "QueryPerformanceCounter"]
    fn counter(value: *mut i64) -> i32;
    #[link_name = "QueryPerformanceFrequency"]
    fn frequency(value: *mut i64) -> i32;
}

pub(super) fn now_100ns() -> Option<i64> {
    static FREQUENCY: OnceLock<Option<i64>> = OnceLock::new();
    let frequency = (*FREQUENCY.get_or_init(|| {
        let mut value = 0;
        (unsafe { frequency(&mut value) } != 0 && value > 0).then_some(value)
    }))?;
    let mut ticks = 0;
    if unsafe { counter(&mut ticks) } == 0 {
        return None;
    }
    (ticks as i128 * 10_000_000 / frequency as i128)
        .try_into()
        .ok()
}
