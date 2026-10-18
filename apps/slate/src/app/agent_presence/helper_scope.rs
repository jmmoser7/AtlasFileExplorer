use super::*;

pub(super) const TASK_LIMIT_SECS: u64 = 300;

pub(super) struct TaskOutcome {
    pub request_id: String,
    pub outcome: &'static str,
    pub status: String,
}
impl TaskOutcome {
    pub fn provider_failed(&self) -> bool {
        matches!(self.outcome, "error" | "offline")
    }
}
impl Collaborator {
    /// Existing task transitions record public facts; presentation controls do not.
    pub(super) fn note_helper_outcome(&mut self, outcome: &'static str, status: &str) {
        if self.parent.is_some() {
            self.helper_outcome = Some(TaskOutcome {
                request_id: self.request.clone(),
                outcome,
                status: status.chars().take(2048).collect(),
            });
        }
    }
}

/// A bounded public result captured before departure revokes the live task.
pub(super) struct HelperResult {
    pub id: String,
    pub parent_id: String,
    pub request_id: String,
    pub name: String,
    pub result: String,
    pub status: String,
    pub outcome: String,
    pub expired: bool,
    pub retired_at: Instant,
}

impl PresenceRuntime {
    /// Retain departed helpers only while their bounded public result is available.
    /// Ordinary chat and independent collaborator grants are never pruned here.
    pub(super) fn refresh_helper_scope(&mut self) {
        let live: std::collections::HashSet<_> = self
            .agents
            .iter()
            .filter(|a| a.parent.is_some())
            .map(|a| a.profile.id.clone())
            .chain(self.helper_results.iter().map(|r| r.id.clone()))
            .collect();
        for a in &mut self.agents {
            if let Some(g) = &mut a.grant {
                g.conversation_ids
                    .retain(|id| !self.helper_endpoints.contains(id) || live.contains(id));
            }
        }
        self.helper_endpoints = live;
    }
}
