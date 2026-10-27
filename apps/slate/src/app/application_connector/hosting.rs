//! One screen-space readability and focus policy for all borrowed application windows.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FocusRequest {
    pub epoch: u64,
    pub focused: bool,
}

#[derive(Default)]
pub(super) struct HostPresentation {
    readable: bool,
    epoch: u64,
    queued: Option<FocusRequest>,
    applied: bool,
    failed: bool,
    revealed: bool,
    release_request_pending: bool,
}
impl HostPresentation {
    pub fn update_readable(&mut self, full: Rect, visible: Rect) -> bool {
        let was_readable = self.readable;
        let (width, height, visible_width, visible_height) = if self.readable {
            (576.0, 320.0, 400.0, 260.0)
        } else {
            (640.0, 360.0, 480.0, 300.0)
        };
        self.readable = full.is_finite()
            && visible.is_finite()
            && full.width() >= width
            && full.height() >= height
            && visible.width() >= visible_width
            && visible.height() >= visible_height;
        if self.readable != was_readable {
            self.failed = false;
        }
        self.readable
    }
    pub fn begin_intent(&mut self) {
        self.failed = false;
        self.release_request_pending = false;
    }
    pub fn ready_for_child_launch(&self) -> bool {
        self.readable && self.revealed && self.applied && self.queued.is_none() && !self.failed
    }
    pub fn revealed(&mut self) {
        self.revealed = true;
    }
    pub fn take_revealed(&mut self) -> bool {
        std::mem::take(&mut self.revealed)
    }
    pub fn request(&self, focused: bool, force: bool) -> Option<FocusRequest> {
        if self
            .queued
            .is_some_and(|request| request.focused == focused)
            || (!force && self.queued.is_none() && self.applied == focused)
            || self.failed
        {
            return None;
        }
        Some(FocusRequest {
            epoch: self.epoch.wrapping_add(1),
            focused,
        })
    }
    pub fn queued(&mut self, request: FocusRequest) {
        self.epoch = request.epoch;
        self.queued = Some(request);
    }
    pub fn complete(&mut self, request: FocusRequest, success: bool) -> bool {
        if self.queued != Some(request) || self.epoch != request.epoch {
            return false;
        }
        self.queued = None;
        if success {
            self.applied = request.focused;
        }
        self.failed = !success;
        true
    }
    pub fn complete_for_release(
        &mut self,
        request: FocusRequest,
        success: bool,
        has_intent: bool,
    ) -> Option<bool> {
        self.complete(request, success)
            .then_some(success && !request.focused && !has_intent)
    }
    pub fn parked(&mut self) {
        self.readable = false;
        self.revealed = false;
        self.suspend_focus();
    }
    pub fn suspend_focus(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        self.queued = None;
        self.applied = false;
        self.failed = false;
    }
}

impl Session {
    pub(super) fn queue_hidden_release(&mut self) {
        self.host.failed = false;
        self.host.release_request_pending = true;
        self.retry_hidden_release();
    }
    pub(super) fn retry_hidden_release(&mut self) {
        if !self.host.release_request_pending {
            return;
        }
        if let Some(request) = self.host.request(false, true) {
            if self.tx.try_send(worker::Action::Focus(request)).is_ok() {
                self.host.queued(request);
                self.host.release_request_pending = false;
            }
        } else {
            // An identical hide is already admitted. Never replay a failed
            // native effect; only retry queue admission before dispatch.
            self.host.release_request_pending = false;
        }
    }
    pub(super) fn release_hidden_surface(&mut self) -> Result<(), String> {
        if let Some(mut surface) = self.surface.take() {
            if let Err(error) = surface.detach_hidden() {
                let _ = surface.park();
                self.surface = Some(surface);
                return Err(error.to_string());
            }
        }
        self.last_geometry = None;
        self.host.parked();
        Ok(())
    }
    pub(super) fn request_native_focus(&mut self, focused: bool, force: bool) {
        if let Some(request) = self.host.request(focused, force) {
            if self.tx.try_send(worker::Action::Focus(request)).is_ok() {
                self.host.queued(request);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rect(w: f32, h: f32) -> Rect {
        Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h))
    }
    #[test]
    fn tiny_cards_never_establish_control_scale_and_readability_has_hysteresis() {
        let mut state = HostPresentation::default();
        assert!(!state.update_readable(rect(94.0, 70.0), rect(94.0, 70.0)));
        assert!(state.update_readable(rect(800.0, 500.0), rect(800.0, 500.0)));
        assert!(state.update_readable(rect(600.0, 340.0), rect(600.0, 340.0)));
        assert!(!state.update_readable(rect(570.0, 340.0), rect(570.0, 340.0)));
        assert!(!state.update_readable(rect(600.0, 340.0), rect(600.0, 340.0)));
        assert!(!state.update_readable(rect(1000.0, 700.0), rect(90.0, 70.0)));
    }
    #[test]
    fn focus_requires_acknowledgement_and_late_ack_after_park_cannot_apply() {
        let mut state = HostPresentation::default();
        let first = state.request(true, false).unwrap();
        state.queued(first);
        assert!(!state.applied);
        assert!(state.request(true, false).is_none());
        state.parked();
        assert!(!state.complete(first, true));
        let second = state.request(true, false).unwrap();
        assert_ne!(first.epoch, second.epoch);
        state.queued(second);
        assert!(state.complete(second, true));
        assert!(state.applied);
    }
    #[test]
    fn cold_child_launch_requires_current_readable_placement_and_focus_ack() {
        let mut state = HostPresentation::default();
        state.update_readable(rect(800.0, 500.0), rect(800.0, 500.0));
        state.revealed();
        let show = state.request(true, false).unwrap();
        state.queued(show);
        assert!(!state.ready_for_child_launch());
        state.complete(show, true);
        assert!(state.ready_for_child_launch());
        state.take_revealed();
        assert!(!state.ready_for_child_launch());
        state.revealed();
        state.update_readable(rect(94.0, 70.0), rect(94.0, 70.0));
        assert!(!state.ready_for_child_launch());
    }
    #[test]
    fn parked_preview_is_a_transition_not_a_per_frame_request() {
        let mut state = HostPresentation::default();
        state.revealed();
        assert!(state.take_revealed());
        assert!(!state.take_revealed());
        let hide = state.request(false, true).unwrap();
        state.queued(hide);
        assert!(state.complete(hide, true));
        assert!(state.request(false, false).is_none());
    }
    #[test]
    fn hide_ack_cannot_release_a_reentered_or_newer_focus_owner() {
        let mut state = HostPresentation::default();
        let hide = state.request(false, true).unwrap();
        state.queued(hide);
        assert_eq!(state.complete_for_release(hide, true, true), Some(false));
        let hide = state.request(false, true).unwrap();
        state.queued(hide);
        let show = state.request(true, false).unwrap();
        state.queued(show);
        assert_eq!(state.complete_for_release(hide, true, false), None);
        assert!(state.complete(show, true));
        let hide = state.request(false, false).unwrap();
        state.queued(hide);
        assert_eq!(state.complete_for_release(hide, true, false), Some(true));
    }
    #[test]
    fn failed_hide_is_not_replayed_by_idle_frames() {
        let mut state = HostPresentation::default();
        let hide = state.request(false, true).unwrap();
        state.queued(hide);
        assert!(state.complete(hide, false));
        assert!(state.request(false, true).is_none());
        state.begin_intent();
        assert!(state.request(false, true).is_some());
    }
}
