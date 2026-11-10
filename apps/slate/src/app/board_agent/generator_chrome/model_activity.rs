//! Default progress disclosure belongs to one target/request, never Board focus.
use super::*;

impl SlateApp {
    fn model_activity_disclosure(&self, id: NodeId) -> std::cell::RefMut<'_, (String, bool)> {
        let key = (self.tab().id, id);
        let request = self.agents.request_of(id).unwrap_or("").to_owned();
        let mut all = self.agents.flow.model_activity_disclosures.borrow_mut();
        let marker = all.entry(key).or_default();
        if marker.0 != request {
            *marker = (request, false);
        }
        std::cell::RefMut::map(all, |all| all.get_mut(&key).unwrap())
    }

    pub(super) fn note_model_activity_fold(&self, id: NodeId) {
        if self.model_node_info(id).is_some() {
            self.model_activity_disclosure(id).1 = true;
        }
    }

    pub(super) fn disclose_model_activity(&self, id: NodeId, state: &mut activity::ActivityState) {
        if self.model_node_info(id).is_none() {
            return;
        }
        let manual = self.model_activity_disclosure(id).1;
        if manual {
            return;
        }
        let request = self.agents.request_of(id);
        let public_work = self.agent_is_running(id)
            && self.agents.session(id).is_some_and(|session| {
                request == Some(session.request.as_str())
                    && session
                        .activity
                        .iter()
                        .any(|event| Some(event.request.as_str()) == request)
            });
        if public_work || self.agent_failure_reason(id).is_some() {
            state.fold = activity::CardFold::Partial;
        }
    }
}
