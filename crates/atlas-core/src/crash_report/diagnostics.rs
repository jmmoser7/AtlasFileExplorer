//! Local crash classifications and the separate, allowlisted submission.

use super::{capture, storage, GraphicsCheckpoint};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const REPORT_READ_LIMIT: u64 = 2 * 1024 * 1024;
const NATIVE_READ_LIMIT: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReleaseChannel {
    Development,
    Preview,
    Stable,
}

impl ReleaseChannel {
    pub(super) fn current() -> Self {
        match option_env!("ATLAS_RELEASE_CHANNEL") {
            Some("stable") => Self::Stable,
            Some("preview") => Self::Preview,
            _ => Self::Development,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum SupervisorFailure {
    Launch,
    Wait,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Completion {
    ProcessExit,
    LaunchFailed,
    WaitFailed,
    Interrupted,
    Unknown,
}

impl From<SupervisorFailure> for Completion {
    fn from(stage: SupervisorFailure) -> Self {
        match stage {
            SupervisorFailure::Launch => Self::LaunchFailed,
            SupervisorFailure::Wait => Self::WaitFailed,
        }
    }
}

impl Completion {
    fn runtime_ms(self, started: u64, ended: u64) -> Option<u64> {
        match self {
            Self::ProcessExit => ended.checked_sub(started),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
struct Evidence {
    native_exception: bool,
    session_checkpoint: bool,
    stderr: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum View {
    Home,
    Board,
    Grid,
    Venn,
    Lens,
    Branch,
    Present,
}

#[derive(Debug, Serialize)]
struct NativeFingerprint {
    exception_code: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    module: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    module_offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub(super) struct RunDiagnostics {
    completion: Completion,
    duration_ms: Option<u64>,
    evidence: Evidence,
    last_view: Option<View>,
    peak_frame_ms: Option<u32>,
    native: Option<NativeFingerprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    graphics: Option<GraphicsCheckpoint>,
}

impl RunDiagnostics {
    pub(super) fn collect(
        dir: &Path,
        completion: Completion,
        started: u64,
        ended: u64,
        native_exception: bool,
        session: Option<&serde_json::Value>,
        stderr: bool,
    ) -> Self {
        Self {
            completion,
            duration_ms: completion.runtime_ms(started, ended),
            evidence: Evidence {
                native_exception,
                session_checkpoint: session.is_some(),
                stderr,
            },
            last_view: session.and_then(session_view),
            peak_frame_ms: session.and_then(peak_frame_ms),
            native: native_exception.then(|| native_fingerprint(dir)).flatten(),
            graphics: GraphicsCheckpoint::read(dir),
        }
    }
}

/// The only crash fields intended for a user's explicit manual submission.
/// No raw diagnostic text, file paths, process identifiers, or memory addresses.
#[derive(Debug, Serialize)]
pub struct CrashSubmission {
    schema_version: u32,
    app: &'static str,
    version: Option<String>,
    release_channel: Option<ReleaseChannel>,
    platform: Option<&'static str>,
    completion: Completion,
    duration_seconds: Option<u64>,
    success: Option<bool>,
    exit_code: Option<i32>,
    panic_count: usize,
    evidence: Evidence,
    last_view: Option<View>,
    peak_frame_ms: Option<u32>,
    native: Option<NativeFingerprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    graphics: Option<GraphicsCheckpoint>,
}

/// Read a completed report directory selected from `recent_reports`.
/// Existing version-one reports can be summarized without rewriting them.
pub fn submission_summary(report_dir: &Path) -> io::Result<CrashSubmission> {
    if report_dir.join(".active").exists() {
        return Err(io::Error::other("crash report is still active"));
    }
    let report_path = report_dir.join("report.json");
    let report: serde_json::Value = match File::open(report_path) {
        Ok(file) => {
            serde_json::from_reader(file.take(REPORT_READ_LIMIT)).map_err(io::Error::other)?
        }
        Err(error)
            if error.kind() == io::ErrorKind::NotFound && storage::has_evidence(report_dir) =>
        {
            serde_json::Value::Null
        }
        Err(error) => return Err(error),
    };
    let run = capture::read_run(&report_dir.join("run.json"));
    let app = match report["app"]
        .as_str()
        .or_else(|| run.as_ref().map(|run| run.app.as_str()))
    {
        Some("slate") => "slate",
        Some("file-atlas") => "file-atlas",
        _ => "unknown",
    };
    let version = report["version"]
        .as_str()
        .or_else(|| run.as_ref().map(|run| run.version.as_str()))
        .and_then(release_version)
        .map(str::to_owned);
    let platform = report["platform"]
        .as_str()
        .or_else(|| run.as_ref().map(|run| run.platform.as_str()))
        .and_then(platform);
    let release_channel = serde_json::from_value(report["release_channel"].clone())
        .ok()
        .or_else(|| run.as_ref().and_then(|run| run.release_channel));
    let exit_code = report["exit_code"]
        .as_i64()
        .and_then(|code| code.try_into().ok());
    let completion: Completion =
        serde_json::from_value(report["diagnostics"]["completion"].clone()).unwrap_or_else(|_| {
            if exit_code.is_some() {
                Completion::ProcessExit
            } else {
                Completion::Unknown
            }
        });
    let duration_seconds = report["started_unix_ms"]
        .as_u64()
        .zip(report["ended_unix_ms"].as_u64())
        .and_then(|(start, end)| completion.runtime_ms(start, end))
        .map(|duration| duration / 1000);
    let native_exception = report["native_exception"].as_bool().unwrap_or(false)
        || storage::has_native_exception(report_dir);
    let session = (!report["session_latest"].is_null()).then_some(&report["session_latest"]);
    let stderr = report["stderr_tail"]
        .as_str()
        .is_some_and(|tail| !tail.is_empty());

    Ok(CrashSubmission {
        schema_version: 1,
        app,
        version,
        release_channel,
        platform,
        completion,
        duration_seconds,
        success: report["success"].as_bool(),
        exit_code,
        panic_count: storage::panic_count(report_dir).min(32),
        evidence: Evidence {
            native_exception,
            session_checkpoint: session.is_some(),
            stderr,
        },
        last_view: session.and_then(session_view),
        peak_frame_ms: session.and_then(peak_frame_ms),
        native: native_exception
            .then(|| native_fingerprint(report_dir))
            .flatten(),
        graphics: GraphicsCheckpoint::from_report(&report["diagnostics"]["graphics"])
            .or_else(|| GraphicsCheckpoint::read(report_dir)),
    })
}

fn release_version(value: &str) -> Option<&str> {
    let (base, preview) = value.split_once("-preview.").unwrap_or((value, ""));
    let numeric = |field: &str| {
        field.len() <= 9
            && field.parse::<u32>().is_ok()
            && field.bytes().all(|byte| byte.is_ascii_digit())
    };
    (base.split('.').count() == 3
        && base.split('.').all(numeric)
        && (base == value || numeric(preview)))
    .then_some(value)
}

fn platform(value: &str) -> Option<&'static str> {
    match value {
        "windows-x86_64" => Some("windows-x86_64"),
        "windows-aarch64" => Some("windows-aarch64"),
        "windows-x86" => Some("windows-x86"),
        "linux-x86_64" => Some("linux-x86_64"),
        "linux-aarch64" => Some("linux-aarch64"),
        "macos-x86_64" => Some("macos-x86_64"),
        "macos-aarch64" => Some("macos-aarch64"),
        _ => None,
    }
}

fn session_view(session: &serde_json::Value) -> Option<View> {
    match session["now"]["view"].as_str()? {
        "home" => Some(View::Home),
        "board" => Some(View::Board),
        "grid" => Some(View::Grid),
        "venn" => Some(View::Venn),
        "lens" => Some(View::Lens),
        "branch" => Some(View::Branch),
        "present" => Some(View::Present),
        _ => None,
    }
}

fn peak_frame_ms(session: &serde_json::Value) -> Option<u32> {
    let value = session["frame"]["max_ms"].as_f64()?;
    (0.0..=60_000.0)
        .contains(&value)
        .then(|| value.round() as u32)
}

fn native_fingerprint(dir: &Path) -> Option<NativeFingerprint> {
    let text = capture::read_tail(&dir.join("native-exception.txt"), NATIVE_READ_LIMIT).ok()?;
    let field = |key| {
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find_map(|(name, value)| (name == key).then_some(value))
    };
    let exception_code =
        u32::from_str_radix(field("exception_code")?.strip_prefix("0x")?, 16).ok()?;
    let module = field("module").and_then(|value| match value.to_ascii_lowercase().as_str() {
        "slate.exe" => Some("slate.exe"),
        "ntdll.dll" => Some("ntdll.dll"),
        "kernel32.dll" => Some("kernel32.dll"),
        "kernelbase.dll" => Some("kernelbase.dll"),
        "ucrtbase.dll" => Some("ucrtbase.dll"),
        "opengl32.dll" => Some("opengl32.dll"),
        "d3d11.dll" => Some("d3d11.dll"),
        "dxgi.dll" => Some("dxgi.dll"),
        "pdfium.dll" => Some("pdfium.dll"),
        "webview2loader.dll" => Some("webview2loader.dll"),
        _ => None,
    });
    let module_offset = module
        .and_then(|_| field("module_offset")?.strip_prefix("0x"))
        .and_then(|offset| u64::from_str_radix(offset, 16).ok())
        .filter(|&offset| offset <= u32::MAX.into());
    let access = field("access").and_then(|value| match value {
        "read" => Some("read"),
        "write" => Some("write"),
        "execute" => Some("execute"),
        _ => None,
    });
    Some(NativeFingerprint {
        exception_code,
        module,
        module_offset,
        access,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crash_report::tests::TempDir;
    use std::fs;

    #[test]
    fn legacy_recovery_time_is_not_reported_as_runtime() {
        let temp = TempDir::new();
        let dir = &temp.0;
        capture::write_json(
            &dir.join("report.json"),
            &serde_json::json!({
                "schema_version": 1,
                "app": "slate",
                "version": "1.0.10",
                "started_unix_ms": 100_001,
                "ended_unix_ms": 178_585_001,
                "success": false,
                "exit_code": null,
                "exit_status": "supervisor error: Previous run ended without a recorded exit status; it may have been terminated or interrupted",
            }),
        )
        .unwrap();
        let submission = serde_json::to_value(submission_summary(dir).unwrap()).unwrap();
        assert_eq!(submission["completion"], "unknown");
        assert_eq!(submission["duration_seconds"], serde_json::Value::Null);
    }

    #[test]
    fn legacy_report_sends_only_allowlisted_crash_facts() {
        let temp = TempDir::new();
        let dir = &temp.0;
        let report = serde_json::json!({
            "schema_version": 1,
            "app": "slate",
            "version": "1.0.10-preview.277",
            "platform": "windows-x86_64",
            "started_unix_ms": 100_001,
            "ended_unix_ms": 103_500,
            "success": false,
            "exit_code": -1_073_741_819_i32,
            "monitor_pid": 123,
            "child_pid": 456,
            "native_exception": true,
            "stderr_tail": "private-stderr",
            "exit_status": "private-error",
            "session_tail": "private-session-tail",
            "executable": {"path": "private-executable-path"},
            "session_latest": {
                "log_path": "private-session-path",
                "marks": ["private-mark"],
                "frame": {"max_ms": 275.4},
                "now": {"view": "board", "tool": "private-tool", "entries": 9},
            },
        });
        capture::write_json(&dir.join("report.json"), &report).unwrap();
        let native = dir.join("native-exception.txt");
        fs::write(
            &native,
            "exception_code=0xc0000005\nmodule=ntdll.dll\nmodule_offset=0x1234\naccess=write\nthread_id=456\nmemory_address=0xdeadbeef\n",
        )
        .unwrap();
        fs::write(
            dir.join("panic-fake.json"),
            "private-panic payload and backtrace",
        )
        .unwrap();
        let submission = serde_json::to_value(submission_summary(dir).unwrap()).unwrap();
        assert_eq!(
            submission,
            serde_json::json!({
                "schema_version": 1,
                "app": "slate",
                "version": "1.0.10-preview.277",
                "release_channel": null,
                "platform": "windows-x86_64",
                "completion": "process_exit",
                "duration_seconds": 3,
                "success": false,
                "exit_code": -1_073_741_819_i32,
                "panic_count": 1,
                "evidence": {"native_exception": true, "session_checkpoint": true, "stderr": true},
                "last_view": "board",
                "peak_frame_ms": 275,
                "native": {
                    "exception_code": 0xc0000005u32,
                    "module": "ntdll.dll",
                    "module_offset": 0x1234,
                    "access": "write",
                }
            })
        );

        fs::write(
            &native,
            "exception_code=0xc0000005\nmodule=private-project.dll\nmodule_offset=0x1234\n",
        )
        .unwrap();
        let submission = serde_json::to_value(submission_summary(dir).unwrap()).unwrap();
        assert_eq!(
            submission["native"],
            serde_json::json!({"exception_code": 0xc0000005u32})
        );
    }
}
