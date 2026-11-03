//! Fence an offered child window to the explicit parent-focus intent that requested it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app::application_connector) struct LaunchToken {
    pub surface: String,
    pub generation: u64,
}

#[derive(Default)]
pub(in crate::app::application_connector) struct LaunchState {
    generation: u64,
    pending: Option<(LaunchToken, String)>,
}

impl LaunchState {
    pub(in crate::app::application_connector) fn begin(
        &mut self,
        surface: &str,
        worker: String,
    ) -> LaunchToken {
        self.generation = self.generation.wrapping_add(1);
        let token = LaunchToken {
            surface: surface.into(),
            generation: self.generation,
        };
        self.pending = Some((token.clone(), worker));
        token
    }

    pub(in crate::app::application_connector) fn cancel(&mut self) {
        self.pending = None;
    }

    pub(in crate::app::application_connector) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(in crate::app::application_connector) fn complete(
        &mut self,
        token: &LaunchToken,
        worker: &str,
        parent_focused: bool,
    ) -> bool {
        if !self
            .pending
            .as_ref()
            .is_some_and(|(pending, _)| pending == token)
        {
            return false;
        }
        let (_, expected_worker) = self.pending.take().unwrap();
        expected_worker == worker && parent_focused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_and_focus_loss_refuse_handoff() {
        let mut state = LaunchState::default();
        let token = state.begin("grasshopper", "worker-a".into());
        state.cancel();
        assert!(!state.is_pending());
        assert!(!state.complete(&token, "worker-a", true));
        let token = state.begin("grasshopper", "worker-a".into());
        assert!(!state.complete(&token, "worker-a", false));
        assert!(!state.is_pending());
    }

    #[test]
    fn changed_worker_consumes_only_its_matching_intent() {
        let mut state = LaunchState::default();
        let token = state.begin("grasshopper", "worker-a".into());
        assert!(!state.complete(&token, "worker-b", true));
        assert!(!state.is_pending());
    }

    #[test]
    fn stale_completion_preserves_newer_pending_intent() {
        let mut state = LaunchState::default();
        let old = state.begin("grasshopper", "worker-a".into());
        let current = state.begin("grasshopper", "worker-a".into());
        assert!(!state.complete(&old, "worker-a", true));
        assert!(state.is_pending());
        assert!(state.complete(&current, "worker-a", true));
    }

    #[test]
    fn successful_handoff_completes_once() {
        let mut state = LaunchState::default();
        let token = state.begin("grasshopper", "worker-a".into());
        assert!(state.is_pending());
        assert!(state.complete(&token, "worker-a", true));
        assert!(!state.complete(&token, "worker-a", true));
        assert!(!state.is_pending());
    }
}
