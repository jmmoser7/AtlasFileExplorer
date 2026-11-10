//! Claude Code print streaming preserves the installed user's subscription login.
use crate::{payload::TextPayload, process::Process, progress::Progress, Provider};
use atlas_agent::{helper::HelperLaunch, AgentModel, AgentRequest, AgentSession};
use serde_json::{json, Value};
use std::{
    path::Path,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub(crate) fn run(
    executable: &Path,
    cwd: &Path,
    request: &AgentRequest,
    tools: Option<&HelperLaunch>,
    payload: &mut TextPayload,
    session: &mut AgentSession,
    answer: usize,
    stop: &AtomicBool,
    changed: &mut impl FnMut(&AgentSession),
) -> Result<(), String> {
    // One captured Model end time covers setup, tools and final response.
    let task_deadline = tools
        .map(|tools| {
            tools
                .remaining_time()
                .map(|remaining| Instant::now() + remaining)
        })
        .transpose()?;

    let (mut process, init) = connect(
        executable,
        cwd,
        Some(request),
        tools,
        stop,
        Some(&mut payload.progress),
    )?;
    crate::catalog::validate(
        &crate::catalog::claude(&init),
        request,
        !payload.images.is_empty(),
        tools.is_some(),
    )?;
    let state = verify_tools(&mut process, tools, stop, Some(&mut payload.progress))?;
    payload.observe_host_ready(tools);
    if let Some(state) = state.as_deref() {
        payload.observe_mcp_status(state);
    }
    payload.progress.connected(session);
    changed(session);
    // The prompt never crosses cmd/PowerShell argument parsing or the command line.
    let deadline = task_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(300));
    if Instant::now() >= deadline {
        return Err("The captured task deadline elapsed before prompt submission.".into());
    }
    let wire = payload.claude_wire();
    payload.send(&mut process, &wire, "claude_stream_user_jsonl")?;

    let mut stream = TextStream::default();
    loop {
        let message = process.receive(stop, deadline)?;
        if payload.progress.claude(&message, tools, session) {
            changed(session);
        }
        if message["type"] == "control_request" {
            respond_control(&mut process, &message, tools, Some(&mut payload.progress))?;
            continue;
        }
        if message["type"] == "system" && message["subtype"] == "init" {
            if let Some(server) = message["mcp_servers"]
                .as_array()
                .and_then(|servers| servers.iter().find(|server| server["name"] == "slate"))
            {
                if let Some(status) = server["status"].as_str() {
                    payload.observe_mcp_status(status);
                }
            }
            if let Some(model) = message["model"].as_str() {
                payload.observe_model(model, "Claude system/init");
            }
        }
        if message["type"] == "assistant" {
            if let Some(model) = message["message"]["model"].as_str() {
                payload.observe_model(model, "Claude assistant model");
            }
        }
        if let Some(id) = message["session_id"].as_str() {
            session.conversation = id.into();
        }
        if message["type"] == "result" {
            if message["is_error"] == true || message["subtype"] != "success" {
                return Err(result_error(&message));
            }
            // The result is authoritative; buffered assistant flushes must not duplicate deltas.
            if let Some(text) = message["result"].as_str() {
                stream.text = text.into();
            }
            session.turns[answer].text.clone_from(&stream.text);
            changed(session);
            process.close_input();
            process.finish(stop, Instant::now() + Duration::from_secs(10))?;
            return Ok(());
        }
        if stream.apply(&message) {
            session.turns[answer].text.clone_from(&stream.text);
            changed(session);
        }
    }
}

pub(crate) fn models(executable: &Path, cwd: &Path) -> Result<Vec<AgentModel>, String> {
    let (_process, init) = connect(executable, cwd, None, None, &AtomicBool::new(false), None)?;
    let models = crate::catalog::claude(&init);
    if models.is_empty() {
        return Err(
            "Claude Code did not advertise any model choices. Update the native CLI.".into(),
        );
    }
    Ok(models)
}

fn connect(
    executable: &Path,
    cwd: &Path,
    request: Option<&AgentRequest>,
    tools: Option<&HelperLaunch>,
    stop: &AtomicBool,
    progress: Option<&mut Progress>,
) -> Result<(Process, Value), String> {
    let mut command = Provider::ClaudeCode.command(executable);
    command.current_dir(cwd).args([
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--permission-prompt-tool",
        "stdio",
        "--settings",
        r#"{"fastMode":false}"#,
    ]);
    command
        .arg("--mcp-config")
        .arg(crate::tool_client::claude_config(tools)?.to_string());
    if let Some(request) = request {
        if let Some(model) = request.model.as_deref().filter(|model| !model.is_empty()) {
            command.arg("--model").arg(model);
        }
        if let Some(effort) = request.model_parameters.get("effort") {
            command.arg("--effort").arg(effort);
        }
    }
    let mut process = Process::spawn(command)?;
    let init = control(
        &mut process,
        "slate-initialize",
        json!({"subtype":"initialize","hooks":null,"skills":[],"excludeDynamicSections":true}),
        tools,
        stop,
        progress,
    )?;
    if init["fast_mode_state"] == "on" {
        return Err("Claude Fast mode is active; this subscription connection requires standard speed without extra-usage opt-in.".into());
    }
    Ok((process, init))
}

fn control(
    process: &mut Process,
    id: &str,
    request: Value,
    tools: Option<&HelperLaunch>,
    stop: &AtomicBool,
    mut progress: Option<&mut Progress>,
) -> Result<Value, String> {
    let subtype = if request["subtype"] == "mcp_status" {
        "mcp_status"
    } else {
        "initialize"
    };
    if let Some(progress) = progress.as_deref_mut() {
        progress.note("control_request_before_stdin", id, subtype, None);
    }
    process.send(&json!({"type":"control_request","request_id":id,"request":request}))?;
    if let Some(progress) = progress.as_deref_mut() {
        progress.note("control_request_stdin_flushed", id, subtype, None);
    }
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let message = process.receive(stop, deadline)?;
        if let Some(progress) = progress.as_deref_mut() {
            progress.observe_status(&message);
        }
        if message["type"] == "control_response" && message["response"]["request_id"] == id {
            if let Some(progress) = progress.as_deref_mut() {
                progress.note(
                    "control_response_received",
                    id,
                    if message["response"]["subtype"] == "success" {
                        "success"
                    } else {
                        "error"
                    },
                    None,
                );
            }
            if message["response"]["subtype"] != "success" {
                return Err("Claude Code rejected subscription control initialization. Check the native CLI version and sign-in.".into());
            }
            return Ok(message["response"]["response"].clone());
        }
        if message["type"] == "control_request" {
            respond_control(process, &message, tools, progress.as_deref_mut())?;
        }
    }
}

fn respond_control(
    process: &mut Process,
    message: &Value,
    tools: Option<&HelperLaunch>,
    mut progress: Option<&mut Progress>,
) -> Result<(), String> {
    let id = &message["request_id"];
    let id_text = id.as_str().unwrap_or_default();
    let request = &message["request"];
    let recognized = request["subtype"] == "can_use_tool";
    let subtype = if recognized {
        "can_use_tool"
    } else {
        "unsupported"
    };
    let allowed = recognized
        && request["tool_name"]
            .as_str()
            .is_some_and(|name| crate::tool_client::allowed_claude_tool(name, tools));
    if let Some(progress) = progress.as_deref_mut() {
        progress.note("control_request_received", id_text, subtype, Some(allowed));
    }
    let response = if recognized {
        let response = if allowed {
            json!({"behavior":"allow","updatedInput":request["input"]})
        } else {
            json!({"behavior":"deny","message":"This operation is outside the explicitly bound Slate model task."})
        };
        json!({"type":"control_response","response":{"subtype":"success","request_id":id,"response":response}})
    } else {
        json!({"type":"control_response","response":{"subtype":"error","request_id":id,"error":"Unsupported client control request"}})
    };
    if let Some(progress) = progress.as_deref_mut() {
        progress.note(
            "control_reply_before_stdin",
            id_text,
            subtype,
            Some(allowed),
        );
    }
    let result = process.send(&response);
    if let Some(progress) = progress.as_deref_mut() {
        progress.note(
            if result.is_ok() {
                "control_reply_stdin_flushed"
            } else {
                "control_reply_stdin_failed"
            },
            id_text,
            subtype,
            Some(allowed),
        );
    }
    result
}

fn result_error(message: &Value) -> String {
    let text = message["result"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ["auth", "login", "log in", "sign in", "401"]
        .iter()
        .any(|word| text.contains(word))
    {
        format!(
            "Claude Code subscription sign-in is required or has expired. Run {} and refresh.",
            Provider::ClaudeCode.login_command()
        )
    } else if ["limit", "quota", "usage", "429"]
        .iter()
        .any(|word| text.contains(word))
    {
        "Claude Code could not complete the request because of an account or usage limit. Check your subscription and retry.".into()
    } else {
        "Claude Code did not complete the request. Check the native CLI version, connection and subscription status.".into()
    }
}

#[derive(Default)]
pub(crate) struct TextStream {
    pub(crate) text: String,
}
impl TextStream {
    pub(crate) fn apply(&mut self, message: &Value) -> bool {
        if message["type"] == "stream_event" {
            let event = &message["event"];
            if event["type"] == "content_block_delta" && event["delta"]["type"] == "text_delta" {
                if let Some(text) = event["delta"]["text"].as_str() {
                    self.text.push_str(text);
                    return true;
                }
            }
        } else if message["type"] == "assistant" {
            let text: String = message["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "text")
                .filter_map(|block| block["text"].as_str())
                .collect();
            if !text.is_empty() {
                self.text = text;
                return true;
            }
        }
        false
    }
}

fn verify_tools(
    process: &mut Process,
    tools: Option<&HelperLaunch>,
    stop: &AtomicBool,
    progress: Option<&mut Progress>,
) -> Result<Option<String>, String> {
    if tools.is_none() {
        return Ok(None);
    }
    let status = control(
        process,
        "slate-mcp-status",
        json!({"subtype":"mcp_status"}),
        tools,
        stop,
        progress,
    )?;
    let state = status["mcpServers"]
        .as_array()
        .and_then(|servers| servers.iter().find(|server| server["name"] == "slate"))
        .and_then(|server| server["status"].as_str())
        .map(str::to_owned);
    crate::tool_client::wait_ready(tools, stop)?;
    if state.as_deref().is_some_and(|state| state != "connected") {
        return Err("Claude Code reports the exact Slate MCP server is not connected. No model prompt was submitted.".into());
    }
    Ok(state)
}
pub(crate) fn probe_tools(
    executable: &Path,
    cwd: &Path,
    tools: &HelperLaunch,
) -> Result<Value, String> {
    let stop = AtomicBool::new(false);
    let (mut process, init) = connect(executable, cwd, None, Some(tools), &stop, None)?;
    let state = verify_tools(&mut process, Some(tools), &stop, None)?;
    Ok(
        json!({"helper_ready":crate::tool_client::ready(tools),"vendor_status":state,"catalog_models":crate::catalog::claude(&init).len(),"fable":init["models"].as_array().and_then(|models| models.iter().find(|model|model["value"]=="fable")).map(|model|json!({"value":model["value"],"resolvedModel":model["resolvedModel"],"displayName":model["displayName"],"supportedEffortLevels":model["supportedEffortLevels"]})),"user_prompt_submitted":false,"native_authority":false}),
    )
}
