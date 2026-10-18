//! Public progress is distinct from model reasoning and replayable turns.
use crate::*;

pub const ACTIVITY_LIMIT: usize = 64;
pub const ACTIVITY_TEXT_LIMIT: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentActivityKind {
    Commentary,
    ToolStarted,
    ToolFinished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentActivity {
    pub id: String,
    pub request: String,
    pub kind: AgentActivityKind,
    pub text: String,
    pub at: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_activity_semantics"
    )]
    pub semantics: Option<AgentActivitySemantics>,
}

impl AgentSession {
    pub fn record_activity(&mut self, mut activity: AgentActivity) {
        if activity.request != self.request || activity.id.is_empty() {
            return;
        }
        activity.text = activity.text.chars().take(ACTIVITY_TEXT_LIMIT).collect();
        activity.semantics = activity.semantics.map(AgentActivitySemantics::normalized);
        if activity.text.trim().is_empty() {
            return;
        }
        if let Some(old) = self.activity.iter_mut().find(|a| a.id == activity.id) {
            *old = activity;
        } else {
            self.activity.push(activity);
        }
        let excess = self.activity.len().saturating_sub(ACTIVITY_LIMIT);
        self.activity.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn progress_is_bounded_request_scoped_and_never_history() {
        let mut session = AgentSession {
            request: "current".into(),
            ..Default::default()
        };
        let item = |id: String, request: &str| AgentActivity {
            id,
            request: request.into(),
            kind: AgentActivityKind::Commentary,
            text: "界".repeat(2000),
            at: 1,
            semantics: None,
        };
        session.record_activity(item("stale".into(), "previous"));
        assert!(session.activity.is_empty());
        for n in 0..100 {
            session.record_activity(item(n.to_string(), "current"));
        }
        assert_eq!(session.activity.len(), ACTIVITY_LIMIT);
        assert_eq!(session.activity[0].id, "36");
        assert_eq!(
            session.activity[0].text.chars().count(),
            ACTIVITY_TEXT_LIMIT
        );
        assert!(checkpoint(&session.turns, None).unwrap().is_empty());
        session.record_activity(item("99".into(), "current"));
        assert_eq!(session.activity.len(), ACTIVITY_LIMIT);
    }
}
