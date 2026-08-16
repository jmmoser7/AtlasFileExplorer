//! Spawn the out-of-process Cursor file-link sidecar (Art. VII.8).
//!
//! Slate never embeds the Cursor runtime. This starts the Node watcher that
//! reads `request.json` and writes `session.json`. I/O only — not the frame loop.

use std::path::{Path, PathBuf};

/// Start `docs/agent/cursor-sidecar` for one portal session, if we can find
/// `node` and the script. `project_cwd` is the bound folder (the agent's cwd),
/// not the AI workspace.
pub fn spawn_cursor_sidecar(
    ai_workspace: &Path,
    session: &str,
    project_cwd: &Path,
) -> Result<std::process::Child, String> {
    if std::env::var_os("CURSOR_API_KEY").is_none() {
        return Err(
            "CURSOR_API_KEY is not set — the sidecar cannot reach Cursor agents without it."
                .into(),
        );
    }
    let script = sidecar_script().ok_or_else(|| {
        "Cursor sidecar script not found. From the repo: npm install in docs/agent/cursor-sidecar, \
or set ATLAS_CURSOR_SIDECAR to index.mjs."
            .to_string()
    })?;
    let node = node_exe().ok_or_else(|| {
        "Node.js was not found — install it to run the Cursor agent link.".to_string()
    })?;
    let log = crate::agent::agent_dir(ai_workspace, session).join("sidecar.log");
    if let Some(parent) = log.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let log_file =
        std::fs::File::create(&log).map_err(|e| format!("Could not create sidecar log: {e}"))?;
    let err_file = log_file
        .try_clone()
        .map_err(|e| format!("Could not clone sidecar log: {e}"))?;

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new(node)
            .arg(&script)
            .current_dir(script.parent().unwrap_or(ai_workspace))
            .env("ATLAS_AI_WORKSPACE", ai_workspace)
            .env("ATLAS_AGENT_SESSION", session)
            .env("ATLAS_AGENT_CWD", project_cwd)
            .stdout(log_file)
            .stderr(err_file)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Could not start Cursor sidecar: {e}"))
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(node)
            .arg(&script)
            .current_dir(script.parent().unwrap_or(ai_workspace))
            .env("ATLAS_AI_WORKSPACE", ai_workspace)
            .env("ATLAS_AGENT_SESSION", session)
            .env("ATLAS_AGENT_CWD", project_cwd)
            .stdout(log_file)
            .stderr(err_file)
            .spawn()
            .map_err(|e| format!("Could not start Cursor sidecar: {e}"))
    }
}

/// Last lines of the sidecar log — named failure, never a blank.
pub fn sidecar_log_tail(ai_workspace: &Path, session: &str, max_lines: usize) -> Option<String> {
    let path = crate::agent::agent_dir(ai_workspace, session).join("sidecar.log");
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return None;
    }
    let start = lines.len().saturating_sub(max_lines);
    Some(lines[start..].join(" "))
}

fn node_exe() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("where")
            .arg("node")
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(|s| PathBuf::from(s.trim()))
            .filter(|p| p.is_file())
    }
    #[cfg(not(windows))]
    {
        let out = std::process::Command::new("which")
            .arg("node")
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(|s| PathBuf::from(s.trim()))
            .filter(|p| p.is_file())
    }
}

fn sidecar_script() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("ATLAS_CURSOR_SIDECAR") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("cursor-sidecar").join("index.mjs"));
            candidates.push(dir.join("../../../docs/agent/cursor-sidecar/index.mjs"));
        }
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/agent/cursor-sidecar/index.mjs"),
    );
    candidates.into_iter().find(|p| p.is_file())
}
