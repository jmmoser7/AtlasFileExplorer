//! Normalize provider items into user messages, answers, and observed artifacts.
use super::*;

#[derive(Default)]
struct Message {
    id: String,
    text: String,
    commentary: bool,
    known: bool,
}
enum StreamItem {
    Message(usize),
    User(AgentTurn),
}

/// Commentary belongs to the public activity log. Only answer
/// items enter conversation history or become the next chat-train reply.
pub(super) struct MessageStream {
    start: usize,
    at: u64,
    items: Vec<Message>,
    order: Vec<StreamItem>,
}

impl MessageStream {
    pub(super) fn new(start: usize, at: u64) -> Self {
        Self {
            start,
            at,
            items: Vec::new(),
            order: Vec::new(),
        }
    }

    pub(super) fn push_user(&mut self, session: &mut AgentSession, text: &str) {
        self.order.push(StreamItem::User(AgentTurn {
            role: "user".into(),
            text: text.into(),
            at: now(),
        }));
        self.rebuild(session);
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
                self.order.push(StreamItem::Message(self.items.len()));
                self.items.push(Message {
                    id: id.into(),
                    ..Default::default()
                });
                self.items.len() - 1
            });
        let message = &mut self.items[index];
        if is_item {
            message.known = true;
            // Older app-server builds omit phase. Preserve a known item's phase
            // across sparse updates; a newly discovered legacy item defaults to answer.
            if let Some(phase) = item["phase"].as_str() {
                message.commentary = phase == "commentary";
            }
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
                at: super::now(),
                semantics: Some(atlas_agent::AgentActivitySemantics::new(
                    atlas_agent::AgentActivityAction::Chat,
                )),
            });
        }
        self.rebuild(session);
    }
    fn rebuild(&self, session: &mut AgentSession) {
        session.turns.truncate(self.start);
        session
            .turns
            .extend(self.order.iter().filter_map(|item| match item {
                StreamItem::User(turn) => Some(turn.clone()),
                StreamItem::Message(index) => {
                    let message = &self.items[*index];
                    (message.known && !message.commentary).then(|| AgentTurn {
                        role: "assistant".into(),
                        text: message.text.clone(),
                        at: self.at,
                    })
                }
            }));
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
#[path = "session/tests.rs"]
mod tests;
