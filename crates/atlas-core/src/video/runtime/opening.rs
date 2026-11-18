use super::{Duration, Instant, STALL_LIMIT};

/// Media Foundation can spend several seconds initializing a cold decoder
/// before returning any samples. Observe only this worker and only until its
/// first pixels. A worker that has entered native startup may wait on another
/// process, but that phase always has an absolute deadline.
pub(super) struct Opening {
    started: Instant,
    sampled: Instant,
    progress: Instant,
    cpu: Option<Duration>,
    admitted: bool,
}

impl Opening {
    pub fn new(now: Instant, cpu: Option<Duration>) -> Self {
        Self {
            started: now,
            sampled: now,
            progress: now,
            cpu,
            admitted: false,
        }
    }

    pub fn admit(&mut self) {
        self.admitted = true;
    }

    pub fn stalled(&mut self, now: Instant, cpu_time: impl FnOnce() -> Option<Duration>) -> bool {
        if now.duration_since(self.started) >= Duration::from_secs(30) {
            return true;
        }
        if self.admitted {
            // Opening a valid decoder may wait in an out-of-process Media
            // Foundation transform. Child CPU is not a progress indicator.
            return false;
        }
        if now.duration_since(self.sampled) >= Duration::from_secs(1) {
            self.sampled = now;
            let sample = cpu_time();
            if self
                .cpu
                .zip(sample)
                .is_some_and(|(prev, next)| next.saturating_sub(prev) >= Duration::from_millis(10))
            {
                self.progress = now;
            }
            self.cpu = sample;
        }
        now.duration_since(self.started) > STALL_LIMIT
            && now.duration_since(self.progress) >= Duration::from_secs(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_startup_can_wait_off_process_but_its_deadline_never_moves() {
        let start = Instant::now();
        let mut opening = Opening::new(start, None);
        opening.admit();
        assert!(!opening.stalled(start + Duration::from_secs(29), || panic!("no CPU polling")));
        opening.admit();
        assert!(opening.stalled(start + Duration::from_secs(30), || panic!("hard limit")));
    }

    #[test]
    fn cold_codec_receives_time_only_while_working_and_has_an_absolute_deadline() {
        let start = Instant::now();
        let mut opening = Opening::new(start, Some(Duration::ZERO));
        assert!(
            !opening.stalled(start + Duration::from_millis(999), || panic!(
                "between samples"
            ))
        );
        for seconds in 1..30 {
            assert!(!opening.stalled(start + Duration::from_secs(seconds), || {
                Some(Duration::from_millis(20 * seconds))
            }));
        }
        assert!(opening.stalled(start + Duration::from_secs(30), || panic!("hard deadline")));
    }

    #[test]
    fn a_parked_or_unobservable_decoder_retains_the_silence_deadline() {
        let start = Instant::now();
        for cpu in [None, Some(Duration::from_millis(20))] {
            // The loader spends CPU before even a parked decoder enters its
            // codec. Early startup work must not move the silence deadline.
            let mut opening = Opening::new(start, cpu.map(|_| Duration::ZERO));
            for seconds in 1..=10 {
                assert!(!opening.stalled(start + Duration::from_secs(seconds), || cpu));
            }
            assert!(opening.stalled(start + Duration::from_millis(10001), || cpu));
        }
    }
}
