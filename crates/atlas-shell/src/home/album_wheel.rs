//! Discrete wheel gestures for image albums, independent of home rack motion.
use eframe::egui::{Event, MouseWheelUnit};

const GESTURE_IDLE_SECONDS: f64 = 0.20;

#[derive(Clone, Debug, Default)]
pub(super) struct AlbumWheel {
    last_point_time: Option<f64>,
    accumulated: f32,
    fired: bool,
}

impl AlbumWheel {
    pub(super) fn step(&mut self, now: f64, events: &[Event], threshold: f32) -> f32 {
        let mut step = 0.0;
        for event in events {
            let Event::MouseWheel { unit, delta, .. } = event else {
                continue;
            };
            let amount = -(delta.x + delta.y);
            if !amount.is_finite() || amount.abs() < f32::EPSILON {
                continue;
            }
            match unit {
                MouseWheelUnit::Line | MouseWheelUnit::Page => {
                    // Windows commonly reports three lines for one detent.
                    // Its amplitude must not jump over intermediate pictures.
                    step = amount.signum();
                    self.last_point_time = None;
                    self.accumulated = 0.0;
                    self.fired = false;
                }
                MouseWheelUnit::Point => {
                    let fresh = self
                        .last_point_time
                        .is_none_or(|last| now - last > GESTURE_IDLE_SECONDS);
                    let reversed =
                        self.accumulated != 0.0 && self.accumulated.signum() != amount.signum();
                    if fresh || reversed {
                        self.accumulated = 0.0;
                        self.fired = false;
                    }
                    self.last_point_time = Some(now);
                    self.accumulated += amount;
                    if !self.fired && self.accumulated.abs() >= threshold.max(1.0) {
                        step = self.accumulated.signum();
                        self.fired = true;
                    }
                }
            }
        }
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Modifiers, Vec2};
    fn wheel(unit: MouseWheelUnit, y: f32) -> Event {
        Event::MouseWheel {
            unit,
            delta: Vec2::new(0.0, y),
            modifiers: Modifiers::NONE,
        }
    }
    #[test]
    fn a_three_line_detent_advances_one_picture() {
        let mut state = AlbumWheel::default();
        assert_eq!(
            state.step(0.0, &[wheel(MouseWheelUnit::Line, -3.0)], 72.0),
            1.0
        );
        assert_eq!(state.step(0.01, &[], 72.0), 0.0);
        assert_eq!(
            state.step(0.02, &[wheel(MouseWheelUnit::Line, -3.0)], 72.0),
            1.0
        );
    }
    #[test]
    fn point_momentum_does_not_skip_a_second_picture() {
        let mut state = AlbumWheel::default();
        assert_eq!(
            state.step(0.0, &[wheel(MouseWheelUnit::Point, -40.0)], 72.0),
            0.0
        );
        assert_eq!(
            state.step(0.05, &[wheel(MouseWheelUnit::Point, -40.0)], 72.0),
            1.0
        );
        assert_eq!(
            state.step(0.10, &[wheel(MouseWheelUnit::Point, -180.0)], 72.0),
            0.0
        );
        assert_eq!(
            state.step(0.35, &[wheel(MouseWheelUnit::Point, -80.0)], 72.0),
            1.0
        );
    }
    #[test]
    fn reversing_trackpad_direction_can_return_to_the_previous_picture() {
        let mut state = AlbumWheel::default();
        assert_eq!(
            state.step(0.0, &[wheel(MouseWheelUnit::Point, -90.0)], 72.0),
            1.0
        );
        assert_eq!(
            state.step(0.05, &[wheel(MouseWheelUnit::Point, 90.0)], 72.0),
            -1.0
        );
    }
}
