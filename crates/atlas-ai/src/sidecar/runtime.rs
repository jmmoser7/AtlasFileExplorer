use super::*;
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
    pub(super) fn command(&self) -> Command {
        let mut cmd = Command::new(&self.exe);
        if self.electron {
            cmd.env("ELECTRON_RUN_AS_NODE", "1");
        }
        cmd
    }

    /// The PowerShell line a person can paste to run `args` the same way.
    pub(super) fn shell_line(&self, args: &str) -> String {
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

pub(super) fn runtime_from(
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
pub(super) fn spawn_node(
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

pub(super) fn path_with_node_dir(node: &Path) -> Option<std::ffi::OsString> {
    let dir = node.parent()?;
    let mut entries = vec![dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        entries.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(entries).ok()
}

pub(super) fn node_candidates() -> Vec<PathBuf> {
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

pub(super) fn push_atlas_node(out: &mut Vec<PathBuf>, raw: Option<&str>) {
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
pub(super) fn cursor_helpers_node(install: &Path) -> PathBuf {
    install
        .join("resources")
        .join("app")
        .join("resources")
        .join("helpers")
        .join("node.exe")
}

#[cfg(windows)]
pub(super) fn well_known_windows_nodes() -> Vec<PathBuf> {
    vec![
        PathBuf::from(r"C:\Program Files\nodejs\node.exe"),
        PathBuf::from(r"C:\Program Files (x86)\nodejs\node.exe"),
    ]
}

pub(super) fn nodes_on_env_path() -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .filter(|p| !is_windows_store_alias(p))
        .collect()
}

pub(super) fn push_unique(out: &mut Vec<PathBuf>, path: PathBuf) {
    if !out.iter().any(|p| p == &path) {
        out.push(path);
    }
}

pub(super) fn is_windows_store_alias(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("WindowsApps"))
}

/// Installs the sidecar's locked packages when `@cursor/sdk` is missing.
/// `install.mjs` downloads and checks them itself, so neither npm nor a
/// system-wide Node install is needed. Proxy variables are honoured.
pub(super) fn ensure_sidecar_deps(node: &NodeRuntime, dir: &Path) -> Result<(), String> {
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

pub(super) fn sidecar_sdk_dir(dir: &Path) -> PathBuf {
    dir.join("node_modules").join("@cursor").join("sdk")
}

pub(super) fn sidecar_script() -> Option<PathBuf> {
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
