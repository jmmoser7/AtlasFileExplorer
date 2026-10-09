//! One hidden PowerShell host for Office PDF adapters. Word and PowerPoint
//! share an application-wide lock so two conversions cannot drive COM at once.
//!
//! A script reports each Office process it started as a
//! `SLATE_OFFICE_STARTED=<pid>` line on stdout. On timeout the helper is
//! killed, and so are those processes — only those, and only while that PID
//! still names the expected image. An instance the user already had open is
//! never reported, so it is never touched.

use std::path::Path;
use std::time::Duration;

pub(crate) enum HostFail {
    Spawn(String),
    Failed(String),
    Timeout,
    Wait(String),
    Stopped,
}

const STARTED: &str = "SLATE_OFFICE_STARTED=";

/// The PID in a `SLATE_OFFICE_STARTED=<pid>` line, if this is one.
fn started_pid(line: &str) -> Option<u32> {
    line.trim()
        .strip_prefix(STARTED)?
        .trim()
        .parse()
        .ok()
        .filter(|pid| *pid != 0)
}

/// `image` is the executable the script launches, e.g. `WINWORD.EXE`.
pub(crate) fn run(
    script: &str,
    env: &[(&str, &Path)],
    image: &str,
    timeout: Duration,
) -> Result<(), HostFail> {
    use std::io::{BufRead, Read};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::{Arc, Mutex, OnceLock};
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HostFail::Spawn(e.to_string()))?;
    let started: Arc<Mutex<Vec<u32>>> = Arc::default();
    if let Some(stdout) = child.stdout.take() {
        let started = Arc::clone(&started);
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Some(pid) = started_pid(&line) {
                    if let Ok(mut pids) = started.lock() {
                        pids.push(pid);
                    }
                }
            }
        });
    }
    let stop = |child: &mut std::process::Child| {
        let _ = child.kill();
        let _ = child.wait();
        let pids = started.lock().map(|p| p.clone()).unwrap_or_default();
        for pid in pids {
            kill_started(pid, image);
        }
    };
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
                stop(&mut child);
                return Err(HostFail::Timeout);
            }
            Err(e) => {
                stop(&mut child);
                return Err(HostFail::Wait(e.to_string()));
            }
        }
    }
}

/// Force-end `pid` only if it is still `image`. The filter guards against a
/// PID the system has since reused for another program.
fn kill_started(pid: u32, image: &str) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("taskkill.exe")
        .args([
            "/PID",
            &pid.to_string(),
            "/FI",
            &format!("IMAGENAME eq {image}"),
            "/F",
        ])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_started_line_names_a_pid() {
        assert_eq!(started_pid("SLATE_OFFICE_STARTED=4242"), Some(4242));
        assert_eq!(started_pid("  SLATE_OFFICE_STARTED= 17 \r"), Some(17));
        assert_eq!(started_pid("SLATE_OFFICE_STARTED=0"), None);
        assert_eq!(started_pid("SLATE_OFFICE_STARTED=word"), None);
        assert_eq!(started_pid("4242"), None);
        assert_eq!(started_pid("note: SLATE_OFFICE_STARTED=4242"), None);
    }

    /// The image filter is what keeps a reused PID safe: a process that is
    /// not the expected program survives, the expected one does not.
    #[test]
    fn a_started_pid_is_killed_only_while_it_is_the_expected_image() {
        use std::os::windows::process::CommandExt;
        let mut child = std::process::Command::new("ping.exe")
            .args(["-n", "60", "127.0.0.1"])
            .creation_flags(0x08000000)
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("ping starts");
        let pid = child.id();
        kill_started(pid, "WINWORD.EXE");
        std::thread::sleep(Duration::from_millis(300));
        assert!(
            child.try_wait().unwrap().is_none(),
            "a process that is not Word is left alone"
        );
        kill_started(pid, "PING.EXE");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let ended = child.try_wait().unwrap().is_some();
        if !ended {
            let _ = child.kill();
        }
        assert!(ended, "the started process is ended");
    }
}
