use std::time::{Duration, Instant};

/// Decoder/preroll work is outside presentation time. Each seek/loop arms a
/// new clock, anchored only when its first eligible frame is ready to show.
pub(super) struct FrameClock {
    anchor: Option<(Instant, f32)>,
    last_frame: Option<Instant>,
}
impl FrameClock {
    pub fn new() -> Self {
        Self {
            anchor: None,
            last_frame: None,
        }
    }
    pub fn reset(&mut self) {
        self.anchor = None;
        self.last_frame = None;
    }
    pub fn media_now(&self, requested: f32, now: Instant) -> f32 {
        self.anchor.map_or(requested, |(wall, media)| {
            media + now.saturating_duration_since(wall).as_secs_f32()
        })
    }
    pub fn due(&mut self, time: f32, now: Instant) -> Instant {
        let (wall, media) = *self.anchor.get_or_insert((now, time));
        wall + Duration::from_secs_f32((time - media).max(0.0))
    }
    /// Skip expensive pixel copies while catching up, but keep delivering advancing pictures
    /// when this decoder cannot sustain the source frame rate.
    pub fn skip_late_frame(&self, time: f32, now: Instant) -> bool {
        let max_lag = Duration::from_millis(120);
        self.last_frame
            .is_some_and(|last| now.saturating_duration_since(last) < max_lag)
            && time + max_lag.as_secs_f32() < self.media_now(time, now)
    }

    pub fn frame_sent(&mut self, now: Instant) {
        self.last_frame = Some(now);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overload_can_skip_late_frames_without_starving_the_viewport() {
        let first = Instant::now();
        let mut clock = FrameClock::new();
        clock.due(0.0, first);
        clock.frame_sent(first);
        let stalled = first + Duration::from_secs(1);
        assert!(!clock.skip_late_frame(0.033, stalled));
        clock.frame_sent(stalled);
        assert!(clock.skip_late_frame(0.066, stalled + Duration::from_millis(20)));
        assert!(!clock.skip_late_frame(0.099, stalled + Duration::from_millis(130)));
        assert!(!clock.skip_late_frame(1.05, stalled + Duration::from_millis(60)));
        clock.reset();
        assert!(!clock.skip_late_frame(0.0, stalled));
    }
    #[test]
    fn cold_decoder_and_loop_startup_never_skip_the_first_frame() {
        let requested = Instant::now();
        let ready = requested + Duration::from_millis(850);
        let mut clock = FrameClock::new();
        assert_eq!(clock.media_now(0.0, ready), 0.0);
        assert_eq!(clock.due(0.0, ready), ready);
        assert_eq!(clock.due(0.125, ready), ready + Duration::from_millis(125));
        clock.reset();
        let replay_ready = ready + Duration::from_secs(2);
        assert_eq!(clock.media_now(3.0, replay_ready), 3.0);
        assert_eq!(clock.due(3.0, replay_ready), replay_ready);
        assert_eq!(
            clock.due(3.125, replay_ready),
            replay_ready + Duration::from_millis(125)
        );
    }
}
