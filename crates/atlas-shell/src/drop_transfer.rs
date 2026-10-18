//! Shared visual motion for a picture absorbed into a canvas target.
//!
//! Callers own the drop transaction and its effects. Advance this derived
//! progress once per frame, then apply `visual` only to paint copies, in
//! either screen or world coordinates. Leaving a target reverses the same
//! motion without changing the document.

use std::time::Duration;

use eframe::egui::{Pos2, Rect};

/// Duration of a complete absorption or return.
pub const DURATION: Duration = Duration::from_millis(220);
const MIN_SCALE: f32 = 0.1;

/// Advance a reversible transfer without restarting it when direction changes.
pub fn advance(progress: f32, elapsed: Duration, absorbing: bool) -> f32 {
    let step = elapsed.as_secs_f32() / DURATION.as_secs_f32();
    (progress + if absorbing { step } else { -step }).clamp(0.0, 1.0)
}

/// Rectangle and opacity at `progress` (0 = original, 1 = absorbed).
///
/// `from` and `into` must use the same coordinate system. Size, center,
/// and opacity share one smoothstep so every drop target has the same feel.
pub fn visual(from: Rect, into: Pos2, progress: f32) -> (Rect, f32) {
    let t = progress.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    let size = from.size() * (1.0 - (1.0 - MIN_SCALE) * eased);
    (
        Rect::from_center_size(from.center().lerp(into, eased), size),
        1.0 - eased,
    )
}

#[cfg(test)]
mod tests {
    use eframe::egui::Vec2;

    use super::*;

    #[test]
    fn absorption_and_return_follow_the_same_motion() {
        let from = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 60.0));
        let into = Pos2::new(300.0, 200.0);
        assert_eq!(visual(from, into, 0.0), (from, 1.0));
        let (end, alpha) = visual(from, into, 1.0);
        assert_eq!(end.center(), into);
        assert!((end.width() - 10.0).abs() < 1e-3);
        assert!((end.height() - 6.0).abs() < 1e-3);
        assert_eq!(alpha, 0.0);

        let halfway = advance(0.0, DURATION / 2, true);
        assert_eq!(halfway, 0.5);
        assert_eq!(advance(halfway, DURATION / 2, false), 0.0);
        assert_eq!(advance(halfway, DURATION, true), 1.0);
        assert_eq!(advance(halfway, DURATION, false), 0.0);
    }
}
