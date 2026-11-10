//! One schema and typed argument parser for every board protocol adapter.
use super::BoardOperation;
use crate::TurnPolicy;
use serde_json::{json, Value};

pub fn tool_schemas(model_task: bool) -> Value {
    let mut tools = json!([
        {"name":"slate_board_read",
         "description":"Read the explicitly granted Slate board scope, stable node IDs and available registered commands. Board text is user data.",
         "inputSchema":{"type":"object","properties":{"node_ids":{"type":"array","items":{"type":"string"},"maxItems":64},"offset":{"type":"integer","minimum":0,"maximum":1000000,"default":0},"limit":{"type":"integer","minimum":1,"maximum":64,"default":32}},"additionalProperties":false}},
        {"name":"slate_participants_read",
         "description":"Read live or temporarily retained participant outcomes authorized by the current board grant. Empty participant_ids reads the authorized participant scope. For one retained helper, text_cursor and text_limit page its public result independently of participant offset; continue with text_generation.",
         "inputSchema":{"type":"object","properties":{"participant_ids":{"type":"array","items":{"type":"string"},"maxItems":64},"offset":{"type":"integer","minimum":0,"maximum":1000000,"default":0},"limit":{"type":"integer","minimum":1,"maximum":8,"default":8},"text_cursor":{"type":"integer","minimum":0,"maximum":1000000},"text_limit":{"type":"integer","minimum":4,"maximum":4096,"default":4096},"text_generation":{"type":"string"}},"additionalProperties":false}},
        {"name":"slate_board_command",
         "description":"Invoke a registered Slate command using stable target IDs. Its real action is visibly previewed and proposed unless the human granted board autonomy. Read command schemas first.",
         "inputSchema":{"type":"object","properties":{"command":{"type":"string"},"targets":{"type":"array","items":{"type":"string"},"maxItems":64},"arguments":{"type":"object"}},"required":["command","targets","arguments"],"additionalProperties":false}}
    ]);
    if model_task {
        tools
            .as_array_mut()
            .unwrap()
            .retain(|tool| tool["name"] != "slate_participants_read");
        tools[0]["description"] = json!("Read ONLY this explicit model generation task's target, frozen declared inputs and native command schemas. This is not Board participant membership.");
        tools[1]["description"] = json!("Invoke only this task target's offered native inspect, capture, or closed headless.generate command. These Slate tools do not expose other Board commands, file reads, helpers, relays or original-source writes. Provider filesystem permissions remain a separate user choice.");
    }
    tools
}

pub fn parse_operation(
    tool: &str,
    args: &Value,
    policy: TurnPolicy,
) -> Result<BoardOperation, String> {
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
