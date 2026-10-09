//! One hidden PowerShell host for Office PDF adapters. Word and PowerPoint
//! share an application-wide lock so two conversions cannot drive COM at once.

use std::path::Path;
use std::time::Duration;

pub(crate) enum HostFail {
    Spawn(String),
    Failed(String),
    Timeout,
    Wait(String),
    Stopped,
}

pub(crate) fn run(script: &str, env: &[(&str, &Path)], timeout: Duration) -> Result<(), HostFail> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    static AUTOMATION: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = AUTOMATION
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| HostFail::Stopped)?;
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-STA",
        "-WindowStyle",
        "Hidden",
        "-Command",
        script,
    ]);
    for (key, path) in env {
        cmd.env(key, path);
    }
    let mut child = cmd
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HostFail::Spawn(e.to_string()))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let mut detail = String::new();
                if let Some(stderr) = child.stderr.take() {
                    let _ = stderr.take(4096).read_to_string(&mut detail);
                }
                return Err(HostFail::Failed(detail.trim().to_string()));
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HostFail::Timeout);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(HostFail::Wait(e.to_string()));
            }
        }
    }
}
