//! Spawn the out-of-process Cursor file-link sidecar (Art. VII.8).
//!
//! Slate never embeds the Cursor runtime. This starts the Node watcher that
//! reads `request.json` and writes `session.json`.
//!
//! Discovery, `npm install`, and the spawn itself are I/O — call them from a
//! worker thread, never the frame loop (Art. II).

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Start `docs/agent/cursor-sidecar` for one portal session, if we can find
/// `node`, install `@cursor/sdk` when missing, and spawn the watcher.
/// `project_cwd` is the bound folder (the agent's cwd), not the AI workspace.
pub fn spawn_cursor_sidecar(
    ai_workspace: &Path,
    session: &str,
    project_cwd: &Path,
) -> Result<Child, String> {
    let api_key = crate::cursor_key::resolve().ok_or_else(|| {
        "Cursor API key is not set. Get one from the Cursor dashboard, then paste it in this portal."
            .to_string()
    })?;
    let script = sidecar_script().ok_or_else(|| {
        "Cursor sidecar script not found. From the repo: npm install in docs/agent/cursor-sidecar, \
or set ATLAS_CURSOR_SIDECAR to index.mjs."
            .to_string()
    })?;
    let node = resolve_node()?;
    let dir = script.parent().unwrap_or(ai_workspace);
    ensure_sidecar_deps(&node, dir)?;

    let log = crate::agent::agent_dir(ai_workspace, session).join("sidecar.log");
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
        &api_key,
        log_file,
        err_file,
    )
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
pub fn resolve_node() -> Result<PathBuf, String> {
    let mut tried = Vec::new();
    for p in node_candidates() {
        let display = p.display().to_string();
        if is_windows_store_alias(&p) {
            tried.push(format!("{display} (Windows Store alias — skipped)"));
            continue;
        }
        match std::fs::metadata(&p) {
            Ok(m) if m.is_file() => return Ok(p),
            Ok(_) => tried.push(format!("{display} (exists but is not a file)")),
            Err(e) => tried.push(format!("{display} ({e})")),
        }
    }
    Err(format!(
        "Slate could not see node.exe. Looked in:\n{}\n\
Install Node.js LTS from nodejs.org, or set ATLAS_NODE to that node.exe.",
        tried.join("\n")
    ))
}

fn spawn_node(
    node: &Path,
    script: &Path,
    ai_workspace: &Path,
    session: &str,
    project_cwd: &Path,
    api_key: &str,
    log_file: File,
    err_file: File,
) -> Result<Child, String> {
    let mut cmd = Command::new(node);
    cmd.arg(script)
        .current_dir(script.parent().unwrap_or(ai_workspace))
        .env("ATLAS_AI_WORKSPACE", ai_workspace)
        .env("ATLAS_AGENT_SESSION", session)
        .env("ATLAS_AGENT_CWD", project_cwd)
        .env("CURSOR_API_KEY", api_key)
        .stdout(log_file)
        .stderr(err_file);
    if let Some(path) = path_with_node_dir(node) {
        cmd.env("PATH", path);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn().map_err(|e| {
        format!(
            "Could not start Cursor sidecar with {}: {e}",
            node.display()
        )
    })
}

fn path_with_node_dir(node: &Path) -> Option<std::ffi::OsString> {
    let dir = node.parent()?;
    let mut entries = vec![dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        entries.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(entries).ok()
}

fn node_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    push_atlas_node(&mut out);

    #[cfg(windows)]
    {
        for p in well_known_windows_nodes() {
            push_unique(&mut out, p);
        }
        for key in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(root) = std::env::var(key) {
                push_unique(
                    &mut out,
                    PathBuf::from(root).join("nodejs").join("node.exe"),
                );
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            push_unique(
                &mut out,
                PathBuf::from(local)
                    .join("Programs")
                    .join("nodejs")
                    .join("node.exe"),
            );
        }
        if let Ok(home) = std::env::var("USERPROFILE") {
            let home = PathBuf::from(home);
            push_unique(&mut out, home.join(".volta").join("bin").join("node.exe"));
            push_unique(
                &mut out,
                home.join("AppData")
                    .join("Roaming")
                    .join("fnm")
                    .join("aliases")
                    .join("default")
                    .join("node.exe"),
            );
        }
        if let Ok(nvm) = std::env::var("NVM_SYMLINK") {
            push_unique(&mut out, PathBuf::from(nvm).join("node.exe"));
        }
    }
    #[cfg(not(windows))]
    {
        for p in [
            "/usr/local/bin/node",
            "/usr/bin/node",
            "/opt/homebrew/bin/node",
        ] {
            push_unique(&mut out, PathBuf::from(p));
        }
    }

    for p in nodes_on_env_path() {
        push_unique(&mut out, p);
    }
    out
}

fn push_atlas_node(out: &mut Vec<PathBuf>) {
    let Ok(raw) = std::env::var("ATLAS_NODE") else {
        return;
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return;
    }
    let p = PathBuf::from(raw);
    push_unique(out, p.clone());
    if p.is_dir() {
        #[cfg(windows)]
        push_unique(out, p.join("node.exe"));
        push_unique(out, p.join("node"));
    }
}

#[cfg(windows)]
fn well_known_windows_nodes() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
        PathBuf::from(r"C:\Program Files (x86)\nodejs\node.exe"),
    ]
}

fn nodes_on_env_path() -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .filter(|p| !is_windows_store_alias(p))
        .collect()
}

fn push_unique(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !out.iter().any(|p| p == &path) {
        out.push(path);
    }
}

fn is_windows_store_alias(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
}

/// `npm install` in the sidecar folder when `@cursor/sdk` is missing.
/// Uses `node` + npm's CLI script so a GUI process never has to run `npm.ps1`.
fn ensure_sidecar_deps(node: &Path, dir: &Path) -> Result<(), String> {
    if sidecar_sdk_dir(dir).is_dir() {
        return Ok(());
    }
    let npm_js = npm_cli_js(node).ok_or_else(|| {
        format!(
            "npm was not found next to {}. Reinstall Node.js LTS, then send again.",
            node.display()
        )
    })?;
    let mut cmd = Command::new(node);
    cmd.arg(&npm_js)
        .arg("install")
        .arg("--omit=dev")
        .arg("--no-fund")
        .arg("--no-audit")
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| {
        format!(
            "Could not run npm install for the Cursor sidecar with {}: {e}",
            node.display()
        )
    })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = [stderr.trim(), stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("npm install failed");
        return Err(format!("npm install failed in {}: {detail}", dir.display()));
    }
    if !sidecar_sdk_dir(dir).is_dir() {
        return Err(format!(
            "npm install finished but @cursor/sdk is still missing in {}.",
            dir.display()
        ));
    }
    Ok(())
}

fn sidecar_sdk_dir(dir: &Path) -> PathBuf {
    dir.join("node_modules").join("@cursor").join("sdk")
}

fn npm_cli_js(node: &Path) -> Option<PathBuf> {
    let dir = node.parent()?;
    let js = dir
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");
    js.is_file().then_some(js)
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
    fn well_known_windows_node_is_a_candidate_without_env() {
        let hits = node_candidates();
        #[cfg(windows)]
        {
            assert!(
                hits.iter()
                    .any(|p| p == Path::new(r"C:\Program Files\nodejs\node.exe")),
                "hardcoded Program Files path must not depend on ProgramFiles: {hits:?}"
            );
        }
        #[cfg(not(windows))]
        {
            assert!(hits.iter().any(|p| p.ends_with("node")));
        }
    }

    #[test]
    fn this_machine_program_files_node_is_found() {
        let found = resolve_node();
        #[cfg(windows)]
        {
            let well_known = PathBuf::from(r"C:\Program Files\nodejs\node.exe");
            if well_known.is_file() {
                let path = found.expect("Program Files node.exe exists — discovery must find it");
                assert!(
                    path.is_file(),
                    "resolved node must exist: {}",
                    path.display()
                );
                return;
            }
        }
        match found {
            Ok(path) => assert!(
                path.is_file(),
                "resolved node must exist: {}",
                path.display()
            ),
            Err(e) => {
                #[cfg(windows)]
                {
                    assert!(
                        !PathBuf::from(r"C:\Program Files\nodejs\node.exe").is_file(),
                        "node.exe is on disk but resolve_node failed: {e}"
                    );
                }
                assert!(
                    e.contains("Looked in:"),
                    "a miss must list the paths we tried: {e}"
                );
            }
        }
    }

    #[test]
    fn npm_cli_sits_next_to_this_machines_node() {
        let Ok(node) = resolve_node() else {
            return;
        };
        assert!(
            npm_cli_js(&node).is_some(),
            "npm-cli.js must sit next to {} so a GUI spawn can install sidecar deps",
            node.display()
        );
    }

    #[test]
    fn sidecar_sdk_is_present_when_node_modules_exists() {
        let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
        let dir = script.parent().unwrap();
        if !dir.join("node_modules").is_dir() {
            return;
        }
        assert!(
            sidecar_sdk_dir(dir).is_dir(),
            "@cursor/sdk must be installed under {}",
            dir.display()
        );
        let node = resolve_node().expect("node is present if node_modules was installed");
        ensure_sidecar_deps(&node, dir).expect("second prepare is a no-op");
    }

    #[test]
    fn sidecar_script_resolves_from_this_crate() {
        let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
        assert!(script.is_file(), "{}", script.display());
        assert!(
            script.file_name().is_some_and(|n| n == "index.mjs"),
            "{}",
            script.display()
        );
    }

    #[test]
    fn a_miss_lists_every_candidate() {
        let previous = std::env::var_os("ATLAS_NODE");
        std::env::set_var("ATLAS_NODE", r"C:\definitely\not\a\real\atlas-node.exe");
        let hits = node_candidates();
        assert!(
            hits.iter().any(|p| p.ends_with("atlas-node.exe")),
            "{hits:?}"
        );
        match previous {
            Some(v) => std::env::set_var("ATLAS_NODE", v),
            None => std::env::remove_var("ATLAS_NODE"),
        }
    }
}
