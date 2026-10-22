use super::*;

#[test]
fn steering_preserves_user_rows_and_streamed_answers_without_duplicates() {
    let mut session = AgentSession::default();
    session.turns.push(AgentTurn {
        role: "user".into(),
        text: "First task".into(),
        at: 0,
    });
    let mut stream = MessageStream::new(1, 1);
    stream.apply("item/started", &json!({"item":{"id":"answer-1","type":"agentMessage","phase":"final_answer","text":"First part"}}), &mut session);
    stream.push_user(&mut session, "First task");
    stream.apply(
        "item/agentMessage/delta",
        &json!({"itemId":"answer-1","delta":" completed"}),
        &mut session,
    );
    stream.apply("item/started", &json!({"item":{"id":"answer-2","type":"agentMessage","phase":"final_answer","text":"Follow-up answer"}}), &mut session);
    assert_eq!(
        session
            .turns
            .iter()
            .map(|t| (t.role.as_str(), t.text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("user", "First task"),
            ("assistant", "First part completed"),
            ("user", "First task"),
            ("assistant", "Follow-up answer")
        ]
    );
}

#[test]
fn activity_uses_observed_categories_and_never_guesses_targets() {
    use atlas_agent::AgentActivityAction;
    let mut session = AgentSession {
        request: "r".into(),
        ..Default::default()
    };
    for (kind, action) in [
        ("commandExecution", AgentActivityAction::Working),
        ("fileChange", AgentActivityAction::Editing),
        ("mcpToolCall", AgentActivityAction::Working),
        ("dynamicToolCall", AgentActivityAction::Working),
        ("webSearch", AgentActivityAction::Searching),
        ("imageGeneration", AgentActivityAction::CreatingImage),
        ("collabAgentToolCall", AgentActivityAction::Delegating),
    ] {
        for method in ["item/started", "item/completed"] {
            collect_activity(
                method,
                &json!({"item":{"id":kind,"type":kind,
                    "arguments":{"action":"reading","node_id":"private-target"},
                    "command":"read private-target", "output":"private-output"}}),
                &mut session,
            );
            let semantics = session.activity.last().unwrap().semantics.as_ref().unwrap();
            assert_eq!(semantics.action, action);
            assert_eq!(semantics.target, None);
        }
    }
    let count = session.activity.len();
    collect_activity(
        "item/started",
        &json!({"item":{"id":"unknown","type":"newTool"}}),
        &mut session,
    );
    assert_eq!(session.activity.len(), count);
    assert!(!serde_json::to_string(&session.activity)
        .unwrap()
        .contains("private-"));
}

#[test]
fn board_activity_names_only_the_observed_public_command() {
    let mut session = AgentSession::default();
    for (method, status) in [
        ("item/started", "inProgress"),
        ("item/completed", "completed"),
        ("item/completed", "failed"),
    ] {
        collect_activity(
            method,
            &json!({"item":{"id":"board-call","type":"dynamicToolCall",
                "tool":"slate_board_command","status":status,
                "arguments":{"command":"portal.web.read_text","targets":["private-target"],
                    "arguments":{"secret":"private-input"}},"output":"private-output"}}),
            &mut session,
        );
        assert!(session
            .activity
            .last()
            .unwrap()
            .text
            .ends_with("portal.web.read_text"));
    }
    collect_activity(
        "item/started",
        &json!({"item":{"id":"bad","type":"dynamicToolCall","tool":"slate_board_command",
            "arguments":{"command":"private-input\nnot-a-command"}}}),
        &mut session,
    );
    assert_eq!(
        session.activity.last().unwrap().text,
        "Using a connected tool"
    );
    assert!(!serde_json::to_string(&session.activity)
        .unwrap()
        .contains("private-"));
}

#[test]
fn public_progress_excludes_reasoning_and_tool_payloads() {
    let mut session = AgentSession {
        request: "r".into(),
        ..Default::default()
    };
    let mut stream = MessageStream::new(0, 7);
    stream.apply(
        "item/started",
        &item("note", json!("commentary"), "I am checking the layout."),
        &mut session,
    );
    stream.apply(
        "item/agentMessage/delta",
        &json!({"itemId":"note","delta":" Then I will render it."}),
        &mut session,
    );
    assert_eq!(session.activity.len(), 1);
    assert!(session.activity[0].text.ends_with("render it."));
    assert!(session.turns.is_empty());
    assert_eq!(
        session.activity[0].semantics.as_ref().unwrap().action,
        atlas_agent::AgentActivityAction::Chat
    );
    collect_activity(
        "item/completed",
        &json!({"item":{"id":"hidden","type":"reasoning","text":"private details"}}),
        &mut session,
    );
    collect_activity(
        "item/started",
        &json!({"item":{"id":"tool","type":"commandExecution","command":"SECRET=raw","aggregatedOutput":"secret output"}}),
        &mut session,
    );
    collect_activity(
        "item/completed",
        &json!({"item":{"id":"tool","type":"commandExecution","exitCode":1,"aggregatedOutput":"secret output"}}),
        &mut session,
    );
    assert_eq!(session.activity.len(), 3);
    let serialized = serde_json::to_string(&session.activity).unwrap();
    assert!(!serialized.contains("SECRET"));
    assert!(!serialized.contains("private details"));
    assert!(!serialized.contains("secret output"));
    assert_eq!(session.activity[2].text, "Tool reported a failure");
    assert_eq!(
        session.activity[2].semantics.as_ref().unwrap().action,
        atlas_agent::AgentActivityAction::Error
    );
}

fn item(id: &str, phase: Value, text: &str) -> Value {
    json!({"item":{"type":"agentMessage","id":id,"phase":phase,"text":text}})
}

#[test]
fn commentary_streams_as_activity_and_final_is_the_only_answer() {
    let mut session = session_from_thread(&json!({"id":"thread"}));
    session.status = AgentStatus::Thinking;
    let mut messages = MessageStream::new(0, 0);
    messages.apply(
        "item/started",
        &item("progress", json!("commentary"), ""),
        &mut session,
    );
    messages.apply(
        "item/agentMessage/delta",
        &json!({"itemId":"progress","delta":"Checking."}),
        &mut session,
    );
    messages.apply(
        "item/completed",
        &item("progress", json!("commentary"), "Checking."),
        &mut session,
    );
    messages.apply(
        "item/completed",
        &json!({"item":{"type":"agentMessage","id":"progress","text":"Checking."}}),
        &mut session,
    );
    assert!(session.turns.is_empty());
    assert_eq!(session.status, AgentStatus::Thinking);
    let prefix = "界".repeat(atlas_agent::ACTIVITY_TEXT_LIMIT);
    messages.apply(
        "item/completed",
        &item("progress", json!("commentary"), &prefix),
        &mut session,
    );
    session.activity[0].at = 0;
    messages.apply(
        "item/agentMessage/delta",
        &json!({"itemId":"progress","delta":" More public text beyond the cap."}),
        &mut session,
    );
    assert_eq!(session.activity[0].text, prefix);
    assert!(
        session.activity[0].at > 0,
        "capped commentary still observes current wall time"
    );
    messages.apply(
        "item/started",
        &item("answer", json!("final_answer"), ""),
        &mut session,
    );
    messages.apply(
        "item/agentMessage/delta",
        &json!({"itemId":"answer","delta":"Hello"}),
        &mut session,
    );
    assert_eq!(session.turns[0].text, "Hello");
    messages.apply(
        "item/completed",
        &item("answer", json!("final_answer"), "Hello!"),
        &mut session,
    );
    assert_eq!(session.turns.len(), 1);
    assert_eq!(session.turns[0].text, "Hello!");
    assert_eq!(
        session.turns[0].at, 0,
        "transcript keeps the original turn timestamp"
    );
}

#[test]
fn late_phase_and_legacy_missing_phase_do_not_lose_answers() {
    let mut session = session_from_thread(&json!({"id":"thread"}));
    let mut messages = MessageStream::new(0, 0);
    messages.apply(
        "item/agentMessage/delta",
        &json!({"itemId":"late","delta":"Working"}),
        &mut session,
    );
    assert!(
        session.turns.is_empty(),
        "wait for item phase before publishing unknown deltas"
    );
    messages.apply(
        "item/completed",
        &item("late", json!("commentary"), "Working"),
        &mut session,
    );
    assert!(session.turns.is_empty());
    messages.apply(
        "item/completed",
        &item("legacy", Value::Null, "Old server answer"),
        &mut session,
    );
    assert_eq!(session.turns[0].text, "Old server answer");
    messages.apply(
        "item/completed",
        &item("partial", json!("partial_answer"), "Stable answer"),
        &mut session,
    );
    assert_eq!(session.turns.len(), 2);
}

#[test]
fn reloaded_history_excludes_commentary_but_keeps_legacy_and_final_answers() {
    let state = session_from_thread(&json!({"turns":[{"items":[
        {"type":"userMessage","content":[{"text":"Hi"}]},
        {"type":"agentMessage","phase":"commentary","text":"I'll reply"},
        {"type":"agentMessage","phase":"final_answer","text":"Hello"},
        {"type":"agentMessage","text":"Legacy"}
    ]}]}));
    assert_eq!(
        state
            .turns
            .iter()
            .map(|turn| turn.text.as_str())
            .collect::<Vec<_>>(),
        ["Hi", "Hello", "Legacy"]
    );
}
