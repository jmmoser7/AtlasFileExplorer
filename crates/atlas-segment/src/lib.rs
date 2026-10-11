//! Offline object segmentation over already-resident preview pixels.
//! One persistent model process retains the encoder output between point prompts.
//! No source paths or filesystem reads are involved in an inference request.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::Duration;

/// One ordered positive or negative point in normalized image coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointPrompt {
    pub point: [f32; 2],
    pub positive: bool,
}

#[derive(Serialize)]
struct WirePoint {
    point: [f32; 2],
    positive: bool,
}

#[derive(Serialize)]
struct WireRequest<'a> {
    key: &'a str,
    width: usize,
    height: usize,
    rgb: &'a [u8],
    points: Vec<WirePoint>,
    window: &'a [Vec<[f32; 2]>],
}

#[derive(Serialize)]
pub struct Request {
    /// Stable identity of the resident pixel revision; dimensions are added
    /// to this key by the worker before reusing its encoded image.
    pub key: String,
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    pub point: [f32; 2],
    pub window: Vec<Vec<[f32; 2]>>,
}

/// A SAM mask proposal and the score returned by SAM's multimask decoder.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Candidate {
    pub quality: f32,
    pub contours: Vec<Vec<[f32; 2]>>,
}

/// Candidate order is the stable order returned by the installed SAM adapter.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Prediction {
    pub candidates: Vec<Candidate>,
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    candidates: Vec<Candidate>,
    // Read older worker replies too, although this process normally embeds
    // the current adapter source.
    #[serde(default)]
    contours: Vec<Vec<[f32; 2]>>,
    error: Option<String>,
}

pub struct Segmenter {
    child: Child,
    input: ChildStdin,
    output: crossbeam_channel::Receiver<Result<String, String>>,
}

impl Segmenter {
    pub fn start(data_dir: &Path) -> Result<Self, String> {
        let root = data_dir.join("segmentation");
        let python = root.join(if cfg!(windows) {
            "venv/Scripts/python.exe"
        } else {
            "venv/bin/python"
        });
        if !python.is_file() || !root.join("model/sam2.1_hiera_tiny.pt").is_file() {
            return Err("Object highlighting needs its local model. Run scripts/setup-segmentation.ps1 once.".into());
        }
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

    /// Segment using the legacy single positive point and select SAM's
    /// highest-scoring candidate.
    pub fn segment(&mut self, request: &Request) -> Result<Vec<Vec<[f32; 2]>>, String> {
        let prediction = self.segment_points(
            request,
            &[PointPrompt {
                point: request.point,
                positive: true,
            }],
        )?;
        Ok(best_candidate(&prediction)
            .map_or_else(Vec::new, |candidate| candidate.contours.clone()))
    }

    /// Segment with ordered positive and negative point prompts, returning
    /// every candidate and its SAM quality score for caller-side selection.
    pub fn segment_points(
        &mut self,
        request: &Request,
        prompts: &[PointPrompt],
    ) -> Result<Prediction, String> {
        validate_request(request, prompts)?;
        // Buffer the RGB array: one pipe syscall per JSON number makes a
        // resident image slower to send than to segment.
        let mut input = BufWriter::with_capacity(64 * 1024, &mut self.input);
        let wire = WireRequest {
            key: &request.key,
            width: request.width,
            height: request.height,
            rgb: &request.rgb,
            points: prompts
                .iter()
                .map(|prompt| WirePoint {
                    point: prompt.point,
                    positive: prompt.positive,
                })
                .collect(),
            window: &request.window,
        };
        serde_json::to_writer(&mut input, &wire).map_err(|e| e.to_string())?;
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
        let candidates = if reply.candidates.is_empty() && !reply.contours.is_empty() {
            vec![Candidate {
                quality: 1.0,
                contours: reply.contours,
            }]
        } else {
            reply.candidates
        };
        if candidates.iter().any(|candidate| {
            !candidate.quality.is_finite()
                || !(0.0..=1.0).contains(&candidate.quality)
                || !valid_contours(&candidate.contours)
        }) {
            return Err("The segmentation worker returned invalid candidates.".into());
        }
        Ok(Prediction { candidates })
    }
}

fn validate_request(request: &Request, prompts: &[PointPrompt]) -> Result<(), String> {
    let pixels = request
        .width
        .checked_mul(request.height)
        .filter(|pixels| request.width >= 2 && request.height >= 2 && *pixels <= 768 * 768)
        .ok_or_else(|| "Invalid segmentation image size.".to_string())?;
    if request.rgb.len() != pixels * 3 {
        return Err("Invalid segmentation image pixels.".into());
    }
    if prompts.is_empty()
        || !prompts.iter().any(|prompt| prompt.positive)
        || prompts.iter().any(|prompt| {
            prompt
                .point
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
        })
    {
        return Err(
            "Segmentation needs finite in-image points and at least one positive point.".into(),
        );
    }
    if request.window.iter().flatten().any(|point| {
        point
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    }) {
        return Err("Invalid segmentation window geometry.".into());
    }
    Ok(())
}

fn valid_contours(contours: &[Vec<[f32; 2]>]) -> bool {
    contours.iter().flatten().all(|point| {
        point
            .iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
    })
}

fn best_candidate(prediction: &Prediction) -> Option<&Candidate> {
    prediction
        .candidates
        .iter()
        .max_by(|a, b| a.quality.total_cmp(&b.quality))
}

impl Drop for Segmenter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            key: "image-revision-1".into(),
            width: 2,
            height: 2,
            rgb: vec![0; 12],
            point: [0.25, 0.5],
            window: vec![vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]],
        }
    }

    #[test]
    fn ordered_prompts_serialize_as_points_with_labels() {
        let request = request();
        let wire = WireRequest {
            key: &request.key,
            width: request.width,
            height: request.height,
            rgb: &request.rgb,
            points: vec![
                WirePoint {
                    point: [0.25, 0.5],
                    positive: true,
                },
                WirePoint {
                    point: [0.75, 0.4],
                    positive: false,
                },
            ],
            window: &request.window,
        };
        let json = serde_json::to_value(wire).unwrap();
        assert_eq!(json["points"][0]["point"], serde_json::json!([0.25, 0.5]));
        assert_eq!(json["points"][0]["positive"], true);
        assert_eq!(json["points"][1]["positive"], false);
    }

    #[test]
    fn legacy_request_serialization_keeps_single_point_field() {
        let json = serde_json::to_value(request()).unwrap();
        assert_eq!(json["point"], serde_json::json!([0.25, 0.5]));
        assert!(json.get("points").is_none());
    }

    #[test]
    fn request_validation_requires_finite_in_range_prompts_and_positive() {
        assert!(validate_request(
            &request(),
            &[
                PointPrompt {
                    point: [0.25, 0.5],
                    positive: true,
                },
                PointPrompt {
                    point: [0.75, 0.4],
                    positive: false,
                },
            ]
        )
        .is_ok());
        assert!(validate_request(&request(), &[]).is_err());
        assert!(validate_request(
            &request(),
            &[PointPrompt {
                point: [0.25, 0.5],
                positive: false,
            }]
        )
        .is_err());
        assert!(validate_request(
            &request(),
            &[PointPrompt {
                point: [f32::NAN, 0.5],
                positive: true,
            }]
        )
        .is_err());
        assert!(validate_request(
            &request(),
            &[PointPrompt {
                point: [1.01, 0.5],
                positive: true,
            }]
        )
        .is_err());
    }

    #[test]
    fn legacy_contours_reply_and_best_candidate_selection_are_supported() {
        let legacy: Reply =
            serde_json::from_str(r#"{"contours":[[[0.1,0.2],[0.2,0.3],[0.3,0.2]]] }"#).unwrap();
        let candidates = if legacy.candidates.is_empty() && !legacy.contours.is_empty() {
            vec![Candidate {
                quality: 1.0,
                contours: legacy.contours,
            }]
        } else {
            legacy.candidates
        };
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].quality, 1.0);

        let prediction = Prediction {
            candidates: vec![
                Candidate {
                    quality: 0.4,
                    contours: vec![],
                },
                Candidate {
                    quality: 0.8,
                    contours: vec![vec![[0.1, 0.1], [0.2, 0.1], [0.2, 0.2]]],
                },
            ],
        };
        assert_eq!(best_candidate(&prediction).unwrap().quality, 0.8);
    }

    #[cfg(windows)]
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
