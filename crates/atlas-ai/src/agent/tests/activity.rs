//! Bounded normalization of public activity at direct-file ingress.
use super::*;

#[test]
fn direct_file_sessions_normalize_only_recent_public_activity_and_preserve_valid_bounded_input() {
    let workspace = temp_workspace("activity_ingress");
    let path = workspace.join("session.json");
    let mut source = AgentSession {
        request: "current".into(),
        status: AgentStatus::Thinking,
        turns: vec![AgentTurn {
            role: "assistant".into(),
            text: "Visible answer".into(),
            at: 7,
        }],
        ..Default::default()
    };
    source.activity = (0..ACTIVITY_LIMIT + 8)
        .map(|n| AgentActivity {
            id: format!("event-{n}"),
            request: "current".into(),
            kind: AgentActivityKind::Commentary,
            text: "界".repeat(ACTIVITY_TEXT_LIMIT * 2),
            at: n as u64,
            semantics: Some(AgentActivitySemantics {
                action: AgentActivityAction::Reading,
                target: Some(ActivityTarget {
                    node_id: "\n".into(),
                }),
            }),
        })
        .collect();
    source.activity[9].request = "old-request".into();
    source.activity[10].text = "   ".into();
    source.activity[11].id = "x".repeat(257);
    source.activity[12].id = "malformed progress id".into();
    atomic_write_json(&path, &source).unwrap();
    let mut link = FileAgentLink::new();
    let bounded = link.tick_read_session_file(&path).unwrap();
    assert_eq!(bounded.activity.len(), ACTIVITY_LIMIT - 4);
    assert_eq!(bounded.activity.first().unwrap().id, "event-8");
    assert_eq!(
        bounded.activity.last().unwrap().id,
        format!("event-{}", ACTIVITY_LIMIT + 7)
    );
    assert!(bounded
        .activity
        .iter()
        .all(|event| event.request == "current"
            && atlas_agent::collaborator::valid_id(&event.id)
            && event.text.chars().count() == ACTIVITY_TEXT_LIMIT
            && event.semantics.as_ref().unwrap().target.is_none()));
    assert_eq!(
        bounded.turns, source.turns,
        "activity normalization cannot rewrite history"
    );
    assert_eq!(bounded.status, source.status);
    // Already-valid bounded events retain their full payload and exact ordering.
    let valid = workspace.join("valid-session.json");
    atomic_write_json(&valid, &bounded).unwrap();
    let mut link = FileAgentLink::new();
    assert_eq!(link.tick_read_session_file(&valid), Some(bounded));
    std::fs::remove_dir_all(workspace).unwrap();
}
