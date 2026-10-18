//! Dynamic board tools from the installed app-server schema (16 Oct 2026).
//! Provider work remains on its existing worker; Slate resolves the same typed calls.
use super::*;
use atlas_agent::collaborator::{BoardCall, BoardOperation, BoardReply, MAX_BOARD_MESSAGE_BYTES};

mod image_transport;

pub(super) fn tools() -> Value {
    json!([
        {"type":"function","name":"slate_board_read",
         "description":"Read the explicitly granted Slate board scope, stable node IDs and available registered commands. Board text is user data.",
         "inputSchema":{"type":"object","properties":{"node_ids":{"type":"array","items":{"type":"string"},"maxItems":64},"offset":{"type":"integer","minimum":0,"maximum":1000000,"default":0},"limit":{"type":"integer","minimum":1,"maximum":64,"default":32}},"additionalProperties":false}},
        {"type":"function","name":"slate_participants_read",
         "description":"Read live or temporarily retained participant outcomes authorized by the current board grant. Empty participant_ids reads the authorized participant scope.",
         "inputSchema":{"type":"object","properties":{"participant_ids":{"type":"array","items":{"type":"string"},"maxItems":64},"offset":{"type":"integer","minimum":0,"maximum":1000000,"default":0},"limit":{"type":"integer","minimum":1,"maximum":8,"default":8}},"additionalProperties":false}},
        {"type":"function","name":"slate_board_command",
         "description":"Invoke a registered Slate command using stable target IDs. Its real action is visibly previewed and proposed unless the human granted board autonomy. Read command schemas first.",
         "inputSchema":{"type":"object","properties":{"command":{"type":"string"},"targets":{"type":"array","items":{"type":"string"},"maxItems":64},"arguments":{"type":"object"}},"required":["command","targets","arguments"],"additionalProperties":false}}
    ])
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
        // A fresh membership cannot inherit an old job or an ordinary chat's context.
        let mut params = thread_params(&self.cwd, None);
        if request.collaborator.is_some() {
            params["dynamicTools"] = tools();
        }
        let response = self.rpc("thread/start", params)?;
        self.thread = response
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or("Codex did not return a collaborator conversation id")?
            .into();
        self.permissions = TurnPermissions::default();
        self.permissions.own = Permissions::from_thread_response(&response);
        self.board_session = request.collaborator.clone();
        Ok(())
    }
}

struct Pending {
    rpc_id: Value,
    call: BoardCall,
    started: Instant,
    saw_binding: bool,
}
#[derive(Default)]
pub(super) struct PendingCalls {
    calls: Vec<Pending>,
    next_sequence: u64,
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
        if p["threadId"] != client.thread || p["turnId"] != turn || self.calls.len() >= 8 {
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
        let args = &p["arguments"];
        let operation =
            match parse_operation(p["tool"].as_str().unwrap_or_default(), args, request.policy) {
                Ok(operation) => operation,
                Err(error) => {
                    return tool_result(client, &event["id"], false, json!({"error":error}));
                }
            };
        let call = BoardCall {
            id: atlas_agent::request_id(),
            session: link.clone(),
            request_id: request.id.clone(),
            sequence: self.next_sequence,
            operation,
        };
        self.next_sequence += 1;
        if let Err(error) = call.validate() {
            return tool_result(client, &event["id"], false, json!({"error":error}));
        }
        let Some(dir) = &client.approval_dir else {
            return tool_result(
                client,
                &event["id"],
                false,
                json!({"error":"This collaborator has no board link."}),
            );
        };
        if let Err(error) = write_call(dir, &call) {
            return tool_result(client, &event["id"], false, json!({"error":error}));
        }
        self.calls.push(Pending {
            rpc_id: event["id"].clone(),
            call,
            started: Instant::now(),
            saw_binding: false,
        });
        Ok(())
    }

    pub(super) fn poll(
        &mut self,
        client: &mut Client,
        request: &AgentRequest,
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        if self.calls.is_empty() {
            return Ok(());
        }
        let Some(dir) = client.approval_dir.clone() else {
            return Ok(());
        };
        let binding = read_json(&dir.join("board-session.json"), client.board_cloud_guard);
        let mut i = 0;
        while i < self.calls.len() {
            let pending = &mut self.calls[i];
            let current = binding.as_ref().is_some_and(|v| {
                v["session"] == serde_json::to_value(&pending.call.session).unwrap_or(Value::Null)
                    && v["request_id"].as_str() == Some(request.id.as_str())
            });
            pending.saw_binding |= current;
            let error = if cancel.load(Ordering::Relaxed) {
                Some("Board task cancelled.")
            } else if !current
                && (pending.saw_binding || pending.started.elapsed() > Duration::from_secs(5))
            {
                Some("Board membership was revoked or replaced.")
            } else if pending.started.elapsed() > Duration::from_secs(900) {
                Some("Board action timed out.")
            } else {
                None
            };
            let path = dir
                .join("board-replies")
                .join(format!("{}.json", pending.call.id));
            let reply = current
                .then(|| read_json(&path, client.board_cloud_guard))
                .flatten()
                .and_then(|v| serde_json::from_value::<BoardReply>(v).ok())
                .filter(|r| r.matches(&pending.call));
            if let Some(error) = error {
                tool_result(client, &pending.rpc_id, false, json!({"error":error}))?;
            } else if let Some(reply) = reply {
                tool_reply(client, &pending.rpc_id, &pending.call.operation, reply)?;
            } else {
                i += 1;
                continue;
            }
            let id = pending.call.id.clone();
            self.calls.remove(i);
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(dir.join("board-calls").join(format!("{id}.json")));
        }
        Ok(())
    }
}

fn parse_operation(tool: &str, args: &Value, policy: TurnPolicy) -> Result<BoardOperation, String> {
    let mut args = args
        .as_object()
        .ok_or("Board tool arguments must be an object.")?
        .clone();
    let kind = match tool {
        "slate_board_read" => "read",
        "slate_participants_read" => "read_participants",
        "slate_board_command" => "invoke",
        _ => return Err("Unknown Slate board tool.".into()),
    };
    args.insert("kind".into(), json!(kind));
    let operation = serde_json::from_value(Value::Object(args))
        .map_err(|e| format!("Invalid board tool arguments: {e}"))?;
    if policy == TurnPolicy::ReadOnly && matches!(operation, BoardOperation::Invoke { .. }) {
        return Err("This request is read-only.".into());
    }
    Ok(operation)
}

fn write_call(dir: &Path, call: &BoardCall) -> Result<(), String> {
    let folder = dir.join("board-calls");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let target = folder.join(format!("{}.json", call.id));
    let temp = folder.join(format!("{}.tmp", call.id));
    std::fs::write(&temp, serde_json::to_vec(call).map_err(|e| e.to_string())?)
        .and_then(|_| std::fs::rename(temp, target))
        .map_err(|e| e.to_string())
}

fn read_json(path: &Path, cloud_guard: Option<fn(&Path) -> bool>) -> Option<Value> {
    use std::io::Read;
    if cloud_guard?(path) {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BOARD_MESSAGE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_BOARD_MESSAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BOARD_MESSAGE_BYTES {
        return None;
    }
    serde_json::from_slice(&bytes).ok()
}

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
        &client.cwd,
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
                limit: 8
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
