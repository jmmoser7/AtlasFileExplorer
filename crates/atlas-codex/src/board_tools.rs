//! Dynamic board tools from the installed app-server schema (16 Oct 2026).
//! Provider work remains on its existing worker; Slate resolves the same typed calls.
use super::*;
use atlas_agent::collaborator::{BoardOperation, BoardReply};

mod image_transport;
mod thread_binding;

#[cfg(test)]
pub(super) fn tools() -> Value {
    codex_tools(false)
}
fn codex_tools(model_task: bool) -> Value {
    let mut tools = atlas_agent::collaborator::tools::tool_schemas(model_task);
    for tool in tools.as_array_mut().unwrap() {
        tool["type"] = json!("function");
    }
    tools
}

impl Client {
    pub(super) fn prepare_board_session(&mut self, request: &AgentRequest) -> Result<(), String> {
        if let Some(link) = &request.collaborator {
            link.validate().map_err(str::to_owned)?;
            if self.board_cloud_guard.is_none() {
                return Err(
                    "This board collaborator requires the host's cloud-safe file reader.".into(),
                );
            }
        }
        if request.collaborator.is_some()
            && (request.oneshot || request.image.is_some() || request.artifact.is_some())
        {
            return Err(
                "Board collaborators use ordinary chat requests, not one-shot generation modes."
                    .into(),
            );
        }
        if request.collaborator == self.board_session {
            return Ok(());
        }
        // Dynamic tools belong to thread/start, not turn/start or thread/resume.
        // A fresh membership receives fresh permissions and tools. The caller
        // replays only this chat's visible checkpoint into its replacement thread.
        let mut params = thread_params(&self.cwd, None);
        if request.collaborator.is_some() {
            params["dynamicTools"] = codex_tools(
                request
                    .collaborator
                    .as_ref()
                    .is_some_and(|link| link.is_model_task()),
            );
        }
        let response = self.rpc("thread/start", params)?;
        self.thread = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or("Codex did not return a collaborator conversation id")?
            .into();
        self.permissions = TurnPermissions::default();
        self.permissions.own = Permissions::from_thread_response(&response);
        thread_binding::persist(self.approval_dir.as_deref(), &self.thread)?;
        self.board_session = request.collaborator.clone();
        self.replay_visible_history |= request.collaborator.is_some();
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct PendingCalls {
    exchange: atlas_agent::collaborator::transport::PendingCalls,
}
impl PendingCalls {
    pub(super) fn start(
        &mut self,
        client: &mut Client,
        request: &AgentRequest,
        turn: &str,
        event: Value,
    ) -> Result<(), String> {
        let p = &event["params"];
        if p["threadId"] != client.thread || p["turnId"] != turn || self.exchange.len() >= 8 {
            return tool_result(
                client,
                &event["id"],
                false,
                json!({"error":"Inactive board turn or full tool queue."}),
            );
        }
        let Some(link) = &request.collaborator else {
            return client.decline(&event);
        };
        let result = (|| -> Result<(), String> {
            let operation = parse_operation(
                p["tool"].as_str().unwrap_or_default(),
                &p["arguments"],
                request.policy,
            )?;
            let root = client
                .approval_dir
                .as_ref()
                .ok_or("This collaborator has no board link.")?;
            self.exchange.start(
                event["id"].clone(),
                link.transport_dir(root),
                link.clone(),
                request.id.clone(),
                operation,
            )
        })();
        if let Err(error) = result {
            tool_result(client, &event["id"], false, json!({"error":error}))?;
        }
        Ok(())
    }
    pub(super) fn poll(
        &mut self,
        client: &mut Client,
        _request: &AgentRequest,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        for done in self.exchange.poll(cancel, client.board_cloud_guard) {
            tool_reply(client, &done.token, &done.call.operation, done.reply)?;
        }
        Ok(())
    }
}
use atlas_agent::collaborator::tools::parse_operation;
#[cfg(test)]
use atlas_agent::collaborator::transport::read_json;

fn tool_result(
    client: &mut Client,
    id: &Value,
    success: bool,
    result: Value,
) -> Result<(), String> {
    client.write(json!({"id":id,"result":{"contentItems":[{"type":"inputText", "text":serde_json::to_string(&result).unwrap_or_default()}],"success":success}}))
}

fn tool_reply(
    client: &mut Client,
    id: &Value,
    operation: &BoardOperation,
    reply: BoardReply,
) -> Result<(), String> {
    let items = match image_transport::content_items(
        client.board_image_workspace.as_deref(),
        client.board_cloud_guard,
        operation,
        reply.success,
        &reply.result,
    ) {
        Ok(items) => items,
        Err(error) => {
            return tool_result(
                client,
                id,
                false,
                json!({"error":error,"result":reply.result}),
            );
        }
    };
    client.write(json!({"id":id,"result":{"contentItems":items,"success":reply.success}}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_or_blocking_cloud_guard_refuses_protocol_reads() {
        let path = std::env::temp_dir().join(format!(
            "slate-board-guard-{}.json",
            atlas_agent::request_id()
        ));
        std::fs::write(&path, b"{\"ok\":true}").unwrap();
        assert!(read_json(&path, None).is_none());
        assert!(read_json(&path, Some(|_| true)).is_none());
        assert_eq!(read_json(&path, Some(|_| false)), Some(json!({"ok":true})));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn tool_schemas_and_calls_match_installed_protocol_and_typed_bridge() {
        let tools = tools();
        assert_eq!(tools.as_array().unwrap().len(), 3);
        assert!(tools
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["type"] == "function"));
        assert_eq!(
            parse_operation(
                "slate_board_read",
                &json!({"node_ids":["42"]}),
                TurnPolicy::Default
            )
            .unwrap(),
            BoardOperation::Read {
                node_ids: vec!["42".into()],
                offset: 0,
                limit: 32
            }
        );
        assert_eq!(
            parse_operation("slate_participants_read", &json!({}), TurnPolicy::Default).unwrap(),
            BoardOperation::ReadParticipants {
                participant_ids: vec![],
                offset: 0,
                limit: 8,
                text_cursor: None,
                text_limit: None,
                text_generation: None
            }
        );
        assert_eq!(
            parse_operation(
                "slate_participants_read",
                &json!({"participant_ids":["helper-1"],"offset":8,"text_cursor":4096,"text_limit":4096,"text_generation":"helper-request-1"}),
                TurnPolicy::Default
            )
            .unwrap(),
            BoardOperation::ReadParticipants {
                participant_ids: vec!["helper-1".into()],
                offset: 8,
                limit: 8,
                text_cursor: Some(4096),
                text_limit: Some(4096),
                text_generation: Some("helper-request-1".into())
            }
        );
        assert!(matches!(
            parse_operation(
                "slate_board_command",
                &json!({"command":"move","targets":[],"arguments":{}}),
                TurnPolicy::Default,
            )
            .unwrap(),
            BoardOperation::Invoke { .. }
        ));
        assert!(parse_operation("shell", &json!({}), TurnPolicy::Default).is_err());
        assert!(parse_operation(
            "slate_board_command",
            &json!({"command":"move","code":"x"}),
            TurnPolicy::Default
        )
        .is_err());
    }

    #[test]
    fn read_only_policy_accepts_participant_reads_and_refuses_board_invocations() {
        assert!(matches!(
            parse_operation("slate_participants_read", &json!({}), TurnPolicy::ReadOnly).unwrap(),
            BoardOperation::ReadParticipants { .. }
        ));
        let arguments = json!({"command":"move","targets":[],"arguments":{}});
        assert_eq!(
            parse_operation("slate_board_command", &arguments, TurnPolicy::ReadOnly).unwrap_err(),
            "This request is read-only."
        );
        assert!(matches!(
            parse_operation("slate_board_command", &arguments, TurnPolicy::Default).unwrap(),
            BoardOperation::Invoke { .. }
        ));
    }
}
