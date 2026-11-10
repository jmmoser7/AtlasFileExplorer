//! Renderer-free MCP stdio adapter over the one neutral BoardCall exchange.
//! No native caller, permit, grant creation, or GUI startup exists here.
use atlas_agent::collaborator::{
    host_tools::HOST_TOOLS_FLAG,
    tools::{parse_operation, tool_schemas},
    transport::{self, CloudGuard, PendingCalls},
    BoardLinkSession, MAX_BOARD_MESSAGE_BYTES,
};
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

// Claude vision's per-image base64 request limit; BoardCall metadata stays 64 KiB.
const MAX_MCP_IMAGE_BASE64_BYTES: usize = 10_000_000;
mod artifact;
mod binding;
mod delivery;
mod route;
pub use binding::{helper_initialized, helper_ready, helper_tool_aliases};

/// Correlation only; the live typed host grant remains authoritative.
pub fn tool_aliases(session: &BoardLinkSession, request: &str) -> [String; 2] {
    let identity = serde_json::to_vec(&(session, request)).expect("string identity serializes");
    let nonce = format!("{:x}", Sha256::digest(identity));
    let schemas = tool_schemas(true);
    std::array::from_fn(|at| {
        format!(
            "m_{}_{}",
            &nonce[..32],
            schemas[at]["name"].as_str().unwrap()
        )
    })
}
fn request_tool_schemas(session: &BoardLinkSession, request: &str) -> Value {
    let mut schemas = tool_schemas(true);
    for (tool, name) in schemas
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(tool_aliases(session, request))
    {
        tool["name"] = json!(name);
    }
    schemas
}
fn parse_task_operation(
    name: &str,
    arguments: &Value,
    session: &BoardLinkSession,
    request: &str,
) -> Result<atlas_agent::collaborator::BoardOperation, String> {
    let at = tool_aliases(session, request)
        .iter()
        .position(|alias| alias == name)
        .ok_or("Tool is not offered by this exact model task request")?;
    let schemas = tool_schemas(true);
    parse_operation(
        schemas[at]["name"].as_str().unwrap(),
        arguments,
        atlas_agent::TurnPolicy::Default,
    )
}

pub fn run_if_requested(args: &[String], guard: CloudGuard) -> Option<Result<(), String>> {
    let at = args.iter().position(|arg| arg == HOST_TOOLS_FLAG)?;
    Some((|| {
        if at != 1 || args.len() != 7 {
            return Err("Invalid Slate host tool invocation".into());
        }
        let root = PathBuf::from(&args[2]);
        if !root.is_absolute() || !atlas_agent::collaborator::valid_id(&args[4]) {
            return Err("Invalid explicit helper route".into());
        }
        let (dir, route) = if args[5] == "artifact" {
            let identity: atlas_agent::artifact_tools::ArtifactIdentity =
                serde_json::from_str(&args[3]).map_err(|_| "Invalid Artifact route")?;
            identity.validate()?;
            if identity.request_id != args[4] {
                return Err("Artifact route/request mismatch".into());
            }
            (root, route::Route::Artifact(identity))
        } else {
            let session: BoardLinkSession =
                serde_json::from_str(&args[3]).map_err(|_| "Invalid model route")?;
            session.validate().map_err(str::to_owned)?;
            let workspace = PathBuf::from(&args[5]);
            if !session.is_model_task() || !workspace.is_absolute() {
                return Err("Invalid explicit model task route".into());
            }
            (
                session.transport_dir(&root),
                route::Route::Model {
                    session,
                    request: args[4].clone(),
                    workspace,
                },
            )
        };
        serve(
            &dir,
            route,
            args[6].parse().map_err(|_| "Invalid task deadline")?,
            guard,
        )
    })())
}

struct Ready(PathBuf);
impl Drop for Ready {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn serve(
    dir: &Path,
    route: route::Route,
    deadline_unix_ms: u64,
    guard: CloudGuard,
) -> Result<(), String> {
    let remaining = atlas_agent::collaborator::host_tools::remaining_time(deadline_unix_ms)?;
    if matches!(&route, route::Route::Artifact(identity) if remaining > Duration::from_secs(identity.budget_seconds))
    {
        return Err("Artifact helper deadline exceeds its validated task budget".into());
    }
    let deadline = Instant::now()
        .checked_add(remaining)
        .ok_or("Helper deadline overflow")?;
    // Host binding is asynchronous, but its absence never grants anything.
    let setup = Instant::now() + Duration::from_secs(5);
    while !route.current(dir, guard) {
        if Instant::now() >= setup {
            return Err("The model task host is unavailable or stale".into());
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let (tx, rx) = mpsc::sync_channel(16);
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(std::io::stdin());
        loop {
            let mut bytes = Vec::new();
            let result = match reader
                .by_ref()
                .take(MAX_BOARD_MESSAGE_BYTES + 1)
                .read_until(b'\n', &mut bytes)
            {
                Ok(0) => break,
                Ok(_) if bytes.len() as u64 > MAX_BOARD_MESSAGE_BYTES => {
                    Err("MCP message exceeds its byte limit".to_string())
                }
                Ok(_) => serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_| "Invalid MCP JSON message".to_string()),
                Err(_) => Err("Could not read MCP input".to_string()),
            };
            let failed = result.is_err();
            if tx.send(result).is_err() || failed {
                break;
            }
        }
    });
    let ready = Ready(dir.join("helper-ready.json"));
    let mut delivery = delivery::Audit::new(dir);
    let mut output = std::io::stdout().lock();
    let mut pending = route::Pending::default();
    let cancel = AtomicBool::new(false);
    let mut initialized = false;
    let mut connected = false;
    let mut tools_listed = false;
    loop {
        let live = Instant::now() < deadline && route.current(dir, guard);
        if !live {
            cancel.store(true, Ordering::Relaxed);
        }
        for done in pending.poll(&route, &cancel, guard) {
            delivery.send_dispatch(
                &mut output,
                || route.current(dir, guard),
                &done.audit,
                done.token,
                done.result,
                done.ticket.as_deref(),
            )?;
        }
        if !live {
            return Err("The model task host ended or revoked this request".into());
        }
        let message = match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(message) => message?,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        };
        let method = message["method"].as_str().unwrap_or_default();
        if method == "notifications/initialized" {
            connected = initialized;
            continue;
        }
        if method == "notifications/cancelled" {
            // A native action may already be admitted. Revocation/publication stays host-owned.
            cancel.store(true, Ordering::Relaxed);
            continue;
        }
        let Some(id) = message.get("id").cloned() else {
            continue;
        };
        if message["jsonrpc"] != "2.0" {
            send_error(&mut output, id, -32600, "Invalid JSON-RPC version")?;
            continue;
        }
        match method {
            "initialize" if !initialized => {
                initialized = true;
                let requested = message["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or_default();
                let version = match requested {
                    "2024-11-05" | "2025-03-26" | "2025-06-18" => requested,
                    _ => "2025-06-18",
                };
                let instructions = route.instructions();
                send(
                    &mut output,
                    json!({"jsonrpc":"2.0","id":id,"result":{"protocolVersion":version,"capabilities":{"tools":{}},"serverInfo":{"name":route.server_name(),"version":env!("CARGO_PKG_VERSION")},"instructions":instructions}}),
                )?;
                write_ready(&ready.0, &route, deadline_unix_ms, false)?;
            }
            "ping" => send(&mut output, json!({"jsonrpc":"2.0","id":id,"result":{}}))?,
            "tools/list" if connected => {
                send(
                    &mut output,
                    json!({"jsonrpc":"2.0","id":id,"result":{"tools":route.schemas()}}),
                )?;
                write_ready(&ready.0, &route, deadline_unix_ms, true)?;
                tools_listed = true;
            }
            "tools/call" if connected && tools_listed => {
                let name = message["params"]["name"].as_str().unwrap_or_default();
                let result = pending.start(
                    &route,
                    dir,
                    id.clone(),
                    name,
                    &message["params"]["arguments"],
                );
                if let Err(error) = result {
                    send(
                        &mut output,
                        json!({"jsonrpc":"2.0","id":id,"result":{"content":[{"type":"text","text":error}],"isError":true}}),
                    )?;
                }
            }
            _ => send_error(
                &mut output,
                id,
                -32601,
                "Method unavailable for this initialized model task",
            )?,
        }
    }
}

fn reply_content(
    workspace: &Path,
    guard: CloudGuard,
    operation: &atlas_agent::collaborator::BoardOperation,
    success: bool,
    result: &Value,
) -> Result<Vec<Value>, String> {
    let mut content = vec![
        json!({"type":"text","text":serde_json::to_string(result).map_err(|e|e.to_string())?}),
    ];
    if let Some(bytes) = atlas_agent::collaborator::images::result_image(
        Some(workspace),
        Some(guard),
        operation,
        success,
        result,
    )? {
        content.push(mcp_image(&bytes)?);
    }
    Ok(content)
}

fn mcp_image(bytes: &[u8]) -> Result<Value, String> {
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    if data.len() > MAX_MCP_IMAGE_BASE64_BYTES {
        return Err("The model capture exceeds the provider's 10 MB encoded image limit. Capture a smaller view and retry.".into());
    }
    Ok(json!({"type":"image","mimeType":"image/png","data":data}))
}

fn send(output: &mut impl Write, value: Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    output
        .write_all(&bytes)
        .and_then(|_| output.write_all(b"\n"))
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}
fn send_error(output: &mut impl Write, id: Value, code: i64, message: &str) -> Result<(), String> {
    send(
        output,
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}),
    )
}
fn write_ready(
    path: &Path,
    route: &route::Route,
    deadline_unix_ms: u64,
    listed: bool,
) -> Result<(), String> {
    let mut value = route.ready();
    value["tools_listed"] = json!(listed);
    value["deadline_unix_ms"] = json!(deadline_unix_ms);
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(&value).map_err(|e| e.to_string())?,
    )
    .and_then(|_| std::fs::rename(temporary, path))
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
