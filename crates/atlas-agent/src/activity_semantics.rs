//! Observable activity categories, independent of providers and renderers.
use crate::*;

pub const ACTIVITY_TARGET_ID_LIMIT: usize = 256;

/// Stable protocol IDs. Faces and props are data in assets/agent-activities.json.
/// A category describes an observed operation, never private reasoning or emotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentActivityAction {
    Idle,
    Resting,
    Listening,
    Reasoning,
    Reading,
    Searching,
    Writing,
    Editing,
    CreatingImage,
    EditingImage,
    Organizing,
    Moving,
    Resizing,
    Frames,
    Presentations,
    Wiring,
    Working,
    ToolWaiting,
    Approval,
    Question,
    Blocked,
    Error,
    Verifying,
    Success,
    Chat,
    Exchanging,
    Delegating,
    SubagentWork,
    Discovery,
    Cancel,
    Retire,
    Retired,
    Restore,
    Offline,
}

/// A stable board node ID explicitly supplied by a board adapter. No paths,
/// prompts or coordinates are inferred from a generic provider tool event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityTarget {
    pub node_id: String,
}

impl ActivityTarget {
    pub fn new(node_id: impl Into<String>) -> Option<Self> {
        let target = Self {
            node_id: node_id.into(),
        };
        target.valid().then_some(target)
    }

    fn valid(&self) -> bool {
        !self.node_id.trim().is_empty()
            && self.node_id.chars().count() <= ACTIVITY_TARGET_ID_LIMIT
            && !self.node_id.chars().any(char::is_control)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentActivitySemantics {
    pub action: AgentActivityAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ActivityTarget>,
}

impl AgentActivitySemantics {
    pub fn new(action: AgentActivityAction) -> Self {
        Self {
            action,
            target: None,
        }
    }

    pub(crate) fn normalized(mut self) -> Self {
        self.target = self.target.filter(ActivityTarget::valid);
        self
    }
}

/// Unknown future categories do not prevent an older app reading a session.
pub(crate) fn de_activity_semantics<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<AgentActivitySemantics>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value::<AgentActivitySemantics>(value)
        .ok()
        .map(AgentActivitySemantics::normalized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn action_ids_match_the_single_visual_catalog() {
        use AgentActivityAction::*;
        let actions = [
            Idle,
            Resting,
            Listening,
            Reasoning,
            Reading,
            Searching,
            Writing,
            Editing,
            CreatingImage,
            EditingImage,
            Organizing,
            Moving,
            Resizing,
            Frames,
            Presentations,
            Wiring,
            Working,
            ToolWaiting,
            Approval,
            Question,
            Blocked,
            Error,
            Verifying,
            Success,
            Chat,
            Exchanging,
            Delegating,
            SubagentWork,
            Discovery,
            Cancel,
            Retire,
            Retired,
            Restore,
            Offline,
        ];
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../assets/agent-activities.json")).unwrap();
        let ids: Vec<_> = catalog["activities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["id"].clone())
            .collect();
        let mut actual: Vec<_> = actions
            .iter()
            .map(|action| {
                serde_json::to_value(action)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        let mut expected: Vec<_> = ids
            .iter()
            .map(|id| id.as_str().unwrap().to_owned())
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }

    #[test]
    fn legacy_and_future_activity_remain_readable() {
        let legacy = json!({"id":"a", "request":"r", "kind":"commentary", "text":"Hello", "at":1});
        let activity: AgentActivity = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(activity.semantics, None);
        assert_eq!(serde_json::to_value(activity).unwrap(), legacy);
        let session: AgentSession =
            serde_json::from_value(json!({"activity":[legacy.clone()]})).unwrap();
        assert_eq!(session.activity[0].semantics, None);
        for semantics in [json!(null), json!({"action":"future-category"}), json!([])] {
            let mut row = legacy.clone();
            row["semantics"] = semantics;
            assert_eq!(
                serde_json::from_value::<AgentActivity>(row)
                    .unwrap()
                    .semantics,
                None
            );
        }
    }

    #[test]
    fn targets_are_explicit_bounded_ids_not_payloads() {
        assert!(ActivityTarget::new("node-42").is_some());
        for id in [" ".into(), "node\nprivate".into(), "界".repeat(257)] {
            assert!(ActivityTarget::new(id).is_none());
        }
        let row = json!({"id":"a", "request":"r", "kind":"tool_started", "text":"Working", "at":1,
            "semantics":{"action":"reading", "target":{"node_id":"\n", "private":"secret"}}});
        let activity: AgentActivity = serde_json::from_value(row).unwrap();
        assert_eq!(
            activity.semantics.unwrap(),
            AgentActivitySemantics::new(AgentActivityAction::Reading)
        );
    }

    #[test]
    fn in_memory_targets_are_validated_when_activity_is_recorded() {
        let mut session = AgentSession {
            request: "r".into(),
            ..Default::default()
        };
        session.record_activity(AgentActivity {
            id: "a".into(),
            request: "r".into(),
            kind: AgentActivityKind::ToolStarted,
            text: "Working".into(),
            at: 1,
            semantics: Some(AgentActivitySemantics {
                action: AgentActivityAction::Moving,
                target: Some(ActivityTarget {
                    node_id: "界".repeat(257),
                }),
            }),
        });
        assert_eq!(session.activity[0].semantics.as_ref().unwrap().target, None);
    }
}
