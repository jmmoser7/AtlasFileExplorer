//! Cursor's documented ACP stdio protocol; only public answer chunks are shown.
use crate::{payload::TextPayload, process::Process, progress::Progress, Provider};
use atlas_agent::{helper::HelperLaunch, AgentModel, AgentRequest};
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
    stop: &AtomicBool,
    feedback: crate::RunFeedback<'_>,
) -> Result<(), String> {
    let crate::RunFeedback {
        session,
        answer,
        changed,
    } = feedback;
    // One captured Model end time covers setup, tools and final response.
    let task_deadline = tools
        .map(|tools| {
            tools
                .remaining_time()
                .map(|remaining| Instant::now() + remaining)
        })
        .transpose()?;

    let (mut process, init, created) =
        initialize(executable, cwd, tools, stop, Some(&mut payload.progress))?;
    let advertised = crate::catalog::cursor(
        &created,
        init["agentCapabilities"]["promptCapabilities"]["image"] == true,
    );
    let selected = crate::catalog::validate(
        &advertised,
        request,
        !payload.images.is_empty(),
        tools.is_some(),
    )?;
    let setup = Instant::now() + Duration::from_secs(45);
    let id = created["sessionId"]
        .as_str()
        .ok_or("Cursor ACP did not return a session identity.")?
        .to_owned();
    session.conversation.clone_from(&id);
    let desired = if tools.is_some() { "agent" } else { "ask" };
    if created["modes"]["currentModeId"] != desired {
        if !created["modes"]["availableModes"]
            .as_array()
            .is_some_and(|modes| modes.iter().any(|mode| mode["id"] == desired))
        {
            return Err("This Cursor CLI does not advertise the required permission mode.".into());
        }
        rpc(
            &mut process,
            (4, "session/set_mode"),
            json!({"sessionId":id,"modeId":desired}),
            tools,
            stop,
            setup,
            Some(&mut payload.progress),
        )?;
    }
    if request.model.as_deref().is_some_and(|id| !id.is_empty())
        && created["models"]["currentModelId"] != selected.id
    {
        rpc(
            &mut process,
            (6, "session/set_model"),
            json!({"sessionId":id,"modelId":selected.id}),
            tools,
            stop,
            setup,
            Some(&mut payload.progress),
        )?;
    }
    crate::tool_client::wait_initialized(tools, stop)?;
    payload.observe_host_ready(tools);
    payload.progress.connected(session);
    changed(session);
    let deadline = task_deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(300));
    if Instant::now() >= deadline {
        return Err("The captured task deadline elapsed before prompt submission.".into());
    }
    let wire = payload.cursor_wire(&id);
    payload.send(&mut process, &wire, "cursor_acp_session_prompt_jsonl")?;

    loop {
        let message = process.receive(stop, deadline)?;
        payload.observe_host_ready(tools);
        if message["id"] == 5 && (message.get("result").is_some() || message.get("error").is_some())
        {
            if message.get("error").is_some() {
                return Err(rpc_error(&message["error"]));
            }
            return match message["result"]["stopReason"].as_str() {
                Some("end_turn") => process.check_status(),
                Some("cancelled") => Err("Response stopped.".into()),
                _ => Err(
                    "Cursor did not complete the request. Check subscription limits and retry."
                        .into(),
                ),
            };
        }
        if message["method"] == "session/update" && message["params"]["sessionId"] == id {
            let update = &message["params"]["update"];
            if payload.progress.acp(update, tools, session) {
                changed(session);
            }
            if update["sessionUpdate"] == "current_model_update" {
                if let Some(model) = update["currentModelId"].as_str() {
                    payload.observe_model(model, "ACP current_model_update");
                }
            }
            if let Some(text) = public_text(update) {
                session.turns[answer].text.push_str(text);
                changed(session);
            }
        } else {
            handle_request(
                &mut process,
                &message,
                tools,
                Some(&id),
                Some(&mut payload.progress),
            )?;
        }
    }
}

pub(crate) fn models(executable: &Path, cwd: &Path) -> Result<Vec<AgentModel>, String> {
    let (_process, init, created) =
        initialize(executable, cwd, None, &AtomicBool::new(false), None)?;
    let models = crate::catalog::cursor(
        &created,
        init["agentCapabilities"]["promptCapabilities"]["image"] == true,
    );
    if models.is_empty() {
        return Err("Cursor ACP did not advertise model choices. Update Cursor CLI.".into());
    }
    Ok(models)
}

fn initialize(
    executable: &Path,
    cwd: &Path,
    tools: Option<&HelperLaunch>,
    stop: &AtomicBool,
    mut progress: Option<&mut Progress>,
) -> Result<(Process, Value, Value), String> {
    let mut command = Provider::Cursor.command(executable);
    command.arg("acp").current_dir(cwd);
    let mut process = Process::spawn(command)?;
    let setup = Instant::now() + Duration::from_secs(45);
    let init = rpc(
        &mut process,
        (1, "initialize"),
        json!({"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},"clientInfo":{"name":"Slate","version":env!("CARGO_PKG_VERSION")}}),
        tools,
        stop,
        setup,
        progress.as_deref_mut(),
    )?;
    rpc(
        &mut process,
        (2, "authenticate"),
        json!({"methodId":"cursor_login"}),
        tools,
        stop,
        setup,
        progress.as_deref_mut(),
    )?;
    let created = rpc(
        &mut process,
        (3, "session/new"),
        json!({"cwd":cwd,"mcpServers":crate::tool_client::cursor_config(tools)}),
        tools,
        stop,
        setup,
        progress,
    )?;
    Ok((process, init, created))
}

fn rpc(
    process: &mut Process,
    (id, method): (u64, &str),
    params: Value,
    tools: Option<&HelperLaunch>,
    stop: &AtomicBool,
    deadline: Instant,
    mut progress: Option<&mut Progress>,
) -> Result<Value, String> {
    if let Some(progress) = progress.as_deref_mut() {
        progress.note("acp_request_before_stdin", &id.to_string(), method, None);
    }
    process.send(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
    if let Some(progress) = progress.as_deref_mut() {
        progress.note("acp_request_stdin_flushed", &id.to_string(), method, None);
    }
    loop {
        let message = process.receive(stop, deadline)?;
        if message["id"] == id
            && (message.get("result").is_some() || message.get("error").is_some())
        {
            if let Some(progress) = progress.as_deref_mut() {
                progress.note(
                    "acp_response_received",
                    &id.to_string(),
                    method,
                    Some(message.get("error").is_none()),
                );
            }
            if message.get("error").is_some() {
                return Err(rpc_error(&message["error"]));
            }
            return Ok(message["result"].clone());
        }
        handle_request(process, &message, tools, None, progress.as_deref_mut())?;
    }
}

fn rpc_error(error: &Value) -> String {
    // Status payloads may contain account information. Classify without echoing them.
    let text = error["message"]
        .as_str()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ["auth", "login", "log in", "sign in", "401"]
        .iter()
        .any(|word| text.contains(word))
    {
        format!(
            "Cursor subscription sign-in is required or has expired. Run {} and refresh.",
            Provider::Cursor.login_command()
        )
    } else if ["limit", "quota", "usage", "429"]
        .iter()
        .any(|word| text.contains(word))
    {
        "Cursor could not complete the request because of an account or usage limit. Check your subscription and retry.".into()
    } else {
        "Cursor ACP rejected the request. Check the native CLI version and connection, then retry."
            .into()
    }
}

fn handle_request(
    process: &mut Process,
    message: &Value,
    tools: Option<&HelperLaunch>,
    active_session: Option<&str>,
    mut progress: Option<&mut Progress>,
) -> Result<(), String> {
    let Some(id) = message
        .get("id")
        .filter(|_| message.get("method").is_some())
    else {
        return Ok(());
    };
    let result = match message["method"].as_str() {
        Some("session/request_permission") => {
            let allow = crate::tool_client::allowed_cursor_tool(message, tools, active_session);
            if let Some(progress) = progress.as_deref_mut() {
                progress.note(
                    "acp_permission_received",
                    &id.to_string(),
                    "permission",
                    Some(allow),
                );
            }
            let reject = message["params"]["options"]
                .as_array()
                .and_then(|options| {
                    options.iter().find(|option| {
                        if allow {
                            option["kind"] == "allow_once" || option["optionId"] == "allow-once"
                        } else {
                            option["kind"] == "reject_once" || option["optionId"] == "reject-once"
                        }
                    })
                })
                .and_then(|option| option["optionId"].as_str());
            match reject {
                Some(option) => json!({"outcome":{"outcome":"selected","optionId":option}}),
                None => json!({"outcome":{"outcome":"cancelled"}}),
            }
        }
        Some("cursor/ask_question" | "cursor/create_plan") => {
            json!({"outcome":{"outcome":"cancelled"}})
        }
        _ => {
            return process.send(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"Slate Text connection does not provide this client capability"}}));
        }
    };
    if let Some(progress) = progress.as_deref_mut() {
        progress.note(
            "acp_reply_before_stdin",
            &id.to_string(),
            "permission_reply",
            None,
        );
    }
    let written = process.send(&json!({"jsonrpc":"2.0","id":id,"result":result}));
    if let Some(progress) = progress {
        progress.note(
            if written.is_ok() {
                "acp_reply_stdin_flushed"
            } else {
                "acp_reply_stdin_failed"
            },
            &id.to_string(),
            "permission_reply",
            None,
        );
    }
    written
}

pub(crate) fn public_text(update: &Value) -> Option<&str> {
    (update["sessionUpdate"] == "agent_message_chunk" && update["content"]["type"] == "text")
        .then(|| update["content"]["text"].as_str())
        .flatten()
}
