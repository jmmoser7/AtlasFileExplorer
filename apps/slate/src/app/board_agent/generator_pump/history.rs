//! Retire pending generation snapshots after a committed history cut.
use super::*;

impl SlateApp {
    /// A committed history cut retires only this target's pending snapshots.
    /// Undo restores the document, never restarts canceled inference.
    pub(crate) fn invalidate_queued_generator_history(&mut self, id: NodeId) {
        let queued: Vec<_> = self
            .agents
            .comfy_queue
            .get(&id)
            .into_iter()
            .flatten()
            .map(|entry| entry.request.id.clone())
            .collect();
        for request in queued {
            self.discard_generation_request(id, &request);
        }
        self.cancel_artifact_generation(id);
        self.cancel_preparing_agent_send(id);
        if let Some(request) = self.agents.requests.remove(&id) {
            self.agents.cancelled_generations.insert(id, request);
        }
        self.agents.awaiting.remove(&id);
        self.agents.capture_wait.remove(&id);
        self.agents
            .flow
            .text_answer_requests
            .retain(|(_, node, _), _| *node != id);
    }
}
