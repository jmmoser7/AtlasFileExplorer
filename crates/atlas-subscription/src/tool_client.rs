//! Explicit machine-local MCP launch configuration and binding verification.
use atlas_agent::helper::HelperLaunch;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(crate) fn claude_config(tools: Option<&HelperLaunch>) -> Result<Value, String> {
    Ok(match tools {
        Some(tools) => {
            json!({"mcpServers":{"slate":{"type":"stdio","command":tools.executable(),"args":tools.arguments(),"timeout":tools.remaining_time()?.as_millis() as u64,"alwaysLoad":true}}})
        }
        None => json!({"mcpServers":{}}),
    })
}
pub(crate) fn cursor_config(tools: Option<&HelperLaunch>) -> Value {
    match tools {
        Some(tools) => {
            json!([{"name":cursor_server_name(tools),"command":tools.executable(),"args":tools.arguments(),"env":[]}])
        }
        None => json!([]),
    }
}
pub(crate) fn initialized(tools: &HelperLaunch) -> bool {
    crate::host_tools::helper_initialized(tools)
}
pub(crate) fn ready(tools: &HelperLaunch) -> bool {
    crate::host_tools::helper_ready(tools)
}
pub(crate) fn wait_initialized(
    tools: Option<&HelperLaunch>,
    stop: &AtomicBool,
) -> Result<(), String> {
    wait_state(tools, stop, false)
}
pub(crate) fn wait_ready(tools: Option<&HelperLaunch>, stop: &AtomicBool) -> Result<(), String> {
    wait_state(tools, stop, true)
}
fn wait_state(tools: Option<&HelperLaunch>, stop: &AtomicBool, listed: bool) -> Result<(), String> {
    let Some(tools) = tools else {
        return Ok(());
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    while if listed {
        !ready(tools)
    } else {
        !initialized(tools)
    } {
        if stop.load(Ordering::Relaxed) {
            return Err("Response stopped.".into());
        }
        if Instant::now() >= deadline {
            return Err("The installed provider did not initialize this exact task's MCP tools. No model prompt was submitted.".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}
pub(crate) fn allowed_claude_tool(name: &str, tools: Option<&HelperLaunch>) -> bool {
    tools.filter(|tools| ready(tools)).is_some_and(|tools| {
        crate::host_tools::helper_tool_aliases(tools)
            .iter()
            .any(|alias| name == format!("mcp__slate__{alias}"))
    })
}

pub(crate) fn cursor_server_name(tools: &HelperLaunch) -> String {
    let binding = serde_json::to_vec(&json!([
        tools.request_id(),
        crate::host_tools::helper_tool_aliases(tools)
    ]))
    .unwrap();
    format!("slate-task-{:x}", Sha256::digest(binding))
}
pub(crate) fn allowed_cursor_tool(
    message: &Value,
    tools: Option<&HelperLaunch>,
    active_session: Option<&str>,
) -> bool {
    let Some(tools) = tools.filter(|tools| ready(tools)) else {
        return false;
    };
    let Some(session) = active_session else {
        return false;
    };
    let params = &message["params"];
    let call = &params["toolCall"];
    let name = cursor_server_name(tools);
    params["sessionId"] == session
        && call["kind"] == "other"
        && call["title"].as_str().is_some_and(|title| {
            crate::host_tools::helper_tool_aliases(tools)
                .iter()
                .any(|alias| title == format!("{name}-{alias}: {alias}"))
        })
}
