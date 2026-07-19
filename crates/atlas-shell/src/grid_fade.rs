//! Canvas dot-grid visibility — full strength while the user zooms or pans,
//! then fades out to a clean background.

const FADE_OUT_SECS: f64 = 0.85;

#[derive(Clone, Copy, Default, Debug)]
pub struct GridFade {
    last_bump: f64,
}

impl GridFade {
    pub fn bump(&mut self, time: f64) {
        self.last_bump = time;
    }

    /// Opacity in `[0, 1]`; decays quadratically after the last navigation input.
    pub fn alpha(&self, time: f64) -> f32 {
        if self.last_bump <= 0.0 {
            return 0.0;
        }
        let elapsed = time - self.last_bump;
        if elapsed >= FADE_OUT_SECS {
            return 0.0;
        }
        let t = (elapsed / FADE_OUT_SECS) as f32;
        1.0 - t * t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_hidden() {
        assert_eq!(GridFade::default().alpha(10.0), 0.0);
    }

    #[test]
    fn full_strength_immediately_after_bump() {
        let mut fade = GridFade::default();
        fade.bump(5.0);
        assert_eq!(fade.alpha(5.0), 1.0);
    }

    #[test]
    fn fades_to_zero() {
        let mut fade = GridFade::default();
        fade.bump(0.0);
        assert_eq!(fade.alpha(FADE_OUT_SECS + 0.01), 0.0);
    }
}
