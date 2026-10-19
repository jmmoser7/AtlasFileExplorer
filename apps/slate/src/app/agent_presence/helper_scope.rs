use super::*;

pub(super) const TASK_LIMIT_SECS: u64 = 300;
pub(super) const RETAINED_RESULT_MAX_BYTES: usize = 65_536;

fn retain_public_text(source: &str) -> (String, usize, bool) {
    let total_bytes = source.len();
    let truncated = total_bytes > RETAINED_RESULT_MAX_BYTES;
    let mut end = total_bytes.min(RETAINED_RESULT_MAX_BYTES);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    (source[..end].to_owned(), total_bytes, truncated)
}

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
    pub total_bytes: usize,
    pub truncated: bool,
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

impl SlateApp {
    pub(super) fn retire_completed_helper(&mut self, who: &str) -> Result<(), String> {
        let completed = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .is_some_and(|a| {
                a.parent.is_some()
                    && a.had_response
                    && !a.inflight
                    && a.work.is_none()
                    && a.read.is_none()
                    && a.portal.is_none()
                    && !self.presence.has_pending_for(a)
                    && a.launched.is_empty()
                    && a.helper_outcome
                        .as_ref()
                        .is_some_and(|o| o.outcome == "success")
            });
        if let Some(a) = self.presence.agents.iter().find(|a| a.profile.id == who) {
            if let Some(parent) = &a.parent {
                let expired = !completed && a.joined_at.elapsed().as_secs() > TASK_LIMIT_SECS;
                let outcome = if completed {
                    "success"
                } else if let Some(outcome) =
                    a.helper_outcome.as_ref().filter(|o| o.outcome != "success")
                {
                    outcome.outcome
                } else if expired {
                    "expired"
                } else {
                    "cancel"
                };
                let status = if outcome == "expired" {
                    "Helper reached its 300-second task limit"
                } else {
                    a.helper_outcome
                        .as_ref()
                        .map_or("Helper task stopped", |o| o.status.as_str())
                };
                let request_id = a
                    .helper_outcome
                    .as_ref()
                    .map_or(a.request.as_str(), |o| o.request_id.as_str());
                let source = self
                    .presence
                    .conversations
                    .get(&a.profile.id)
                    .as_ref()
                    .and_then(|s| {
                        s.turns
                            .iter()
                            .rev()
                            .find(|t| t.request == request_id && t.role == "assistant")
                    })
                    .map_or("", |turn| turn.text.as_str());
                let (result, total_bytes, truncated) = retain_public_text(source);
                if self.presence.helper_results.len() >= 32 {
                    self.presence.helper_results.remove(0);
                }
                self.presence.helper_results.push(HelperResult {
                    id: a.profile.id.clone(),
                    parent_id: parent.clone(),
                    request_id: request_id.into(),
                    name: a.profile.name.clone(),
                    result,
                    total_bytes,
                    truncated,
                    status: status.chars().take(2048).collect(),
                    outcome: outcome.into(),
                    expired,
                    retired_at: Instant::now(),
                });
            }
        }
        self.depart_presence(who, chrome::RANDOM_DEPARTURE, completed)
    }
}

#[cfg(test)]
mod tests {
    use super::{retain_public_text, RETAINED_RESULT_MAX_BYTES};

    #[test]
    fn retained_public_result_caps_at_utf8_boundary_and_keeps_original_byte_count() {
        let source = "🦀".repeat(RETAINED_RESULT_MAX_BYTES / 2 + 1);
        let (retained, total_bytes, truncated) = retain_public_text(&source);
        assert!(retained.len() <= RETAINED_RESULT_MAX_BYTES);
        assert!(retained.is_char_boundary(retained.len()));
        assert_eq!(total_bytes, source.len());
        assert!(truncated);
        assert_eq!(retained.len() + 2, total_bytes);
    }

    #[test]
    fn retained_public_result_keeps_complete_short_text() {
        let (retained, total_bytes, truncated) = retain_public_text("Hello 🦀");
        assert_eq!(retained, "Hello 🦀");
        assert_eq!(total_bytes, retained.len());
        assert!(!truncated);
    }
}
