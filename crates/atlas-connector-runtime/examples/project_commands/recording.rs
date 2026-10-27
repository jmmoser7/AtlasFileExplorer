//! Optional production recording; process ownership is restricted to this encoder.
use super::{support::Evidence, Result};
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub(super) struct Recording {
    child: Option<Child>,
    output: PathBuf,
}
impl Recording {
    pub(super) fn start(hwnd: &str, evidence: &mut Evidence) -> Result<Option<Self>> {
        let Some(exe) = std::env::var_os("ATLAS_PROJECT_RECORD_FFMPEG") else {
            return Ok(None);
        };
        let output = PathBuf::from(
            std::env::var_os("ATLAS_PROJECT_FOOTAGE").ok_or("ATLAS_PROJECT_FOOTAGE required")?,
        );
        if output.exists() {
            return Err("footage output already exists".into());
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        let log = File::create(evidence.directory.join("ffmpeg.log"))?;
        let child = Command::new(exe)
            .args([
                "-hide_banner",
                "-nostats",
                "-f",
                "gdigrab",
                "-framerate",
                "15",
                "-i",
                &format!("hwnd={hwnd}"),
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-crf",
                "20",
                "-pix_fmt",
                "yuv420p",
                "-vf",
                "pad=ceil(iw/2)*2:ceil(ih/2)*2",
                "-n",
            ])
            .arg(&output)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()?;
        let recording = Self {
            child: Some(child),
            output,
        };
        evidence.record(
            json!({"event":"recording_started","pid":recording.child.as_ref().map(Child::id),
                "output":recording.output,"hwnd":hwnd}),
        )?;
        Ok(Some(recording))
    }
    fn stop(&mut self) -> Option<Value> {
        let mut child = self.child.take()?;
        let mut errors = Vec::new();
        if let Some(mut input) = child.stdin.take() {
            if let Err(error) = input.write_all(b"q\n") {
                errors.push(error.to_string());
            }
        }
        let started = Instant::now();
        let mut termination_requested = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(error) => {
                    errors.push(error.to_string());
                    if let Err(error) = child.kill() {
                        errors.push(error.to_string());
                    }
                    termination_requested = true;
                    break None;
                }
            }
            if !termination_requested && started.elapsed() >= Duration::from_secs(9) {
                // This handle belongs only to our optional encoder, never Rhino.
                if let Err(error) = child.kill() {
                    errors.push(error.to_string());
                }
                termination_requested = true;
            }
            if started.elapsed() >= Duration::from_secs(10) {
                errors.push("encoder shutdown exceeded ten seconds".into());
                break None;
            }
            std::thread::sleep(
                Duration::from_secs(10)
                    .saturating_sub(started.elapsed())
                    .min(Duration::from_millis(50)),
            );
        };
        let output_bytes = match fs::metadata(&self.output) {
            Ok(metadata) => Some(metadata.len()),
            Err(error) => {
                errors.push(error.to_string());
                None
            }
        };
        Some(
            json!({"event":"recording_finished","pid":child.id(),"output":self.output,
            "exit_code":status.as_ref().and_then(|status|status.code()),
            "success":status.as_ref().is_some_and(|status|status.success())
                && output_bytes.is_some_and(|bytes|bytes>0),
            "termination_requested":termination_requested,"output_bytes":output_bytes,"errors":errors}),
        )
    }
    pub(super) fn finish(&mut self, evidence: &mut Evidence) {
        if let Some(receipt) = self.stop() {
            if let Err(error) = evidence.record(receipt) {
                eprintln!("Could not record optional footage status: {error}");
            }
        }
    }
}
impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
