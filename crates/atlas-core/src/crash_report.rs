//! Local crash evidence, independent of the application's frame loop.
//!
//! A small supervising process owns [`RunReport`], redirects the app's stderr
//! to [`RunReport::stderr_path`], and calls [`RunReport::finish`] after it exits.
//! The app installs [`install_panic_hook`] before starting any workers. Native
//! exceptions need no in-process exception handler: the supervisor survives
//! them and records the operating system's exit status.
//!
//! Reports contain diagnostics only: no document snapshots, memory dumps,
//! environment inventory, or secret-store reads. Nothing is uploaded.

mod capture;
mod panic_hook;
mod recovery;
mod storage;

#[cfg(test)]
mod tests;

pub use panic_hook::install_panic_hook;
pub use storage::{acknowledge, reports_dir, unread_reports, ReportSummary};

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const STDERR_LIMIT: usize = 64 * 1024;
pub(crate) const SESSION_LIMIT: usize = 64 * 1024;
pub(crate) const RETAIN_REPORTS: usize = 20;

#[derive(Serialize, Deserialize)]
struct RunMetadata {
    schema_version: u32,
    app: String,
    version: String,
    executable: Option<ExecutableIdentity>,
    platform: String,
    started_unix_ms: u64,
    monitor_pid: u32,
    child_pid: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct ExecutableIdentity {
    path: PathBuf,
    size_bytes: u64,
    modified_unix_ms: Option<u64>,
}

#[derive(Serialize)]
struct ExitReport<'a> {
    #[serde(flatten)]
    run: &'a RunMetadata,
    ended_unix_ms: u64,
    success: bool,
    exit_code: Option<i32>,
    exit_code_hex: Option<String>,
    exit_status: String,
    panic_count: usize,
    native_exception: bool,
    stderr_tail: String,
    session_latest: Option<serde_json::Value>,
    session_tail: String,
}

/// One child app launch. Keep this alive until the child has been waited on.
pub struct RunReport {
    dir: PathBuf,
    session_dir: PathBuf,
    run: RunMetadata,
}

impl RunReport {
    pub fn start(app: &str, version: &str) -> io::Result<Self> {
        Self::start_at(
            reports_dir(app),
            crate::index::data_dir().join("session-log"),
            app,
            version,
        )
    }

    fn start_at(root: PathBuf, session_dir: PathBuf, app: &str, version: &str) -> io::Result<Self> {
        fs::create_dir_all(&root)?;
        let _ = storage::prune(&root);
        let dir = new_run_dir(&root)?;
        let run = RunMetadata {
            schema_version: 1,
            app: storage::app_component(app),
            version: capture::clip(version, 256),
            executable: executable_identity(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            started_unix_ms: now_unix_ms(),
            monitor_pid: std::process::id(),
            child_pid: None,
        };
        let report = Self {
            dir,
            session_dir,
            run,
        };
        fs::write(
            report.dir.join(".active"),
            report.run.monitor_pid.to_string(),
        )?;
        report.write_metadata()?;
        Ok(report)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Parent redirects stdout and stderr here; finish keeps only a bounded tail.
    pub fn stderr_path(&self) -> PathBuf {
        self.dir.join("stderr.log")
    }

    pub fn set_child_pid(&mut self, pid: u32) -> io::Result<()> {
        self.run.child_pid = Some(pid);
        self.write_metadata()
    }

    fn write_metadata(&self) -> io::Result<()> {
        capture::write_json(&self.dir.join("run.json"), &self.run)
    }

    /// Returns report.json for a nonzero/native exit or any recorded panic.
    /// Successful launches without panic evidence leave no report directory.
    pub fn finish(self, exit: ExitStatus) -> io::Result<Option<PathBuf>> {
        self.finish_record(exit.success(), exit.code(), exit.to_string())
    }

    /// Preserve a diagnostic when the supervisor cannot launch/wait for the app.
    pub fn finish_error(self, error: &str) -> io::Result<PathBuf> {
        self.finish_record(
            false,
            None,
            format!("supervisor error: {}", capture::clip(error, 4096)),
        )?
        .ok_or_else(|| io::Error::other("supervisor error report was not saved"))
    }

    fn finish_record(
        self,
        success: bool,
        exit_code: Option<i32>,
        exit_status: String,
    ) -> io::Result<Option<PathBuf>> {
        let panic_count = storage::panic_count(&self.dir);
        let native_exception = storage::has_native_exception(&self.dir);
        if success && panic_count == 0 && !native_exception {
            fs::remove_dir_all(&self.dir)?;
            return Ok(None);
        }
        let stderr_tail = capture::read_tail(&self.stderr_path(), STDERR_LIMIT).unwrap_or_default();
        let (session_latest, session_tail) = capture::session_evidence(
            &self.session_dir,
            &self.run.app,
            self.run.child_pid,
            self.run.started_unix_ms,
        );
        let report = ExitReport {
            run: &self.run,
            ended_unix_ms: now_unix_ms(),
            success,
            exit_code,
            exit_code_hex: exit_code.map(|code| format!("0x{:08X}", code as u32)),
            exit_status,
            panic_count,
            native_exception,
            stderr_tail,
            session_latest,
            session_tail,
        };
        // Bound the raw redirect file too, before writing the report. Keeping
        // the original unbounded file would defeat report retention's limit.
        let _ = fs::write(self.stderr_path(), report.stderr_tail.as_bytes());
        let path = self.dir.join("report.json");
        capture::write_json(&path, &report)?;
        fs::write(self.dir.join("unread"), b"")?;
        fs::remove_file(self.dir.join(".active"))?;
        if let Some(root) = self.dir.parent() {
            let _ = storage::prune(root);
        }
        Ok(Some(path))
    }
}

impl Drop for RunReport {
    fn drop(&mut self) {
        // Once a child exists, only successful finalization clears ownership.
        // A failed write leaves its marker and stderr for next-start recovery.
        if self.run.child_pid.is_none() {
            let _ = fs::remove_file(self.dir.join(".active"));
            if !self.dir.join("report.json").exists() && !storage::has_evidence(&self.dir) {
                let _ = fs::remove_dir_all(&self.dir);
            }
        }
    }
}

fn new_run_dir(root: &Path) -> io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..100 {
        let name = format!(
            "{}-{}-{}",
            now_unix_ms(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = root.join(name);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "crash-report directory collision",
    ))
}

pub(crate) fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn executable_identity() -> Option<ExecutableIdentity> {
    let path = std::env::current_exe().ok()?;
    let metadata = fs::metadata(&path).ok()?;
    Some(ExecutableIdentity {
        path,
        size_bytes: metadata.len(),
        modified_unix_ms: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64),
    })
}
