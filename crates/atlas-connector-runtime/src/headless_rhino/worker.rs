use super::{GeometryPlan, Units};
use crate::{artifact::ArtifactStore, transport::Cancellation};
use atlas_connector::{ArtifactTransfer, MAX_ARTIFACT_BYTES};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

// One owned STA helper at a time, including independently created adapters.
static RHINO_WORKER: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone)]
pub struct HeadlessRhino {
    /// Trusted installation/build configuration, never command parameters.
    pub executable: PathBuf,
    pub rhino_system: PathBuf,
    pub workspace: PathBuf,
    pub timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelReport {
    pub units: Units,
    pub actual_units: String,
    pub source_objects: Vec<SourceObject>,
    pub objects: usize,
    pub preserved_objects: usize,
    pub meshes: usize,
    pub vertices: usize,
    pub faces: usize,
    pub operation_ids: Vec<String>,
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceObject {
    pub object_id: String,
    pub name: String,
    pub geometry: String,
    pub layer: i32,
    pub material: i32,
}

pub struct GeneratedModel {
    pub report: ModelReport,
    pub bytes: Vec<u8>,
}

impl GeneratedModel {
    /// Publication still uses the existing authenticated artifact transport.
    pub fn retain(
        self,
        store: &mut ArtifactStore,
        transfer_id: &str,
    ) -> Result<(ModelReport, ArtifactTransfer), String> {
        let descriptor = store
            .insert(transfer_id, "model/vnd.rhino", self.bytes)
            .map_err(|e| e.to_string())?;
        Ok((self.report, descriptor))
    }
}

impl HeadlessRhino {
    /// Blocking SDK job: run only following Session::begin -> Begin::Dispatch for
    /// GENERATE_COMMAND, with SourceMode::IsolatedDraft. This is a backend primitive,
    /// not a second authority policy. Cancellation/failure never publishes bytes.
    pub fn generate(
        &self,
        plan: &GeometryPlan,
        cancel: &Cancellation,
        progress: impl FnMut(&str),
    ) -> Result<GeneratedModel, String> {
        self.generate_from(plan, None, cancel, progress)
    }

    pub fn generate_from(
        &self,
        plan: &GeometryPlan,
        source: Option<&[u8]>,
        cancel: &Cancellation,
        mut progress: impl FnMut(&str),
    ) -> Result<GeneratedModel, String> {
        self.generate_with_origin(plan, source, None, cancel, &mut progress)
    }

    pub fn generate_with_origin(
        &self,
        plan: &GeometryPlan,
        source: Option<&[u8]>,
        origin: Option<&std::path::Path>,
        cancel: &Cancellation,
        mut progress: impl FnMut(&str),
    ) -> Result<GeneratedModel, String> {
        plan.validate()?;
        if cancel.is_cancelled() {
            return Err("Headless generation cancelled before launch".into());
        }
        let _guard = RHINO_WORKER.try_lock().map_err(|_| {
            "Rhino headless worker is busy; retry when the current job finishes".to_string()
        })?;
        if self.timeout < Duration::from_secs(1) || self.timeout > Duration::from_secs(300) {
            return Err("Rhino timeout must be 1..300 seconds".into());
        }
        if !self.executable.is_absolute()
            || !self.executable.is_file()
            || !self.rhino_system.join("RhinoCommon.dll").is_file()
        {
            return Err("Headless Rhino unavailable: build tools/headless-rhino and install/license Rhino 8".into());
        }
        if !self.workspace.is_absolute() {
            return Err("Headless workspace must be an absolute trusted directory".into());
        }
        fs::create_dir_all(&self.workspace).map_err(|e| e.to_string())?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let job = self
            .workspace
            .join(format!("rhino-{}-{nonce}", std::process::id()));
        fs::create_dir(&job).map_err(|e| e.to_string())?; // never reuse/overwrite a linked file
        if let Some(bytes) = source {
            if bytes.len() as u64 > MAX_ARTIFACT_BYTES
                || !bytes.starts_with(b"3D Geometry File Format ")
            {
                return Err("Bound Rhino source is invalid or exceeds 64 MiB".into());
            }
            fs::write(job.join("input.3dm"), bytes).map_err(|e| e.to_string())?;
            if let Some(root) = origin {
                if !root.is_absolute() {
                    return Err("Source texture origin must be absolute".into());
                }
                fs::write(
                    job.join("source-root.txt"),
                    root.to_string_lossy().as_bytes(),
                )
                .map_err(|e| e.to_string())?;
            }
        }
        fs::write(
            job.join("plan.json"),
            serde_json::to_vec(plan).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut command = Command::new(&self.executable);
        command
            .arg(&self.rhino_system)
            .arg(&job)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW; RhinoCore default is headless
        }
        progress("starting Rhino helper");
        let mut child = command
            .spawn()
            .map_err(|e| format!("Cannot launch Rhino helper: {e}"))?;
        let start = Instant::now();
        let mut stopped = None;
        let mut seen = 0;
        let status = loop {
            if let Ok(log) = fs::read_to_string(job.join("progress.txt")) {
                if log.len() > 32768 {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("Rhino progress limit exceeded".into());
                }
                // Only complete lines are public progress; no private agent text.
                if let Some(end) = log.rfind('\n') {
                    if end + 1 > seen {
                        for line in log[seen..=end].lines() {
                            progress(line);
                        }
                        seen = end + 1;
                    }
                }
            }
            let timed_out = start.elapsed() > self.timeout;
            if (cancel.is_cancelled() || timed_out) && stopped.is_none() {
                if let Err(e) = fs::write(job.join("stop"), []) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("Cannot stop Rhino helper: {e}"));
                }
                progress("stopping owned Rhino job; waiting for SDK boundary");
                stopped = Some(Instant::now());
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(e.to_string());
                }
            }
            if stopped.is_some_and(|time| time.elapsed() > Duration::from_secs(5)) {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Rhino stopped during an uninterruptible SDK call; isolated output is uncertain and was not published".into());
            }
            thread::sleep(Duration::from_millis(100));
        };
        if stopped.is_some() || cancel.is_cancelled() {
            return Err("Rhino job cancelled or timed out; output was not published".into());
        }
        if !status.success() {
            let error = read_bounded(&job.join("error.txt"), 4096)
                .ok()
                .and_then(|v| String::from_utf8(v).ok())
                .unwrap_or_else(|| "Runtime/license startup or geometry operation failed".into());
            return Err(format!("Rhino helper {status}: {error}"));
        }
        progress("verifying generated artifact bytes");
        let report: ModelReport =
            serde_json::from_slice(&read_bounded(&job.join("result.json"), 32768)?)
                .map_err(|e| format!("Invalid Rhino report: {e}"))?;
        let bytes = read_bounded(&job.join("model.3dm"), MAX_ARTIFACT_BYTES)?;
        if !bytes.starts_with(b"3D Geometry File Format ")
            || report.units != plan.units
            || report.objects > 256
            || report.meshes > 1000000
            || report.vertices > 1000000
            || report.faces > 1000000
            || report.operation_ids.len() + report.preserved_objects != report.objects
            || report
                .operation_ids
                .iter()
                .any(|id| !plan.operations.iter().any(|op| op.id() == id))
            || (0..3).any(|i| {
                !report.min[i].is_finite()
                    || !report.max[i].is_finite()
                    || report.min[i] > report.max[i]
            })
        {
            return Err("Rhino artifact/report failed bounded geometry verification".into());
        }
        if cancel.is_cancelled() {
            return Err("Rhino job cancelled before publication".into());
        }
        Ok(GeneratedModel { report, bytes })
    }
}

fn read_bounded(path: &std::path::Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("Rhino output exceeds byte budget".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err("Rhino output is empty or exceeds byte budget".into());
    }
    Ok(bytes)
}
