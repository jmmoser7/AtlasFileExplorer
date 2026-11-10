//! Public typed milestones and bounded protocol metadata, never message bodies.
use atlas_agent::{
    helper::HelperLaunch, AgentActivity, AgentActivityAction as Action, AgentActivityKind as Kind,
    AgentActivitySemantics, AgentSession,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
};

mod status;

pub(crate) struct Progress {
    status: status::Status,
    path: PathBuf,
    request: String,
    count: usize,
    tools: HashMap<String, String>,
    observed: HashSet<String>,
    phase: String,
}
impl Progress {
    pub(crate) fn new(dir: &Path, request: &str) -> Self {
        Self {
            status: status::Status::default(),
            path: dir.join(format!(
                "subscription-events-{:x}.jsonl",
                Sha256::digest(request.as_bytes())
            )),
            request: request.into(),
            count: 0,
            tools: HashMap::new(),
            observed: HashSet::new(),
            phase: String::new(),
        }
    }
    pub(crate) fn note(&mut self, category: &str, id: &str, subtype: &str, allowed: Option<bool>) {
        if self.count >= 256 {
            return;
        }
        self.count += 1;
        let safe_id = atlas_agent::collaborator::valid_id(id).then_some(id);
        let value = json!({"request":self.request,"at_unix_ms":now(),"category":category,"id":safe_id,"subtype":subtype,"allowed":allowed});
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{value}");
        }
    }
    pub(crate) fn observe_status(&mut self, message: &Value) -> Option<&'static str> {
        let observed = self.status.observe(message);
        if message["type"] == "system"
            || message["type"] == "rate_limit_event"
            || message["type"] == "tool_progress"
        {
            let summary = self.status.summary(&self.request);
            let _ = std::fs::write(
                self.path.with_extension("summary.json"),
                serde_json::to_vec_pretty(&summary).unwrap(),
            );
        }
        if let Some((fields, label)) = observed {
            if self.count < 256 {
                self.count += 1;
                let event = json!({"request":self.request,"at_unix_ms":now(),"category":"public_provider_status","details":fields});
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)
                {
                    let _ = writeln!(file, "{event}");
                }
            }
            label
        } else {
            None
        }
    }
    fn activity(
        &mut self,
        session: &mut AgentSession,
        id: &str,
        kind: Kind,
        text: &str,
        action: Action,
    ) {
        session.record_activity(AgentActivity {
            id: id.into(),
            request: self.request.clone(),
            kind,
            text: text.into(),
            at: now() / 1000,
            semantics: Some(AgentActivitySemantics::new(action)),
        });
    }
    pub(crate) fn connected(&mut self, session: &mut AgentSession) {
        self.note("provider_initialized", "", "connected", None);
        self.activity(
            session,
            "subscription:connected",
            Kind::Commentary,
            "Connected to the subscription provider",
            Action::Working,
        );
    }
    fn reasoning(&mut self, session: &mut AgentSession) {
        if self.phase == "reasoning" {
            return;
        }
        self.phase = "reasoning".into();
        self.note("reasoning_phase_started", "", "typed_phase_only", None);
        self.activity(
            session,
            &format!("subscription:reasoning:{}", self.count),
            Kind::Commentary,
            "Reasoning",
            Action::Reasoning,
        );
    }
    fn start(&mut self, session: &mut AgentSession, id: &str, name: &str) {
        if !atlas_agent::collaborator::valid_id(id) {
            return;
        }
        if self.tools.len() >= 128 && !self.tools.contains_key(id) {
            return;
        }
        self.phase = "tool".into();
        if self
            .tools
            .get(id)
            .is_some_and(|old| old == name || (old != "native_command" && name == "native_command"))
        {
            return;
        }
        self.tools.insert(id.into(), name.into());
        self.observed.insert(format!("start:{id}"));
        self.note("tool_use_received", id, name, None);
        let (text, action) = match name {
            "read_context" => ("Reading the submitted task context", Action::Reading),
            "read_file" => ("Reading a staged input file", Action::Reading),
            "write_file" => ("Writing a dashboard file", Action::Writing),
            "native_inspect" => ("Inspecting the native model", Action::Verifying),
            "native_capture" => ("Capturing the native model", Action::Reading),
            "native_generate" => ("Generating the native model", Action::Writing),
            _ => ("Working with the native model tools", Action::ToolWaiting),
        };
        self.activity(
            session,
            &format!("tool:{id}:start"),
            Kind::ToolStarted,
            text,
            action,
        );
    }
    fn finish(&mut self, session: &mut AgentSession, id: &str, failed: bool) {
        let Some(name) = self.tools.get(id).cloned() else {
            return;
        };
        if !self.observed.insert(format!("finish:{id}")) {
            return;
        }
        self.note("tool_result_received", id, &name, Some(!failed));
        self.phase = "tool_result".into();
        self.activity(
            session,
            &format!("tool:{id}:finish"),
            Kind::ToolFinished,
            if failed {
                "The tool reported a failure"
            } else {
                "Received the tool result"
            },
            if failed {
                Action::Error
            } else {
                Action::Verifying
            },
        );
    }
    pub(crate) fn claude(
        &mut self,
        message: &Value,
        tools: Option<&HelperLaunch>,
        session: &mut AgentSession,
    ) -> bool {
        let before = session.activity.clone();
        if let Some(label) = self.observe_status(message) {
            self.activity(
                session,
                "subscription:public-status",
                Kind::Commentary,
                label,
                if label == "Subscription usage is currently unavailable" {
                    Action::Blocked
                } else {
                    Action::ToolWaiting
                },
            );
        }
        if let Some(kind) = message["type"]
            .as_str()
            .filter(|kind| matches!(*kind, "user" | "assistant" | "result"))
        {
            self.note(
                "vendor_envelope_received",
                message["uuid"].as_str().unwrap_or_default(),
                kind,
                None,
            );
        }
        if message["type"] == "stream_event" {
            let event = &message["event"];
            if event["type"] == "content_block_start" {
                let block = &event["content_block"];
                if block["type"] == "thinking" {
                    self.reasoning(session);
                }
                if block["type"] == "tool_use" {
                    self.claude_start(block, tools, session);
                }
            }
        }
        if message["type"] == "assistant" {
            for block in message["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if block["type"] == "tool_use" {
                    self.claude_start(block, tools, session);
                }
            }
        }
        if message["type"] == "user" {
            for block in message["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if block["type"] == "tool_result" {
                    if let Some(id) = block["tool_use_id"].as_str() {
                        self.finish(session, id, block["is_error"] == true);
                    }
                }
            }
        }
        if message["type"] == "result" {
            self.note(
                "provider_terminal_received",
                "",
                if message["is_error"] == true {
                    "error"
                } else {
                    "result"
                },
                None,
            );
        }
        session.activity != before
    }
    fn claude_start(
        &mut self,
        block: &Value,
        tools: Option<&HelperLaunch>,
        session: &mut AgentSession,
    ) {
        let (Some(id), Some(name), Some(tools)) =
            (block["id"].as_str(), block["name"].as_str(), tools)
        else {
            return;
        };
        for (index, alias) in crate::host_tools::helper_tool_aliases(tools)
            .iter()
            .enumerate()
        {
            if name == format!("mcp__slate__{alias}") {
                self.start(
                    session,
                    id,
                    if index == 0 {
                        "read_context"
                    } else {
                        if matches!(tools, HelperLaunch::Artifact(_)) {
                            if index == 1 {
                                "read_file"
                            } else {
                                "write_file"
                            }
                        } else {
                            match block["input"]["command"].as_str() {
                                Some("board.model.connector.inspect") => "native_inspect",
                                Some("board.model.connector.capture") => "native_capture",
                                Some("board.model.connector.invoke")
                                    if block["input"]["arguments"]["command_id"]
                                        == "org.slate.rhino.headless.generate" =>
                                {
                                    "native_generate"
                                }
                                _ => "native_command",
                            }
                        }
                    },
                );
            }
        }
    }
    pub(crate) fn acp(
        &mut self,
        update: &Value,
        tools: Option<&HelperLaunch>,
        session: &mut AgentSession,
    ) -> bool {
        let before = session.activity.clone();
        if update["sessionUpdate"] == "agent_thought_chunk" {
            self.reasoning(session);
        }
        if matches!(
            update["sessionUpdate"].as_str(),
            Some("tool_call" | "tool_call_update")
        ) {
            if let Some(id) = update["toolCallId"].as_str() {
                if update["sessionUpdate"] == "tool_call" {
                    if let Some(tools) = tools {
                        let server = crate::tool_client::cursor_server_name(tools);
                        for (index, alias) in crate::host_tools::helper_tool_aliases(tools)
                            .iter()
                            .enumerate()
                        {
                            if update["title"] == format!("{server}-{alias}: {alias}") {
                                self.start(
                                    session,
                                    id,
                                    if index == 0 {
                                        "read_context"
                                    } else if matches!(tools, HelperLaunch::Artifact(_)) {
                                        if index == 1 {
                                            "read_file"
                                        } else {
                                            "write_file"
                                        }
                                    } else {
                                        "native_command"
                                    },
                                );
                            }
                        }
                    }
                }
                if matches!(update["status"].as_str(), Some("completed" | "failed")) {
                    self.finish(session, id, update["status"] == "failed");
                }
            }
        }
        session.activity != before
    }
}
pub(crate) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests;
