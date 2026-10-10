//! Offline object segmentation over already-resident preview pixels.
//! One persistent model process retains the encoder output between point prompts.
//! No source paths or filesystem reads are involved in an inference request.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

#[derive(Serialize)]
pub struct Request {
    pub key: String,
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    pub point: [f32; 2],
    pub window: Vec<Vec<[f32; 2]>>,
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    contours: Vec<Vec<[f32; 2]>>,
    error: Option<String>,
}

/// The local model and its venv are both on disk. No process is started.
pub fn installed(data_dir: &Path) -> bool {
    let root = data_dir.join("segmentation");
    root.join(python_relative()).is_file() && root.join("model/sam2.1_hiera_tiny.pt").is_file()
}

fn python_relative() -> &'static str {
    if cfg!(windows) {
        "venv/Scripts/python.exe"
    } else {
        "venv/bin/python"
    }
}

pub struct Segmenter {
    child: Child,
    input: ChildStdin,
    output: crossbeam_channel::Receiver<Result<String, String>>,
}

impl Segmenter {
    pub fn start(data_dir: &Path) -> Result<Self, String> {
        let root = data_dir.join("segmentation");
        if !installed(data_dir) {
            return Err("Object highlighting needs its local model. Run scripts/setup-segmentation.ps1 once.".into());
        }
        let python = root.join(python_relative());
        let mut command = Command::new(python);
        command
            .args(["-u", "-c", include_str!("segmentation.py")])
            .env("ATLAS_SEGMENT_MODEL", root.join("model"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // No console; below-normal scheduling keeps canvas input ahead of inference.
            command.creation_flags(0x0800_4000);
        }
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().ok_or("Segmentation input unavailable")?;
        let mut reader = BufReader::new(
            child
                .stdout
                .take()
                .ok_or("Segmentation output unavailable")?,
        );
        let (tx, output) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || loop {
            let mut line = String::new();
            let reply = match reader.read_line(&mut line) {
                Ok(0) => Err("The local segmentation worker stopped. Run scripts/setup-segmentation.ps1 to repair it.".into()),
                Ok(_) => Ok(line),
                Err(error) => Err(error.to_string()),
            };
            let ended = reply.is_err();
            if tx.send(reply).is_err() || ended {
                break;
            }
        });
        let ready = output.recv_timeout(Duration::from_secs(120));
        if !matches!(ready, Ok(Ok(ref line)) if line.trim() == "{\"ready\":true}") {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The local object model could not start. Run scripts/setup-segmentation.ps1 to repair it.".into());
        }
        Ok(Self {
            child,
            input,
            output,
        })
    }

    pub fn segment(&mut self, request: &Request) -> Result<Vec<Vec<[f32; 2]>>, String> {
        // Buffer the RGB array: one pipe syscall per JSON number makes a
        // resident image slower to send than to segment.
        let mut input = BufWriter::with_capacity(64 * 1024, &mut self.input);
        serde_json::to_writer(&mut input, request).map_err(|e| e.to_string())?;
        input.write_all(b"\n").map_err(|e| e.to_string())?;
        input.flush().map_err(|e| e.to_string())?;
        drop(input);
        let line = self
            .output
            .recv_timeout(Duration::from_secs(120))
            .map_err(|_| {
                "The local segmentation worker did not respond within two minutes.".to_string()
            })??;
        let reply: Reply = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if let Some(error) = reply.error {
            return Err(error);
        }
        if reply
            .contours
            .iter()
            .flatten()
            .any(|p| p.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v)))
        {
            return Err("The segmentation worker returned invalid geometry.".into());
        }
        Ok(reply.contours)
    }
}

impl Drop for Segmenter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires scripts/setup-segmentation.ps1"]
    fn local_model_round_trip_reuses_the_image_for_another_object() {
        let root = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("NativeFileAtlas");
        let mut model = Segmenter::start(&root).expect("local model starts offline");
        let mut rgb = Vec::new();
        for y in 0..128 {
            for x in 0..192 {
                rgb.extend_from_slice(if (20..70).contains(&x) && (20..110).contains(&y) {
                    &[210, 40, 30]
                } else if (110..170).contains(&x) && (40..95).contains(&y) {
                    &[30, 80, 210]
                } else {
                    &[230, 230, 230]
                });
            }
        }
        let mut request = Request {
            key: "two-objects".into(),
            width: 192,
            height: 128,
            rgb,
            point: [0.23, 0.5],
            window: vec![vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]],
        };
        let first = model.segment(&request).expect("first point");
        request.point = [0.73, 0.5];
        let second = model
            .segment(&request)
            .expect("second point on cached image");
        assert!(!first.is_empty() && !second.is_empty());
        assert_ne!(first, second, "point prompts choose different objects");
    }
}
