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
    let api_key = crate::cursor_key::resolve().ok_or_else(|| {
        "Cursor API key is not set. Get one from the Cursor dashboard, then paste it in this portal."
            .to_string()
    })?;
    let script = sidecar_script().ok_or_else(|| {
        "Cursor sidecar script not found. From the repo: npm install in docs/agent/cursor-sidecar, \
or set ATLAS_CURSOR_SIDECAR to index.mjs."
            .to_string()
    })?;
    let node = node_exe().ok_or_else(|| {
        "Slate could not see node.exe (a GUI launch often misses the terminal PATH). \
If Node is installed, set ATLAS_NODE to node.exe, or install it from nodejs.org."
            .to_string()
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
            .env("CURSOR_API_KEY", &api_key)
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
            .env("CURSOR_API_KEY", &api_key)
            .stdout(log_file)
            .stderr(err_file)
            .spawn()
            .map_err(|e| format!("Could not start Cursor sidecar: {e}"))
    }
}

/// Human setup steps next to the sidecar script, when we can find them.
pub fn setup_doc() -> Option<PathBuf> {
    let script = sidecar_script()?;
    let doc = script.parent()?.join("SETUP.md");
    doc.is_file().then_some(doc)
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

/// Absolute `node` / `node.exe`. GUI apps on Windows often inherit a PATH
/// that never saw the installer, so `where node` is not enough — we look in
/// the usual install folders first.
fn node_exe() -> Option<PathBuf> {
    node_candidates()
        .into_iter()
        .find(|p| usable_node(p))
        .or_else(node_on_path)
}

fn usable_node(path: &Path) -> bool {
    path.is_file() || path.exists()
}

fn node_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(p) = std::env::var("ATLAS_NODE") {
        let p = p.trim();
        if !p.is_empty() {
            out.push(PathBuf::from(p));
        }
    }
    #[cfg(windows)]
    {
        if let Ok(root) = std::env::var("ProgramFiles") {
            out.push(PathBuf::from(root).join("nodejs").join("node.exe"));
        }
        if let Ok(root) = std::env::var("ProgramFiles(x86)") {
            out.push(PathBuf::from(root).join("nodejs").join("node.exe"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            out.push(
                PathBuf::from(local)
                    .join("Programs")
                    .join("nodejs")
                    .join("node.exe"),
            );
        }
        if let Ok(home) = std::env::var("USERPROFILE") {
            out.push(
                PathBuf::from(home)
                    .join(".volta")
                    .join("bin")
                    .join("node.exe"),
            );
        }
        if let Ok(nvm) = std::env::var("NVM_SYMLINK") {
            out.push(PathBuf::from(nvm).join("node.exe"));
        }
    }
    #[cfg(not(windows))]
    {
        out.push(PathBuf::from("/usr/local/bin/node"));
        out.push(PathBuf::from("/usr/bin/node"));
    }
    out
}

fn node_on_path() -> Option<PathBuf> {
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
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .find(|p| usable_node(p) && !is_windows_store_alias(p))
            .or_else(|| {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(PathBuf::from)
                    .find(|p| usable_node(p))
            })
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
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .find(|p| usable_node(p))
    }
}

#[cfg(windows)]
fn is_windows_store_alias(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_windows_node_is_a_candidate() {
        let hits = node_candidates();
        #[cfg(windows)]
        {
            assert!(
                hits.iter().any(|p| p
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains(r"nodejs\node.exe")),
                "GUI launches miss PATH — we have to look in Program Files: {hits:?}"
            );
        }
        #[cfg(not(windows))]
        {
            assert!(hits.iter().any(|p| p.ends_with("node")));
        }
    }

    #[test]
    fn node_exe_finds_program_files_install_when_present() {
        let Some(found) = node_exe() else {
            return;
        };
        assert!(
            usable_node(&found),
            "resolved node must exist: {}",
            found.display()
        );
    }
}
