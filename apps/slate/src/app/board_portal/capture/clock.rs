//! Recording time starts with the first frame and excludes every pause.
use std::time::{Duration, Instant};

#[derive(Default)]
pub(super) struct RecordingClock {
    origin: Option<Instant>,
    paused_at: Option<Instant>,
    paused: Duration,
    accepts_from: Option<Instant>,
}

impl RecordingClock {
    pub(super) fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    pub(super) fn pause(&mut self, now: Instant) {
        self.paused_at.get_or_insert(now);
    }

    pub(super) fn resume(&mut self, now: Instant) {
        if let Some(paused_at) = self.paused_at.take() {
            if self.origin.is_some() {
                self.paused += now.saturating_duration_since(paused_at);
            }
            self.accepts_from = Some(now);
        }
    }

    pub(super) fn elapsed(&self, now: Instant) -> Duration {
        self.origin.map_or(Duration::ZERO, |origin| {
            self.paused_at
                .unwrap_or(now)
                .saturating_duration_since(origin)
                .saturating_sub(self.paused)
        })
    }

    pub(super) fn project_frame(&mut self, at: Instant) -> Option<Instant> {
        if self.is_paused() || self.accepts_from.is_some_and(|first| at < first) {
            return None;
        }
        let origin = *self.origin.get_or_insert(at);
        (at >= origin).then(|| origin + self.elapsed(at))
    }

    pub(super) fn stopped_at(&self, now: Instant) -> Instant {
        self.origin.unwrap_or(now) + self.elapsed(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_and_pauses_do_not_extend_the_recorded_timeline() {
        let origin = Instant::now();
        let mut clock = RecordingClock::default();
        assert_eq!(
            clock.elapsed(origin + Duration::from_secs(20)),
            Duration::ZERO
        );
        let first = origin + Duration::from_secs(20);
        assert_eq!(clock.project_frame(first), Some(first));
        clock.pause(first + Duration::from_secs(3));
        assert_eq!(
            clock.elapsed(first + Duration::from_secs(100)),
            Duration::from_secs(3)
        );
        assert!(clock
            .project_frame(first + Duration::from_secs(4))
            .is_none());
        clock.resume(first + Duration::from_secs(13));
        assert!(clock
            .project_frame(first + Duration::from_secs(12))
            .is_none());
        assert_eq!(
            clock.project_frame(first + Duration::from_secs(15)),
            Some(first + Duration::from_secs(5))
        );
        clock.pause(first + Duration::from_secs(16));
        assert_eq!(
            clock.stopped_at(first + Duration::from_secs(90)),
            first + Duration::from_secs(6)
        );
        clock.resume(first + Duration::from_secs(26));
        assert_eq!(
            clock.elapsed(first + Duration::from_secs(27)),
            Duration::from_secs(7)
        );
    }

    #[test]
    fn pausing_before_the_first_frame_keeps_zero_duration() {
        let origin = Instant::now();
        let mut clock = RecordingClock::default();
        clock.pause(origin);
        clock.resume(origin + Duration::from_secs(30));
        let first = origin + Duration::from_secs(31);
        assert_eq!(clock.project_frame(first), Some(first));
        assert_eq!(clock.elapsed(first), Duration::ZERO);
    }
}
