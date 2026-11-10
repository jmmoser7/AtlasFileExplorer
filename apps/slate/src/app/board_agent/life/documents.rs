//! Document-addressed access to the existing parked agent runtimes.
use super::*;
impl SlateApp {
    /// Document-addressed runtime lookup; NodeIds are local to a workbook.
    pub(crate) fn agent_runtime_in(&self, doc: u64) -> Option<&AgentRuntime> {
        if self.agents.life.doc.unwrap_or(self.tab().id) == doc {
            Some(&self.agents)
        } else {
            self.agents.life.parked.get(&doc)
        }
    }

    pub(crate) fn poll_model_task_session_in(
        &mut self,
        doc: u64,
        node: NodeId,
        dir: &std::path::Path,
    ) -> Option<std::sync::Arc<atlas_ai::agent::AgentSession>> {
        self.agents.sources.poll(dir, None).or_else(|| {
            self.agent_runtime_in(doc)
                .and_then(|runtime| runtime.sessions.get(&node).cloned())
        })
    }
    pub(crate) fn stop_model_task_provider_in(
        &mut self,
        doc: u64,
        node: NodeId,
        request: &str,
        session: &str,
        reason: &str,
    ) {
        if self
            .agent_runtime_in(doc)
            .and_then(|runtime| runtime.request_of(node))
            != Some(request)
        {
            return;
        }
        if let Some(link) = self.agents.codex.get(session) {
            link.stop_with_reason(reason.to_owned());
        }
    }
}
