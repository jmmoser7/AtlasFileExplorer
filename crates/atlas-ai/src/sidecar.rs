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
        access_env(policy, crate::access::granted(session)),
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
Install Cursor (it brings its own Node) or Node.js LTS from nodejs.org, or set ATLAS_NODE to that node.exe.",
        tried.join("\n")
    ))
}

/// What runs the sidecar: a `node` executable, or `Cursor.exe` as Node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRuntime {
    pub exe: PathBuf,
    /// `exe` is Cursor's Electron binary, run with `ELECTRON_RUN_AS_NODE=1`.
    pub electron: bool,
}

impl NodeRuntime {
    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.exe);
        if self.electron {
            cmd.env("ELECTRON_RUN_AS_NODE", "1");
        }
        cmd
    }

    /// The PowerShell line a person can paste to run `args` the same way.
    fn shell_line(&self, args: &str) -> String {
        let env = if self.electron {
            "$env:ELECTRON_RUN_AS_NODE=1; "
        } else {
            ""
        };
        format!("{env}& \"{}\" {args}", self.exe.display())
    }
}

/// [`resolve_node`], else the installed `Cursor.exe` run as Node, so a
/// machine with Cursor needs no separate Node install.
pub fn resolve_runtime() -> Result<NodeRuntime, String> {
    runtime_from(resolve_node(), crate::launch::cursor_exe())
}

fn runtime_from(
    node: Result<PathBuf, String>,
    cursor: Option<PathBuf>,
) -> Result<NodeRuntime, String> {
    match (node, cursor) {
        (Ok(exe), _) => Ok(NodeRuntime {
            exe,
            electron: false,
        }),
        (Err(_), Some(exe)) => Ok(NodeRuntime {
            exe,
            electron: true,
        }),
        (Err(e), None) => Err(e),
    }
}

#[allow(clippy::too_many_arguments)] // Process inputs and redirected streams are explicit.
fn spawn_node(
    node: &NodeRuntime,
    script: &Path,
    ai_workspace: &Path,
    session: &str,
    project_cwd: &Path,
    link_dir: &Path,
    api_key: &str,
    access: [(&str, &str); 2],
    log_file: File,
    err_file: File,
) -> Result<Child, String> {
    let mut cmd = node.command();
    cmd.arg(script)
        .current_dir(script.parent().unwrap_or(ai_workspace))
        .env("ATLAS_AI_WORKSPACE", ai_workspace)
        .env("ATLAS_AGENT_SESSION", session)
        .env("ATLAS_AGENT_CWD", project_cwd)
        .env("ATLAS_AGENT_LINK_DIR", link_dir)
        .env("ATLAS_PARENT_PID", std::process::id().to_string())
        .envs(access)
        .env("CURSOR_API_KEY", api_key)
        .stdout(log_file)
        .stderr(err_file);
    if let Some(path) = path_with_node_dir(&node.exe).filter(|_| !node.electron) {
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
            node.exe.display()
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
    push_atlas_node(&mut out, std::env::var("ATLAS_NODE").ok().as_deref());
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push_unique(
                &mut out,
                dir.join("runtime")
                    .join(if cfg!(windows) { "node.exe" } else { "node" }),
            );
        }
    }

    #[cfg(windows)]
    {
        for p in well_known_windows_nodes() {
            push_unique(&mut out, p);
        }
        for key in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(root) = std::env::var(key) {
                let root = PathBuf::from(root);
                push_unique(&mut out, root.join("nodejs").join("node.exe"));
                push_unique(&mut out, cursor_helpers_node(&root.join("cursor")));
                push_unique(&mut out, cursor_helpers_node(&root.join("Cursor")));
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            push_unique(
                &mut out,
                local.join("Programs").join("nodejs").join("node.exe"),
            );
            // Cursor's desktop install bundles Node here. A Slate window
            // started outside Cursor does not inherit that folder on PATH.
            push_unique(
                &mut out,
                cursor_helpers_node(&local.join("Programs").join("cursor")),
            );
            push_unique(
                &mut out,
                cursor_helpers_node(&local.join("Programs").join("Cursor")),
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

fn push_atlas_node(out: &mut Vec<PathBuf>, raw: Option<&str>) {
    let Some(raw) = raw else {
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
fn cursor_helpers_node(install: &Path) -> PathBuf {
    install
        .join("resources")
        .join("app")
        .join("resources")
        .join("helpers")
        .join("node.exe")
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

/// Installs the sidecar's locked packages when `@cursor/sdk` is missing.
/// `install.mjs` downloads and checks them itself, so neither npm nor a
/// system-wide Node install is needed. Proxy variables are honoured.
fn ensure_sidecar_deps(node: &NodeRuntime, dir: &Path) -> Result<(), String> {
    if sidecar_sdk_dir(dir).is_dir() {
        return Ok(());
    }
    let mut cmd = node.command();
    cmd.arg("install.mjs")
        .current_dir(dir)
        .env("NODE_USE_ENV_PROXY", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let failed = |detail: &str| {
        format!(
            "Could not install the Cursor sidecar packages: {detail}\n\
To install them by hand, run in PowerShell:\n  cd \"{}\"\n  {}\nthen send again.",
            dir.display(),
            node.shell_line("install.mjs")
        )
    };
    let out = cmd.output().map_err(|e| failed(&e.to_string()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let detail = [stderr.trim(), stdout.trim()]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or("the installer stopped without a message");
        let detail = detail
            .strip_prefix("Could not install the Cursor sidecar packages: ")
            .unwrap_or(detail);
        return Err(failed(detail));
    }
    if !sidecar_sdk_dir(dir).is_dir() {
        return Err(failed("@cursor/sdk is still missing after the install"));
    }
    Ok(())
}

fn sidecar_sdk_dir(dir: &Path) -> PathBuf {
    dir.join("node_modules").join("@cursor").join("sdk")
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
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                let bundled = PathBuf::from(local)
                    .join("Programs")
                    .join("cursor")
                    .join("resources")
                    .join("app")
                    .join("resources")
                    .join("helpers")
                    .join("node.exe");
                assert!(
                    hits.iter().any(|p| p == &bundled),
                    "Cursor's bundled node must be a candidate: {hits:?}"
                );
            }
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
    fn without_node_cursor_runs_the_sidecar_as_node() {
        let cursor = PathBuf::from("Cursor/Cursor.exe");
        let node = PathBuf::from("nodejs/node.exe");
        let found = runtime_from(Ok(node.clone()), Some(cursor.clone())).unwrap();
        assert_eq!(
            found,
            NodeRuntime {
                exe: node,
                electron: false
            }
        );
        let fallback = runtime_from(Err("no node".into()), Some(cursor.clone())).unwrap();
        assert_eq!(
            fallback,
            NodeRuntime {
                exe: cursor,
                electron: true
            }
        );
        let cmd = fallback.command();
        assert!(cmd
            .get_envs()
            .any(|(k, v)| k == "ELECTRON_RUN_AS_NODE" && v == Some("1".as_ref())));
        assert!(fallback
            .shell_line("install.mjs")
            .starts_with("$env:ELECTRON_RUN_AS_NODE=1; & \""));
        assert_eq!(
            runtime_from(Err("Looked in: x".into()), None),
            Err("Looked in: x".into())
        );
    }

    #[test]
    fn this_machine_cursor_runs_scripts_as_a_new_enough_node() {
        let Some(exe) = crate::launch::cursor_exe() else {
            return;
        };
        let out = NodeRuntime {
            exe,
            electron: true,
        }
        .command()
        .args(["-e", "process.stdout.write(process.versions.node)"])
        .output()
        .unwrap();
        let version = String::from_utf8_lossy(&out.stdout).into_owned();
        let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();
        assert!(
            parts.len() == 3 && (parts[0], parts[1]) >= (22, 13),
            "@cursor/sdk needs Node 22.13 or newer, Cursor runs {version:?}"
        );
    }

    #[test]
    fn sidecar_js_tests_pass() {
        let Ok(node) = resolve_runtime() else {
            return;
        };
        let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
        let dir = script.parent().unwrap();
        let mut tests: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| name.ends_with(".test.mjs"))
            .collect();
        tests.sort();
        assert!(tests.iter().any(|t| t == "install.test.mjs"), "{tests:?}");
        let out = node
            .command()
            .arg("--test")
            .args(&tests)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn watchers_are_node_or_cursor_processes() {
        assert_eq!(watcher_image("\"node.exe\""), Some(Watcher::Node));
        assert_eq!(watcher_image("node"), Some(Watcher::Node));
        assert_eq!(watcher_image("\"Cursor.exe\""), Some(Watcher::Electron));
        assert_eq!(watcher_image("\"explorer.exe\""), None);
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
        // `node_modules` can outlive the Node that installed it — it is checked
        // in on one machine and cloned onto another.
        let Ok(node) = resolve_runtime() else {
            return;
        };
        ensure_sidecar_deps(&node, dir).expect("second prepare is a no-op");
    }

    #[test]
    fn a_missing_sdk_installs_without_npm() {
        let Ok(node) = resolve_runtime() else {
            return;
        };
        let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
        let dir =
            std::env::temp_dir().join(format!("atlas-sidecar-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(
            script.with_file_name("install.mjs"),
            dir.join("install.mjs"),
        )
        .unwrap();
        std::fs::write(
            dir.join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{},"node_modules/@cursor/sdk":{
                "version":"1.0.28",
                "resolved":"http://127.0.0.1:1/@cursor/sdk/-/sdk-1.0.28.tgz",
                "integrity":"sha512-AAAA"}}}"#,
        )
        .unwrap();
        let err = ensure_sidecar_deps(&node, &dir).unwrap_err();
        assert!(
            err.contains("Could not install the Cursor sidecar packages"),
            "{err}"
        );
        assert!(err.contains("127.0.0.1:1"), "names the download: {err}");
        assert!(
            err.contains("install.mjs") && !err.contains("npm"),
            "manual steps need no npm: {err}"
        );
        assert!(!dir.join("node_modules").exists());
        let _ = std::fs::remove_dir_all(&dir);
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
    fn sidecar_guide_twin_matches_the_rust_guide() {
        let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
        let js = std::fs::read_to_string(script.with_file_name("artifacts.mjs")).unwrap();
        let guide = atlas_agent::artifact_guide(Some("{DIR}"));
        let (head, rest) = guide.split_once("Save new files").unwrap();
        let (folder, rest) = rest.split_once("Edit existing").unwrap();
        let (_, tail) = rest.split_once("return.json beside session.json").unwrap();
        for part in [head, folder.split_once("{DIR}").unwrap().1, tail] {
            assert!(
                js.contains(part),
                "artifacts.mjs drifted from artifact_guide: {part}"
            );
        }
    }

    #[test]
    fn a_read_only_sidecar_never_starts_with_full_access() {
        let env = |read_only, granted| access_env(SidecarPolicy { read_only }, granted);
        assert_eq!(
            env(false, true),
            [
                ("ATLAS_AGENT_FULL_ACCESS", "1"),
                ("ATLAS_AGENT_READ_ONLY", "0")
            ]
        );
        assert_eq!(env(false, false)[0], ("ATLAS_AGENT_FULL_ACCESS", "0"));
        for granted in [false, true] {
            assert_eq!(
                env(true, granted),
                [
                    ("ATLAS_AGENT_FULL_ACCESS", "0"),
                    ("ATLAS_AGENT_READ_ONLY", "1")
                ],
                "{granted}"
            );
        }
        assert!(!SidecarPolicy::default().read_only);
    }

    #[test]
    fn sidecar_goal_guide_twin_matches_the_rust_guide() {
        let script = sidecar_script().expect("docs/agent/cursor-sidecar/index.mjs must ship");
        let js = std::fs::read_to_string(script.with_file_name("artifacts.mjs")).unwrap();
        for (claims, in_reply) in [(true, false), (false, false), (true, true), (false, true)] {
            let goal = atlas_agent::GoalTurn {
                goal: "{GOAL}".into(),
                claims,
                in_reply,
            };
            let guide = atlas_agent::goal_guide_in(&goal, None);
            for part in guide
                .split(['\n'])
                .flat_map(|l| l.split("{GOAL}"))
                .flat_map(|l| l.split("return.json beside session.json"))
                .filter(|p| !p.is_empty())
            {
                assert!(
                    js.contains(part),
                    "artifacts.mjs drifted from goal_guide_in: {part}"
                );
            }
        }
        assert!(js.contains(&format!(
            "'{}'",
            atlas_agent::GOAL_MARKER.replace('\n', "\\n")
        )));
    }

    #[test]
    fn stop_leaves_stale_or_foreign_pids_alone() {
        let dir = std::env::temp_dir().join(format!("atlas-sidecar-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(stop(&dir), Ok(false), "no record");
        for text in ["not a pid", &std::process::id().to_string(), "4294967"] {
            std::fs::write(dir.join("sidecar.pid"), text).unwrap();
            assert_eq!(stop(&dir), Ok(false), "{text}");
            assert!(!dir.join("sidecar.pid").exists(), "{text}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explicit_override_is_retained_even_when_missing() {
        let missing = "missing-atlas-runtime/atlas-node.exe";
        let mut hits = Vec::new();
        push_atlas_node(&mut hits, Some(missing));
        assert_eq!(hits, vec![PathBuf::from(missing)]);
    }
}
