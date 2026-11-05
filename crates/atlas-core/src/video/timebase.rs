/// Native codec timestamps may start after zero (composition/edit offsets).
/// A native video clip measures time from its first non-preroll video frame.
/// This is video-only timing; no audio synchronization is implied.
#[derive(Clone, Copy, Debug)]
pub(super) struct Timeline {
    origin_hns: i64,
}
impl Timeline {
    pub fn from_first_frame(origin_hns: i64) -> Self {
        Self { origin_hns }
    }
    pub fn clip_time(self, native_hns: i64) -> f32 {
        ((native_hns as f64 - self.origin_hns as f64) / 10_000_000.0) as f32
    }
    pub fn seek_position(self, clip_seconds: f32) -> i64 {
        let seconds = if clip_seconds.is_finite() {
            clip_seconds.max(0.0)
        } else {
            0.0
        };
        // SetCurrentPosition is already in presentation time, unlike sample
        // composition timestamps. Adding the sample origin can exceed duration.
        (seconds as f64 * 10_000_000.0).min(i64::MAX as f64) as i64
    }
    pub fn origin_hns(self) -> i64 {
        self.origin_hns
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_origin_maps_zero_resume_trim_and_replay_consistently() {
        let timeline = Timeline::from_first_frame(2_500_000);
        assert_eq!(timeline.origin_hns(), 2_500_000);
        assert_eq!(timeline.clip_time(2_500_000), 0.0);
        assert_eq!(timeline.clip_time(3_750_000), 0.125);
        for seconds in [0.0, 0.125, 0.5, 0.75, 1.0] {
            let presentation = timeline.seek_position(seconds);
            assert_eq!(presentation, (seconds as f64 * 10_000_000.0) as i64);
            assert_eq!(
                timeline.clip_time(presentation + timeline.origin_hns()),
                seconds
            );
        }
        assert_eq!(timeline.seek_position(0.5), 5_000_000);
        assert_eq!(timeline.seek_position(f32::NAN), 0);
        assert_eq!(timeline.seek_position(0.0), 0);
        assert_eq!(timeline.clip_time(1_250_000), -0.125);
        let end_sample = timeline.seek_position(0.96);
        assert!(
            end_sample < 10_000_000,
            "last strip seek must stay within the1spresentation"
        );
    }
    #[test]
    fn zero_origin_is_identity_and_frame_intervals_are_preserved() {
        let zero = Timeline::from_first_frame(0);
        assert_eq!(zero.clip_time(zero.seek_position(0.5)), 0.5);
        let shifted = Timeline::from_first_frame(90_000_000);
        assert_eq!(
            shifted.clip_time(91_250_000) - shifted.clip_time(90_000_000),
            0.125
        );
        assert_eq!(
            shifted.clip_time(92_500_000) - shifted.clip_time(91_250_000),
            0.125
        );
    }
}
