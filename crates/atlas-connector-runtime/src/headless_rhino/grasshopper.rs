//! Serial mailbox for an owned, warm Rhino/Grasshopper process.

use super::grasshopper_packages::{LocalPackage, MissingPackage, PackageCandidate};
use crate::transport::Cancellation;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(300);
const JOB_TIMEOUT: Duration = Duration::from_secs(180);
const POLL: Duration = Duration::from_millis(35);
const MESSAGE_LIMIT: usize = 8 * 1024 * 1024;
static NEXT_WORKER: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize)]
pub struct Request<'a> {
    pub action: &'a str,
    pub viewer_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<&'a Path>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<&'a serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<&'a serde_json::Value>,
}

#[derive(Deserialize)]
pub struct Response {
    pub status: String,
    #[serde(default)]
    pub controls: serde_json::Value,
    #[serde(default)]
    pub ports: serde_json::Value,
    #[serde(default)]
    pub outputs: serde_json::Value,
    #[serde(default)]
    pub missing_packages: Vec<MissingPackage>,
    #[serde(default)]
    pub trusted_sources: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<PackageCandidate>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub local_package: Option<LocalPackage>,
    #[serde(default)]
    pub loaded_component_ids: Vec<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub baked: bool,
    pub error: Option<String>,
}

/// The job owns its transient output until the caller finishes reading it.
pub struct Output {
    pub response: Response,
    recovery: Option<String>,
    directory: OwnedDirectory,
    _root: Arc<OwnedDirectory>,
}

impl Output {
    pub fn recovery_warning(&self) -> Option<&str> {
        self.recovery.as_deref()
    }

    pub fn preview_path(&self) -> Option<PathBuf> {
        self.response
            .preview
            .then(|| self.directory.0.join("preview.3dm"))
    }

    pub fn bake_path(&self) -> Option<PathBuf> {
        self.response
            .baked
            .then(|| self.directory.0.join("bake.3dm"))
    }
}

struct OwnedDirectory(PathBuf);

impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct Worker {
    child: Child,
    input: ChildStdin,
    root: Arc<OwnedDirectory>,
    next_job: u64,
    recovery: Option<String>,
}

struct StartupFailure {
    message: String,
    plugin_crash_protection: bool,
}

impl From<String> for StartupFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            plugin_crash_protection: false,
        }
    }
}

impl From<&str> for StartupFailure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl Worker {
    pub fn is_recovery(&self) -> bool {
        self.recovery.is_some()
    }

    pub fn start(
        executable: &Path,
        rhino_system: &Path,
        workspace: &Path,
        cancel: &Cancellation,
    ) -> Result<Self, String> {
        match Self::start_profile(executable, rhino_system, workspace, cancel, false) {
            Ok(worker) => Ok(worker),
            Err(error) if error.plugin_crash_protection && !cancel.is_cancelled() => {
                let mut worker =
                    Self::start_profile(executable, rhino_system, workspace, cancel, true)
                        .map_err(|fallback| {
                            format!(
                                "{} Built-in-only startup also failed: {}",
                                error.message, fallback.message
                            )
                        })?;
                worker.recovery = Some(format!(
                    "Running with Grasshopper's built-in components only. {}",
                    error.message
                ));
                Ok(worker)
            }
            Err(error) => Err(error.message),
        }
    }

    fn start_profile(
        executable: &Path,
        rhino_system: &Path,
        workspace: &Path,
        cancel: &Cancellation,
        built_ins_only: bool,
    ) -> Result<Self, StartupFailure> {
        if cancel.is_cancelled() {
            return Err("Grasshopper startup cancelled".into());
        }
        if !executable.is_absolute() || !executable.is_file() {
            return Err("The Slate headless Rhino helper is unavailable".into());
        }
        if !rhino_system.join("RhinoCommon.dll").is_file() {
            return Err("Grasshopper needs a licensed local Rhino 8 installation".into());
        }
        if !workspace.is_absolute() {
            return Err("Grasshopper worker directory must be absolute".into());
        }
        fs::create_dir_all(workspace).map_err(io_error)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io_error)?
            .as_nanos();
        let serial = NEXT_WORKER.fetch_add(1, Ordering::Relaxed);
        let root = workspace.join(format!("gh-{}-{stamp}-{serial}", std::process::id()));
        fs::create_dir(&root).map_err(io_error)?;
        let root = Arc::new(OwnedDirectory(root));
        fs::create_dir(root.0.join("jobs")).map_err(io_error)?;

        let mut command = Command::new(executable);
        command.arg(rhino_system).arg(&root.0).arg("grasshopper");
        if built_ins_only {
            command.arg("built-ins-only");
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(io_error)?;
        let Some(input) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Could not open the Grasshopper worker mailbox".into());
        };
        let mut worker = Self {
            child,
            input,
            root,
            next_job: 0,
            recovery: None,
        };
        if let Err(error) = worker.wait_for(&worker.root.0.join("ready"), STARTUP_TIMEOUT, cancel) {
            let plugin_crash_protection = worker.root.0.join("plugin-crash-protection").is_file();
            let stage = read_bounded(&worker.root.0.join("progress.txt"), 512)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok());
            return Err(StartupFailure {
                message: if plugin_crash_protection {
                    error
                } else {
                    stage.map_or_else(|| error.clone(), |stage| format!("{error}. {stage}"))
                },
                plugin_crash_protection,
            });
        }
        Ok(worker)
    }

    pub fn request(
        &mut self,
        request: &impl Serialize,
        cancel: &Cancellation,
    ) -> Result<Output, String> {
        if cancel.is_cancelled() {
            return Err("Grasshopper request cancelled".into());
        }
        self.next_job += 1;
        let token = format!("{:032x}", self.next_job);
        let directory = self.root.0.join("jobs").join(&token);
        fs::create_dir(&directory).map_err(io_error)?;
        let directory = OwnedDirectory(directory);
        let request = serde_json::to_vec(request).map_err(io_error)?;
        if request.len() > MESSAGE_LIMIT {
            return Err("Grasshopper request exceeds the worker size limit".into());
        }
        fs::write(directory.0.join("request.json"), request).map_err(io_error)?;
        writeln!(self.input, "{token}").map_err(io_error)?;
        self.input.flush().map_err(io_error)?;
        let response = directory.0.join("response.json");
        self.wait_for(&response, JOB_TIMEOUT, cancel)?;
        let mut response: Response =
            serde_json::from_slice(&read_bounded(&response, MESSAGE_LIMIT)?).map_err(io_error)?;
        if let Some(warning) = &self.recovery {
            if response.status == "missing_packages" {
                response.status = "error".into();
                response.diagnostics.insert(0, format!(
                    "This definition requires plug-ins outside the built-in recovery profile. {warning}"
                ));
                response.missing_packages.clear();
            } else {
                response.diagnostics.insert(0, warning.clone());
            }
        }
        Ok(Output {
            response,
            recovery: self.recovery.clone(),
            directory,
            _root: Arc::clone(&self.root),
        })
    }

    fn wait_for(
        &mut self,
        path: &Path,
        timeout: Duration,
        cancel: &Cancellation,
    ) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                return Err(if cancel.is_cancelled() {
                    "Grasshopper request cancelled".into()
                } else {
                    "Grasshopper did not respond before the Rhino worker deadline".into()
                });
            }
            if path.is_file() {
                return Ok(());
            }
            let error = self.root.0.join("error.txt");
            if error.is_file() || self.child.try_wait().map_err(io_error)?.is_some() {
                return Err(read_bounded(&error, 16384)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| "The Grasshopper worker exited during a request".into()));
            }
            thread::sleep(POLL);
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(io_error)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > limit {
        return Err("The Grasshopper mailbox response exceeded its size limit".into());
    }
    Ok(bytes)
}

fn io_error(error: impl std::fmt::Display) -> String {
    format!("Grasshopper worker: {error}")
}
