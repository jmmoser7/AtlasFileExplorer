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
        if activity.request != self.request || !crate::collaborator::valid_id(&activity.id) {
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

#[cfg(test)]
mod id_tests {
    use super::*;

    #[test]
    fn activity_ids_use_the_existing_bounded_grammar_without_rewriting_source_identity() {
        let request = request_id();
        let turns = vec![AgentTurn {
            role: "assistant".into(),
            text: "Keep this answer".into(),
            at: 7,
        }];
        let mut session = AgentSession {
            request: request.clone(),
            turns: turns.clone(),
            ..Default::default()
        };
        let event = |id: String, text: &str, at| AgentActivity {
            id,
            request: request.clone(),
            kind: AgentActivityKind::Commentary,
            text: text.into(),
            at,
            semantics: None,
        };
        // Actual adapter forms plus both supported stable-ID punctuation kinds.
        for id in [
            "commentary:7d3f25be-55c1-42d8-9968-7b0aad54d846".to_string(),
            "tool:call_123:start".into(),
            "tool:call_123:finish".into(),
            format!("{request}:activity:1"),
            "public-event-0".into(),
            "a.b_c-d:1".into(),
        ] {
            assert!(crate::collaborator::valid_id(&id));
            session.record_activity(event(id.clone(), "Observed public progress", 1));
            assert_eq!(session.activity.last().unwrap().id, id);
        }
        let boundary = "x".repeat(256);
        session.record_activity(event(boundary.clone(), "First observation", 1));
        assert_eq!(session.activity.last().unwrap().id, boundary);
        let accepted = session.clone();
        for invalid in [
            "x".repeat(257),
            String::new(),
            "two words".into(),
            "path/id".into(),
            "line\nbreak".into(),
            "界".into(),
        ] {
            assert!(!crate::collaborator::valid_id(&invalid));
            session.record_activity(event(invalid, "Rejected public progress", 99));
            assert_eq!(
                session, accepted,
                "reject progress without changing identity or transcript"
            );
        }
        let count = session.activity.len();
        session.record_activity(event(boundary.clone(), "A genuine coalesced update", 2));
        assert_eq!(session.activity.len(), count);
        let latest = session.activity.last().unwrap();
        assert_eq!(latest.id, boundary);
        assert_eq!(latest.text, "A genuine coalesced update");
        assert_eq!(latest.at, 2);
        assert_eq!(session.request, request);
        assert_eq!(session.turns, turns);
    }
}
