//! Transient detail budget shared by board painters and preview requests.
use crate::app::{Camera, SlateApp};
use eframe::egui;
use std::time::Duration;

const REFINE_DELAY: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(in crate::app) struct BoardMotion {
    previous: Option<Camera>,
    refine_at: Option<f64>,
    #[cfg(all(test, windows))]
    disabled_for_probe: bool,
}

impl BoardMotion {
    fn observe(&mut self, camera: Camera, now: f64) -> Option<Duration> {
        if self
            .previous
            .replace(camera)
            .is_some_and(|old| old.offset != camera.offset || old.z != camera.z)
        {
            self.refine_at = Some(now + REFINE_DELAY.as_secs_f64());
        }
        self.refine_at = self.refine_at.filter(|&deadline| deadline > now);
        self.refine_at
            .map(|deadline| Duration::from_secs_f64(deadline - now))
    }
}

impl SlateApp {
    pub(in crate::app) fn observe_board_camera(&mut self, ctx: &egui::Context, now: f64) {
        let tab = self.tab_mut();
        #[cfg(all(test, windows))]
        if tab.board_motion.disabled_for_probe {
            return;
        }
        if let Some(remaining) = tab.board_motion.observe(tab.cam, now) {
            // Even a final wheel notch must refine without another input event.
            ctx.request_repaint_after(remaining);
        }
    }

    pub(in crate::app) fn board_camera_in_motion(&self) -> bool {
        !self.at_home
            && self.presenting.is_none()
            && self.portal_chrome.maximized.is_none()
            && self.tab().board_motion.refine_at.is_some()
    }

    #[cfg(all(test, windows))]
    pub(in crate::app) fn disable_board_motion_detail_for_test(&mut self) {
        self.tab_mut().board_motion.disabled_for_probe = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;

    #[test]
    fn only_camera_changes_extend_the_motion_budget() {
        let mut motion = BoardMotion::default();
        let mut camera = Camera::default();
        assert!(motion.observe(camera, 1.0).is_none());
        camera.offset += Vec2::new(0.25, -0.5);
        assert!(motion.observe(camera, 1.01).is_some());
        assert!(motion.observe(camera, 1.07).is_some());
        camera.z *= 2.0;
        assert!(motion.observe(camera, 1.10).is_some());
        assert!(motion.observe(camera, 1.19).is_some());
        assert!(motion.observe(camera, 1.21).is_none());
        assert!(motion.observe(camera, 1.50).is_none());
    }
}
