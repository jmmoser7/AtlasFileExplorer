//! Dedicated Board pan input and the transient detail budget for camera motion.
use crate::app::{Camera, SlateApp};
use eframe::egui;
use std::time::Duration;

const REFINE_DELAY: Duration = Duration::from_millis(100);

#[derive(Clone, Copy)]
struct PanCapture {
    button: egui::PointerButton,
    requires_hand: bool,
    position: egui::Pos2,
    moved: bool,
    pass: u64,
}

pub(super) struct DedicatedPan {
    id: egui::Id,
    capture: PanCapture,
    released: bool,
    pub(super) delta: egui::Vec2,
}

impl DedicatedPan {
    pub(super) fn read(
        resp: &egui::Response,
        tab: u64,
        space: bool,
        space_interrupted: bool,
        hand: bool,
    ) -> Option<Self> {
        use egui::PointerButton::{Middle, Primary};

        let primary_pans = space || hand;
        let ctx = &resp.ctx;
        let id = resp.id.with(("dedicated-board-pan", tab));
        let pass = ctx.cumulative_pass_nr();
        let previous = ctx.data_mut(|data| {
            let capture = data.get_temp::<PanCapture>(id);
            data.remove::<PanCapture>(id);
            capture
        });
        let clicked_middle = resp.clicked_by(Middle);
        let clicked_primary = primary_pans && resp.clicked_by(Primary);
        let from_global = ctx
            .layer_transform_from_global(resp.layer_id)
            .unwrap_or_default();
        ctx.input(|input| {
            if input.key_pressed(egui::Key::Escape) {
                return None;
            }
            let pointer = &input.pointer;
            let mut capture = previous
                .filter(|capture| match capture.button {
                    Primary if capture.requires_hand => hand,
                    Primary => !space_interrupted,
                    _ => true,
                })
                .filter(|capture| pass.saturating_sub(capture.pass) <= 1)
                .or_else(|| {
                    input.events.iter().find_map(|event| {
                        let egui::Event::PointerButton {
                            pos,
                            button,
                            pressed: true,
                            ..
                        } = *event
                        else {
                            return None;
                        };
                        let clicked = match button {
                            Middle => clicked_middle,
                            Primary if primary_pans => clicked_primary,
                            _ => return None,
                        };
                        let owns_press =
                            pointer.button_down(button) && resp.is_pointer_button_down_on();
                        ((owns_press || clicked) && resp.interact_rect.contains(from_global * pos))
                            .then_some(PanCapture {
                                button,
                                requires_hand: button == Primary && !space,
                                position: pos,
                                moved: false,
                                pass,
                            })
                    })
                })?;
            let release = input.events.iter().find_map(|event| match *event {
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    ..
                } if button == capture.button => Some(pos),
                _ => None,
            });
            if release.is_none() && !pointer.button_down(capture.button) {
                return None;
            }
            let position = release.or(pointer.latest_pos()).unwrap_or(capture.position);
            let delta = (position - capture.position) * from_global.scaling;
            capture.position = position;
            capture.moved |= delta != egui::Vec2::ZERO;
            capture.pass = pass;
            Some(Self {
                id,
                capture,
                released: release.is_some(),
                delta,
            })
        })
    }

    pub(super) fn primary(&self) -> bool {
        self.capture.button == egui::PointerButton::Primary
    }

    pub(super) fn active(&self) -> bool {
        !self.released || self.capture.moved
    }

    pub(super) fn retain(&self, ctx: &egui::Context) {
        if !self.released {
            ctx.data_mut(|data| data.insert_temp(self.id, self.capture));
        }
    }
}

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
