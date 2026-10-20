use super::*;

#[test]
fn completed_reply_keeps_its_completion_time_after_the_next_request_starts() {
    let mut h = host();
    let who = add(&mut h);
    h.app.presence_send_chat(&who, "First question").unwrap();
    let first_request = h.app.presence.agents[0].request.clone();
    let mut thread = h.app.presence_conversation(&who).unwrap().clone();
    let mut first = AgentSession {
        request: first_request.clone(),
        status: atlas_agent::AgentStatus::Thinking,
        updated_at: 10,
        turns: vec![
            atlas_agent::AgentTurn {
                role: "user".into(),
                text: "First question".into(),
                at: 10,
            },
            atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "First answer".into(),
                at: 10,
            },
        ],
        ..Default::default()
    };
    assert!(thread.absorb(&h.app.presence.agents[0], &first));
    assert_eq!(
        thread
            .turns
            .iter()
            .find(|turn| turn.text == "First answer")
            .unwrap()
            .at,
        10
    );
    first.status = atlas_agent::AgentStatus::Idle;
    first.updated_at = 30;
    first.activity.push(atlas_agent::AgentActivity {
        id: "progress".into(),
        request: first_request,
        kind: atlas_agent::AgentActivityKind::ToolFinished,
        text: "Finished work".into(),
        at: 25,
        semantics: None,
    });
    assert!(thread.absorb(&h.app.presence.agents[0], &first));
    let first_reply = thread
        .turns
        .iter()
        .find(|turn| turn.role == "assistant")
        .unwrap();
    assert_eq!(first_reply.at, 30);
    assert!(!thread.absorb(&h.app.presence.agents[0], &first));

    let next_request = "next-request";
    thread.push(
        "user",
        "Second question",
        "You",
        Some("local"),
        next_request,
        "complete",
    );
    h.app.presence.agents[0].request = next_request.into();
    let next = AgentSession {
        request: next_request.into(),
        status: atlas_agent::AgentStatus::Thinking,
        turns: vec![
            atlas_agent::AgentTurn {
                role: "user".into(),
                text: "First question".into(),
                at: 10,
            },
            atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "First answer".into(),
                at: 10,
            },
            atlas_agent::AgentTurn {
                role: "user".into(),
                text: "Second question".into(),
                at: 40,
            },
            atlas_agent::AgentTurn {
                role: "assistant".into(),
                text: "Second answer".into(),
                at: 40,
            },
        ],
        ..Default::default()
    };
    assert!(thread.absorb(&h.app.presence.agents[0], &next));
    assert_eq!(
        thread
            .turns
            .iter()
            .find(|turn| turn.text == "First answer")
            .unwrap()
            .at,
        30
    );
}
