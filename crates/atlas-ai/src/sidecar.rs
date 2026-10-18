//! Spawn the out-of-process Cursor file-link sidecar (Art. VII.8).
//!
//! Slate never embeds the Cursor runtime. This starts the Node watcher that
//! reads `request.json` and writes `session.json`.
//!
//! Discovery, the package install, and the spawn itself are I/O — call them
//! from a worker thread, never the frame loop (Art. II).

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
mod worker;
pub use worker::CursorLink;

/// Start `docs/agent/cursor-sidecar` for one portal session, if we can find
/// `node`, install `@cursor/sdk` when missing, and spawn the watcher.
/// `project_cwd` is the bound folder (the agent's cwd), not the AI workspace.
pub fn spawn_cursor_sidecar(
    ai_workspace: &Path,
    session: &str,
    project_cwd: &Path,
) -> Result<Child, String> {
    spawn_cursor_sidecar_in(
        ai_workspace,
        session,
        project_cwd,
        &crate::agent::agent_dir(ai_workspace, session),
    )
}

/// Continue a saved link directory even if the default AI workspace changed.
pub fn spawn_cursor_sidecar_in(
    ai_workspace: &Path,
    session: &str,
    project_cwd: &Path,
    link_dir: &Path,
) -> Result<Child, String> {
    spawn_cursor_sidecar_with(
        ai_workspace,
        session,
        project_cwd,
        link_dir,
        SidecarPolicy::default(),
    )
}

/// Tool permissions a sidecar holds for its whole life. The Cursor SDK takes
/// the tool allowlist when the agent is created or resumed, so changing it
/// means restarting the sidecar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SidecarPolicy {
    pub read_only: bool,
    pub artifact: bool,
    /// A resolved next-turn grant; None preserves legacy spawn callers.
    pub full_access: Option<bool>,
}

impl SidecarPolicy {
    /// Snapshot the next request's authority. An idle caller restarts Cursor
    /// when this differs from the policy its existing sidecar carries.
    pub fn for_request(session: &str, request: &atlas_agent::AgentRequest) -> Self {
        Self::from_grants(
            request,
            crate::access::granted_for_request(session, request),
            crate::access::relay_granted(session),
        )
    }

    fn from_grants(request: &atlas_agent::AgentRequest, granted: bool, relay: bool) -> Self {
        Self {
            read_only: request.policy == atlas_agent::TurnPolicy::ReadOnly,
            artifact: request.artifact.is_some(),
            full_access: Some(crate::runtime::turn_full_access(granted, relay, request)),
        }
    }
}

/// [`spawn_cursor_sidecar_in`] under `policy`.
pub fn spawn_cursor_sidecar_with(
    ai_workspace: &Path,
    session: &str,
    project_cwd: &Path,
    link_dir: &Path,
    policy: SidecarPolicy,
) -> Result<Child, String> {
    let api_key = crate::cursor_key::resolve().ok_or_else(|| {
        "Cursor API key is not set. Get one from the Cursor dashboard, then paste it in this portal."
            .to_string()
    })?;
    let script = sidecar_script().ok_or_else(|| {
        "Cursor sidecar script not found. Set ATLAS_CURSOR_SIDECAR to docs/agent/cursor-sidecar/index.mjs."
            .to_string()
    })?;
    let node = resolve_runtime()?;
    let dir = script.parent().unwrap_or(ai_workspace);
    ensure_sidecar_deps(&node, dir)?;

    let log = link_dir.join("sidecar.log");
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let log_file = File::create(&log).map_err(|e| format!("Could not create sidecar log: {e}"))?;
    let err_file = log_file
        .try_clone()
        .map_err(|e| format!("Could not clone sidecar log: {e}"))?;

    spawn_node(
        &node,
        &script,
        ai_workspace,
        session,
        project_cwd,
        link_dir,
        &api_key,
        access_env(
            policy,
            policy.full_access.unwrap_or_else(|| {
                if policy.artifact {
                    crate::access::artifact_granted(session)
                } else {
                    crate::access::granted(session)
                }
            }),
        ),
        log_file,
        err_file,
    )
}

/// A read-only sidecar never carries the full-access grant.
fn access_env(policy: SidecarPolicy, granted: bool) -> [(&'static str, &'static str); 2] {
    let flag = |on: bool| if on { "1" } else { "0" };
    [
        (
            "ATLAS_AGENT_FULL_ACCESS",
            flag(granted && !policy.read_only),
        ),
        ("ATLAS_AGENT_READ_ONLY", flag(policy.read_only)),
    ]
}

/// Whether a live sidecar already watches this link folder.
pub fn alive(link_dir: &Path) -> bool {
    std::fs::read_to_string(link_dir.join("sidecar.pid"))
        .ok()
        .and_then(|t| t.trim().parse::<u32>().ok())
        .is_some_and(|pid| pid != std::process::id() && watcher_process(pid).is_some())
}

/// Stop the sidecar recorded in `<link_dir>/sidecar.pid` (the newest watcher
/// for that link folder writes its pid there). Only a live `node` process is
/// killed, so a stale file whose pid the OS has reused is left alone. A
/// watcher running inside `Cursor.exe` is never killed — that pid may be
/// the editor itself — it exits when its record is removed. Returns whether
/// a watcher was stopped. Spawns system tools: call from a worker.
pub fn stop(link_dir: &Path) -> Result<bool, String> {
    let record = link_dir.join("sidecar.pid");
    let Ok(text) = std::fs::read_to_string(&record) else {
        return Ok(false);
    };
    let Ok(pid) = text.trim().parse::<u32>() else {
        let _ = std::fs::remove_file(&record);
        return Ok(false);
    };
    let watcher = (pid != std::process::id())
        .then(|| watcher_process(pid))
        .flatten();
    if watcher == Some(Watcher::Node) {
        kill(pid)?;
    }
    let _ = std::fs::remove_file(&record);
    Ok(watcher.is_some())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watcher {
    Node,
    /// `Cursor.exe` running as Node.
    Electron,
}

fn watcher_image(image: &str) -> Option<Watcher> {
    let image = image.trim().trim_matches('"').to_ascii_lowercase();
    if image.starts_with("node") {
        Some(Watcher::Node)
    } else if image.starts_with("cursor") {
        Some(Watcher::Electron)
    } else {
        None
    }
}

#[cfg(windows)]
fn watcher_process(pid: u32) -> Option<Watcher> {
    use std::os::windows::process::CommandExt;
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.lines()
        .filter(|l| l.contains(&format!("\"{pid}\"")))
        .find_map(|l| watcher_image(l.split(',').next()?))
}

#[cfg(not(windows))]
fn watcher_process(pid: u32) -> Option<Watcher> {
    let out = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    let comm = String::from_utf8_lossy(&out.stdout).into_owned();
    let name = Path::new(comm.trim()).file_name()?.to_str()?.to_string();
    watcher_image(&name)
}

fn kill(pid: u32) -> Result<(), String> {
    let mut cmd;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd = Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000);
    }
    #[cfg(not(windows))]
    {
        cmd = Command::new("kill");
        cmd.arg(pid.to_string());
    }
    let out = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Could not stop the sidecar: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Could not stop the sidecar (pid {pid}): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Human setup steps next to the sidecar script, when we can find them.
pub fn setup_doc() -> Option<PathBuf> {
    let script = sidecar_script()?;
    let doc = script.parent()?.join("SETUP.md");
    doc.is_file().then_some(doc)
}

/// SDK catalog/history access on a worker; never scrape IDE databases to imply attach.
pub fn query_cursor(cwd: &Path, channel: Option<&str>) -> Result<serde_json::Value, String> {
    let mut args = vec![
        if channel.is_some() {
            "--read".to_string()
        } else {
            "--list".to_string()
        },
        cwd.to_string_lossy().into_owned(),
    ];
    if let Some(id) = channel {
        args.push(id.to_string());
    }
    run_sidecar_query(&args)
}

/// Account model catalog. Worker only; the portal caches the result.
pub fn query_cursor_models() -> Result<serde_json::Value, String> {
    run_sidecar_query(&["--models".to_string()])
}

fn run_sidecar_query(args: &[String]) -> Result<serde_json::Value, String> {
    let script = sidecar_script().ok_or("Cursor sidecar is unavailable")?;
    let node = resolve_runtime()?;
    ensure_sidecar_deps(&node, script.parent().unwrap())?;
    let mut cmd = node.command();
    cmd.arg(&script);
    for arg in args {
        cmd.arg(arg);
    }
    if let Some(key) = crate::cursor_key::resolve() {
        cmd.env("CURSOR_API_KEY", key);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let reader = |mut pipe: Box<dyn std::io::Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.by_ref().take(16 * 1024 * 1024).read_to_end(&mut bytes);
            bytes
        })
    };
    let out = reader(Box::new(stdout));
    let err = reader(Box::new(stderr));
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let bytes = out.join().unwrap_or_default();
            let error = err.join().unwrap_or_default();
            if !status.success() {
                return Err(String::from_utf8_lossy(&error).chars().take(500).collect());
            }
            return serde_json::from_slice(&bytes).map_err(|e| format!("Cursor catalog: {e}"));
        }
        if start.elapsed() > std::time::Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cursor catalog timed out".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// Last lines of the sidecar log — named failure, never a blank.
pub fn sidecar_log_tail(ai_workspace: &Path, session: &str, max_lines: usize) -> Option<String> {
    let path = crate::agent::agent_dir(ai_workspace, session).join("sidecar.log");
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && l.len() < 400)
        .collect();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join(" "))
}

/// Absolute `node` / `node.exe`. GUI apps on Windows often inherit a PATH
/// that never saw the installer, so `where node` is not enough. Well-known
/// install folders are hardcoded (they do not depend on `ProgramFiles`).
mod runtime;
use runtime::{ensure_sidecar_deps, sidecar_script, spawn_node};
pub use runtime::{resolve_node, resolve_runtime, NodeRuntime};
#[cfg(test)]
mod tests;
