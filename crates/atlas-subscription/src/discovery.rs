//! Current-user native CLI discovery. No credential files are read or copied.
use std::{
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Cursor,
    ClaudeCode,
}

impl Provider {
    pub fn id(self) -> &'static str {
        match self {
            Self::Cursor => "cursor-cli",
            Self::ClaudeCode => "claude-code",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Cursor => "Cursor",
            Self::ClaudeCode => "Claude Code",
        }
    }
    pub fn login_command(self) -> String {
        let suffix = match self {
            Self::Cursor => "login",
            Self::ClaudeCode => "auth login --claudeai",
        };
        if let Some(path) = executable(self) {
            // PowerShell call operator and single-quoted literal survive spaces,
            // Unicode, dollar signs and apostrophes in another person's profile.
            format!(
                "& '{}' {suffix}",
                path.to_string_lossy().replace('\'', "''")
            )
        } else {
            format!(
                "{} {suffix}",
                match self {
                    Self::Cursor => "agent",
                    Self::ClaudeCode => "claude",
                }
            )
        }
    }
    pub fn install_url(self) -> &'static str {
        match self {
            Self::Cursor => "https://cursor.com/docs/cli/installation",
            Self::ClaudeCode => "https://code.claude.com/docs/en/setup",
        }
    }
    /// Child-only environment changes keep API credentials from shadowing OAuth.
    /// Home/config selectors and subscription OAuth tokens remain vendor-owned.
    pub fn command(self, executable: &Path) -> Command {
        let mut command = if self == Self::Cursor {
            cursor_native_command(executable).unwrap_or_else(|| native_command(executable))
        } else {
            native_command(executable)
        };
        for name in api_environment(self) {
            command.env_remove(name);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        command
    }
}

/// The official Windows package is Node + index.js behind shell shims. Launch
/// those exact installed components directly: no cmd expansion, execution-policy
/// bypass or PowerShell stdio re-encoding in the JSON transport.
fn cursor_native_command(launcher: &Path) -> Option<Command> {
    let root = launcher.parent()?;
    let mut installs = vec![root.to_path_buf()];
    if let Ok(entries) = std::fs::read_dir(root.join("versions")) {
        let mut versions: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(cursor_version_dir)
            })
            .collect();
        versions.sort_by_key(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let date: Vec<u32> = name
                .split('-')
                .next()
                .unwrap_or_default()
                .split('.')
                .filter_map(|part| part.parse().ok())
                .collect();
            (date, name.to_owned())
        });
        installs.extend(versions.into_iter().rev());
    }
    let install = installs
        .into_iter()
        .find(|dir| dir.join("node.exe").is_file() && dir.join("index.js").is_file())?;
    let mut command = Command::new(install.join("node.exe"));
    command.arg(install.join("index.js"));
    command.env("CURSOR_INVOKED_AS", "cursor-agent");
    Some(command)
}

fn cursor_version_dir(name: &str) -> bool {
    let Some((date, suffix)) = name.split_once('-') else {
        return false;
    };
    let parts: Vec<_> = date.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && !suffix.is_empty()
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthStatus {
    LoggedIn,
    LoginRequired,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct Discovery {
    pub executable: PathBuf,
    pub version: String,
    pub auth: AuthStatus,
    /// Normalized method only; never account identity, token or raw status text.
    pub method: Option<String>,
    pub provider: Provider,
}
impl Discovery {
    pub fn command(&self) -> Command {
        self.provider.command(&self.executable)
    }
}

fn api_environment(provider: Provider) -> &'static [&'static str] {
    match provider {
        Provider::Cursor => &["CURSOR_API_KEY"],
        Provider::ClaudeCode => &[
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_USE_FOUNDRY",
        ],
    }
}

fn native_command(executable: &Path) -> Command {
    #[cfg(windows)]
    if executable
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("ps1"))
    {
        // -File passes arguments as data. Do not change or bypass execution policy.
        let powershell = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = Command::new(powershell);
        command
            .args(["-NoProfile", "-NonInteractive", "-File"])
            .arg(executable);
        return command;
    }
    Command::new(executable)
}

/// Pure candidate construction is shared by normal desktop discovery and tests.
fn candidates(provider: Provider, env: &impl Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let override_key = match provider {
        Provider::Cursor => "ATLAS_CURSOR_AGENT",
        Provider::ClaudeCode => "ATLAS_CLAUDE_BIN",
    };
    if let Some(path) = env(override_key).filter(|path| !path.is_empty()) {
        out.push(PathBuf::from(path));
    }
    let names: &[&str] = if cfg!(windows) {
        match provider {
            Provider::Cursor => &[
                "cursor-agent.exe",
                "agent.exe",
                "cursor-agent.cmd",
                "agent.cmd",
                "cursor-agent.ps1",
                "agent.ps1",
            ],
            Provider::ClaudeCode => &["claude.exe", "claude.cmd", "claude.ps1"],
        }
    } else {
        match provider {
            Provider::Cursor => &["cursor-agent", "agent"],
            Provider::ClaudeCode => &["claude"],
        }
    };
    let mut roots = Vec::new();
    if let Some(path) = env("PATH") {
        roots.extend(std::env::split_paths(&path));
    }
    if let Some(home) = env("USERPROFILE").or_else(|| env("HOME")) {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/bin"));
        roots.push(home.join(".claude/local"));
    }
    if let Some(local) = env("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        roots.push(local.join("cursor-agent"));
        roots.push(local.join("Programs/Claude"));
    }
    if let Some(roaming) = env("APPDATA") {
        roots.push(PathBuf::from(roaming).join("npm"));
    }
    for root in roots {
        for name in names {
            let path = root.join(name);
            if !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

pub fn executable(provider: Provider) -> Option<PathBuf> {
    candidates(provider, &|key| std::env::var_os(key))
        .into_iter()
        .find(|path| path.is_file())
}

/// Worker-only, bounded read-only probes. No paid model generation or login.
pub fn discover(provider: Provider) -> Result<Discovery, String> {
    let executable = executable(provider).ok_or_else(|| format!(
        "{} native CLI is not installed. Install it from {} then run {}. WSL-only installs are not supported by this connection.",
        provider.label(), provider.install_url(), provider.login_command()
    ))?;
    let version = output(provider.command(&executable), &["--version"])?;
    if !version.success {
        return Err(format!(
            "{} CLI could not report its version. Update or reinstall the native CLI.",
            provider.label()
        ));
    }
    let help = output(provider.command(&executable), &["--help"])?;
    let expected = match provider {
        Provider::Cursor => "cursor",
        Provider::ClaudeCode => "claude",
    };
    if !help.success || !help.stdout.to_ascii_lowercase().contains(expected) {
        return Err(format!("The discovered command is not a supported {} CLI. Update the native CLI or set {} to its executable.", provider.label(), match provider { Provider::Cursor => "ATLAS_CURSOR_AGENT", Provider::ClaudeCode => "ATLAS_CLAUDE_BIN" }));
    }
    let args: &[&str] = match provider {
        Provider::Cursor => &["status"],
        Provider::ClaudeCode => &["auth", "status"],
    };
    let status = output(provider.command(&executable), args)?;
    let (auth, method) = auth_status(provider, status.success, &status.stdout, &status.stderr);
    Ok(Discovery {
        provider,
        executable,
        version: version.stdout.trim().chars().take(160).collect(),
        auth,
        method,
    })
}

fn auth_status(
    provider: Provider,
    success: bool,
    stdout: &str,
    stderr: &str,
) -> (AuthStatus, Option<String>) {
    if provider == Provider::ClaudeCode {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) {
            let method = value["authMethod"]
                .as_str()
                .filter(|method| {
                    matches!(
                        *method,
                        "none"
                            | "claude.ai"
                            | "oauth_token"
                            | "api_key"
                            | "api_key_helper"
                            | "third_party"
                    )
                })
                .map(str::to_owned);
            let logged_in = value["loggedIn"].as_bool();
            let subscription = matches!(method.as_deref(), Some("claude.ai" | "oauth_token"));
            let auth = if success && logged_in == Some(true) && subscription {
                AuthStatus::LoggedIn
            } else if logged_in == Some(false)
                || matches!(
                    method.as_deref(),
                    Some("none" | "api_key" | "api_key_helper" | "third_party")
                )
            {
                AuthStatus::LoginRequired
            } else {
                AuthStatus::Unknown
            };
            return (auth, method);
        }
    }
    let text = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if [
        "not logged in",
        "not authenticated",
        "unauthenticated",
        "login required",
        "please log in",
        "please login",
    ]
    .iter()
    .any(|word| text.contains(word))
    {
        return (AuthStatus::LoginRequired, None);
    }
    if provider == Provider::Cursor && (text.contains("api key") || text.contains("api-key")) {
        return (AuthStatus::LoginRequired, Some("api_key".into()));
    }
    if provider == Provider::Cursor
        && success
        && ["logged in", "authenticated"]
            .iter()
            .any(|word| text.contains(word))
    {
        return (AuthStatus::LoggedIn, Some("cursor_login".into()));
    }
    (AuthStatus::Unknown, None)
}

struct ProbeOutput {
    success: bool,
    stdout: String,
    stderr: String,
}
fn output(mut command: Command, args: &[&str]) -> Result<ProbeOutput, String> {
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| format!("Could not start the native provider CLI: {error}. Check its installation and Windows execution policy."))?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let out = std::thread::spawn(move || drain(stdout));
    let err = std::thread::spawn(move || drain(stderr));
    let deadline = Instant::now() + Duration::from_secs(12);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("The native provider CLI connection check timed out. Check the CLI in a terminal and retry.".into());
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Could not check the native provider CLI: {error}"));
            }
        }
    };
    Ok(ProbeOutput {
        success: status.success(),
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}
fn drain(mut input: impl Read) -> String {
    let mut retained = Vec::new();
    let mut buffer = [0; 4096];
    while let Ok(n) = input.read(&mut buffer) {
        if n == 0 {
            break;
        }
        let keep = n.min(262_144_usize.saturating_sub(retained.len()));
        retained.extend_from_slice(&buffer[..keep]);
    }
    String::from_utf8_lossy(&retained).into_owned()
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
