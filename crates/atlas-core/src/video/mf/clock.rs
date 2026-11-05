use std::time::{Duration, Instant};

/// Decoder/preroll work is outside presentation time. Each seek/loop arms a
/// new clock, anchored only when its first eligible frame is ready to show.
pub(super) struct FrameClock {
    anchor: Option<(Instant, f32)>,
}
impl FrameClock {
    pub fn new() -> Self {
        Self { anchor: None }
    }
    pub fn reset(&mut self) {
        self.anchor = None;
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
}
#[cfg(test)]
mod tests {
    use super::*;
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
