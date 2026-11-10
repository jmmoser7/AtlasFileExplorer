//! Ephemeral request dispatch; no helper routing is persisted into durable models.
use super::*;
impl CodexLink {
    pub fn send(&self, request: AgentRequest) -> Result<(), String> {
        self.send_with_access_snapshot(request, None)
    }
    /// Keep the host-observed policy off request.json and retain it for this turn.
    pub fn send_with_access_snapshot(
        &self,
        request: AgentRequest,
        access: Option<crate::access::Snapshot>,
    ) -> Result<(), String> {
        self.send_with_host_tools(request, access, None)
    }
    /// A helper route is captured for this request, never cached on a provider.
    pub fn send_with_host_tools(
        &self,
        request: AgentRequest,
        access: Option<crate::access::Snapshot>,
        tools: Option<atlas_agent::collaborator::HostToolsLaunch>,
    ) -> Result<(), String> {
        self.send_with_helper(
            request,
            access,
            tools.map(atlas_agent::helper::HelperLaunch::Model),
        )
    }
    pub fn send_with_helper(
        &self,
        request: AgentRequest,
        access: Option<crate::access::Snapshot>,
        tools: Option<atlas_agent::helper::HelperLaunch>,
    ) -> Result<(), String> {
        request.validate_artifact_request()?;
        if let Some(tools) = &tools {
            tools.validate_for(&request)?;
        }
        if let Ok(mut reason) = self.cancel_reason.lock() {
            *reason = None;
        }
        self.cancel.store(false, Ordering::Relaxed);
        let mut current = self
            .current_request
            .lock()
            .map_err(|_| "Current request witness unavailable")?;
        let previous = std::mem::replace(&mut *current, request.id.clone());
        if self.tx.try_send((request, access, tools)).is_err() {
            *current = previous;
            return Err("The agent is busy. Wait or stop the current response.".into());
        }
        Ok(())
    }
}
