#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SamplePlan {
    Cached,
    Forward,
    Seek,
}
pub(super) const FORWARD_LIMIT: usize = 64;
pub(super) const NEAR_TIME: f32 = 0.08;

pub(super) fn plan(last: Option<f32>, target: f32, rate: Option<f32>) -> SamplePlan {
    let Some(last) = last.filter(|last| last.is_finite()) else {
        return SamplePlan::Seek;
    };
    if !target.is_finite() {
        return SamplePlan::Seek;
    }
    let distance = target - last;
    if distance.abs() <= NEAR_TIME {
        return SamplePlan::Cached;
    }
    if distance < 0.0 {
        return SamplePlan::Seek;
    }
    let Some(rate) = rate.filter(|rate| rate.is_finite() && *rate > 0.0) else {
        return SamplePlan::Seek;
    };
    if (distance * rate).ceil() <= FORWARD_LIMIT as f32 {
        SamplePlan::Forward
    } else {
        SamplePlan::Seek
    }
}
pub(super) fn frame_rate(packed: u64) -> Option<f32> {
    let (numerator, denominator) = ((packed >> 32) as u32, packed as u32);
    if numerator == 0 || denominator == 0 {
        return None;
    }
    Some(numerator as f32 / denominator as f32)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_samples_reuse_or_advance_but_long_backward_and_unknown_gaps_seek() {
        assert_eq!(plan(None, 0.0, Some(60.0)), SamplePlan::Seek);
        assert_eq!(plan(Some(0.0), 0.05, None), SamplePlan::Cached);
        assert_eq!(plan(Some(0.1), 0.05, Some(60.0)), SamplePlan::Cached);
        assert_eq!(plan(Some(0.0), 0.1, Some(60.0)), SamplePlan::Forward);
        assert_eq!(plan(Some(0.0), 1.0, Some(64.0)), SamplePlan::Forward);
        assert_eq!(plan(Some(0.0), 1.1, Some(60.0)), SamplePlan::Seek);
        assert_eq!(plan(Some(1.0), 0.5, Some(60.0)), SamplePlan::Seek);
        assert_eq!(plan(Some(0.0), 0.5, None), SamplePlan::Seek);
        assert_eq!(plan(Some(0.0), 0.5, Some(0.0)), SamplePlan::Seek);
        assert_eq!(plan(Some(0.0), f32::NAN, Some(60.0)), SamplePlan::Seek);
    }
    #[test]
    fn frame_rate_comes_from_a_valid_media_ratio() {
        assert_eq!(frame_rate((60u64 << 32) | 1), Some(60.0));
        assert_eq!(frame_rate(60u64 << 32), None);
        assert_eq!(frame_rate(1), None);
    }
}
