//! Normalize provider items into user messages, answers, and observed artifacts.
use super::*;

#[derive(Default)]
struct Message {
    id: String,
    text: String,
    commentary: bool,
    known: bool,
}

/// Commentary belongs to the public activity log. Only answer
/// items enter conversation history or become the next chat-train reply.
pub(super) struct MessageStream {
    start: usize,
    at: u64,
    items: Vec<Message>,
}

impl MessageStream {
    pub(super) fn new(start: usize, at: u64) -> Self {
        Self {
            start,
            at,
            items: Vec::new(),
        }
    }

    pub(super) fn apply(&mut self, method: &str, params: &Value, session: &mut AgentSession) {
        let item = &params["item"];
        let is_item =
            matches!(method, "item/started" | "item/completed") && item["type"] == "agentMessage";
        if !is_item && method != "item/agentMessage/delta" {
            return;
        }
        let id = if is_item {
            &item["id"]
        } else {
            &params["itemId"]
        };
        let Some(id) = id.as_str() else { return };
        let index = self
            .items
            .iter()
            .position(|message| message.id == id)
            .unwrap_or_else(|| {
                self.items.push(Message {
                    id: id.into(),
                    ..Default::default()
                });
                self.items.len() - 1
            });
        let message = &mut self.items[index];
        if is_item {
            message.known = true;
            // Older app-server builds omit phase: preserve their answer text.
            message.commentary = item["phase"] == "commentary";
            if let Some(text) = item["text"].as_str() {
                message.text = text.into();
            }
        } else {
            message
                .text
                .push_str(params["delta"].as_str().unwrap_or_default());
        }
        if message.known && message.commentary {
            session.record_activity(atlas_agent::AgentActivity {
                id: format!("commentary:{}", message.id),
                request: session.request.clone(),
                kind: atlas_agent::AgentActivityKind::Commentary,
                text: message.text.clone(),
                at: self.at,
                semantics: Some(atlas_agent::AgentActivitySemantics::new(
                    atlas_agent::AgentActivityAction::Chat,
                )),
            });
        }
        session.turns.truncate(self.start);
        session.turns.extend(
            self.items
                .iter()
                .filter(|message| message.known && !message.commentary)
                .map(|message| AgentTurn {
                    role: "assistant".into(),
                    text: message.text.clone(),
                    at: self.at,
                }),
        );
    }
}

pub fn session_from_thread(thread: &Value) -> AgentSession {
    let mut session = AgentSession {
        activity: Vec::new(),
        usage: None,
        approval: None,
        conversation: thread["id"].as_str().unwrap_or_default().into(),
        artifacts: vec![],
        status: AgentStatus::Idle,
        provider: "codex".into(),
        turns: vec![],
        updated_at: thread["updatedAt"].as_u64().unwrap_or(0),
        bundle: Default::default(),
        request: String::new(),
    };
    for turn in thread["turns"].as_array().into_iter().flatten() {
        let mut items = Vec::new();
        for item in turn["items"].as_array().into_iter().flatten() {
            match item["type"].as_str().unwrap_or_default() {
                "userMessage" => {
                    let text = item["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|c| c["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    session.turns.push(AgentTurn {
                        role: "user".into(),
                        text: atlas_agent::display_prompt(&text).into(),
                        at: 0,
                    });
                }
                "agentMessage" if item["phase"] != "commentary" => session.turns.push(AgentTurn {
                    role: "assistant".into(),
                    text: item["text"].as_str().unwrap_or_default().into(),
                    at: 0,
                }),
                _ => items.push(item),
            }
        }
        let index = session.turns.len().saturating_sub(1);
        for item in items {
            collect_artifacts(
                &mut session,
                item,
                index,
                turn["id"].as_str().unwrap_or_default(),
            );
        }
    }
    session
}

pub(super) fn collect_activity(method: &str, params: &Value, session: &mut AgentSession) {
    use atlas_agent::{
        AgentActivity, AgentActivityAction, AgentActivityKind, AgentActivitySemantics,
    };
    if !matches!(method, "item/started" | "item/completed") {
        return;
    }
    let item = &params["item"];
    let completed = method == "item/completed";
    let (start, finish, action) = match item["type"].as_str().unwrap_or_default() {
        "commandExecution" => (
            "Running a command",
            "Command finished",
            AgentActivityAction::Working,
        ),
        "fileChange" => (
            "Editing generated files",
            "File edits finished",
            AgentActivityAction::Editing,
        ),
        "dynamicToolCall" if item["tool"] == "slate_board_read" => (
            "Reading the granted board scope",
            "Board read finished",
            AgentActivityAction::Reading,
        ),
        "mcpToolCall" | "dynamicToolCall" => (
            "Using a connected tool",
            "Connected tool finished",
            AgentActivityAction::Working,
        ),
        "webSearch" => (
            "Searching reference material",
            "Reference search finished",
            AgentActivityAction::Searching,
        ),
        "imageGeneration" => (
            "Generating an image",
            "Image generation finished",
            AgentActivityAction::CreatingImage,
        ),
        "collabAgentToolCall" => (
            "Working with another agent",
            "Agent task finished",
            AgentActivityAction::Delegating,
        ),
        _ => return,
    };
    let Some(id) = item["id"].as_str() else {
        return;
    };
    let failed = matches!(item["status"].as_str(), Some("failed" | "error"))
        || item["exitCode"].as_i64().is_some_and(|code| code != 0);
    let text = if completed && failed {
        "Tool reported a failure"
    } else if completed {
        finish
    } else {
        start
    };
    session.record_activity(AgentActivity {
        id: format!("tool:{id}:{}", if completed { "finish" } else { "start" }),
        request: session.request.clone(),
        kind: if completed {
            AgentActivityKind::ToolFinished
        } else {
            AgentActivityKind::ToolStarted
        },
        text: text.into(),
        at: super::now(),
        semantics: Some(AgentActivitySemantics::new(if completed && failed {
            AgentActivityAction::Error
        } else {
            action
        })),
    });
}

pub(super) fn collect_artifacts(
    session: &mut AgentSession,
    item: &Value,
    turn: usize,
    turn_id: &str,
) {
    let mut add = |kind, source: &str, title: &str| {
        session.record_artifact(AgentArtifact {
            id: format!(
                "{turn_id}:{}:{source}",
                item["id"].as_str().unwrap_or_default()
            ),
            turn,
            kind,
            source: source.into(),
            title: title.into(),
        });
    };
    match item["type"].as_str().unwrap_or_default() {
        "fileChange" if item["status"] == "completed" => {
            for change in item["changes"].as_array().into_iter().flatten() {
                let kind = match change.pointer("/kind/type").and_then(Value::as_str) {
                    Some("add") => ArtifactKind::Created,
                    Some("delete") => ArtifactKind::Deleted,
                    _ => ArtifactKind::Modified,
                };
                if let Some(path) = change["path"].as_str() {
                    add(kind, path, path);
                }
            }
        }
        "webSearch" => {
            if let Some(url) = item.pointer("/action/url").and_then(Value::as_str) {
                add(ArtifactKind::Web, url, url);
            }
            for source in item["sources"].as_array().into_iter().flatten() {
                if let Some(url) = source["url"].as_str() {
                    add(
                        ArtifactKind::Web,
                        url,
                        source["title"].as_str().unwrap_or(url),
                    );
                }
            }
        }
        "commandExecution" if item["exitCode"].as_i64() == Some(0) => {
            for action in item["commandActions"].as_array().into_iter().flatten() {
                if action["type"] == "read" {
                    if let Some(path) = action["path"].as_str() {
                        add(ArtifactKind::Read, path, path);
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut messages = MessageStream::new(0, 17);
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
        assert!(session.turns.is_empty());
        assert_eq!(session.status, AgentStatus::Thinking);
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
}
