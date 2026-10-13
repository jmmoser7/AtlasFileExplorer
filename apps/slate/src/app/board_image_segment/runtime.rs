//! Derived hover state and bounded model worker for image highlights.

use super::{region_node, SegmentResult};
use atlas_segment::{Candidate, PointPrompt, Prediction, Request, Segmenter};
use eframe::egui::{self, Pos2, Rect};
use slate_doc::{scene::Node, NodeId};
use std::time::Instant;

const SAMPLE_EDGE: usize = 768;

/// Sharing resident pixels makes dispatch constant-time; conversion stays off UI.
pub(super) struct PendingImage {
    /// SAM image-encoder key (pixels plus mirroring, not the active crop).
    pub key: String,
    /// Selection identity also contains crop geometry for stale-result checks.
    pub source_key: String,
    pub pixels: std::sync::Arc<egui::ColorImage>,
    pub mirror: [bool; 2],
    pub prompts: Vec<PointPrompt>,
    pub window: Vec<Vec<[f32; 2]>>,
}

impl PendingImage {
    fn prepare(self) -> Result<(Request, Vec<PointPrompt>), String> {
        let [w, h] = self.pixels.size;
        if w < 2 || h < 2 {
            return Err("Image preview is too small to highlight.".into());
        }
        let scale = (SAMPLE_EDGE as f32 / w.max(h) as f32).min(1.0);
        let width = (w as f32 * scale).round().max(2.0) as usize;
        let height = (h as f32 * scale).round().max(2.0) as usize;
        let mut rgb = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let sx = x * w / width;
                let sy = y * h / height;
                let sx = if self.mirror[0] { w - 1 - sx } else { sx };
                let sy = if self.mirror[1] { h - 1 - sy } else { sy };
                let color = self.pixels.pixels[sy * w + sx].to_srgba_unmultiplied();
                rgb.extend_from_slice(&color[..3]);
            }
        }
        let point = self
            .prompts
            .iter()
            .find(|prompt| prompt.positive)
            .map_or([0.5, 0.5], |prompt| prompt.point);
        Ok((
            Request {
                key: self.key,
                width,
                height,
                rgb,
                point,
                window: self.window,
            },
            self.prompts,
        ))
    }
}

type WorkerReply = (u64, Result<Prediction, String>);

#[derive(Clone)]
pub(super) struct PinnedSelection {
    pub(super) tab: u64,
    pub(super) host: NodeId,
    pub(super) source_key: String,
    pub(super) result: SegmentResult,
    pub(super) prompts: Vec<PointPrompt>,
    pub(super) screen: Pos2,
    pub(super) point_norm: [f32; 2],
}

/// Derived hover state for local image subject highlights; never journaled.
#[derive(Default)]
pub(crate) struct ImageSegmentRuntime {
    pub(super) hover: Option<Hover>,
    pub(super) result: Option<SegmentResult>,
    pub(super) pinned: Option<PinnedSelection>,
    pub(super) lasso: Option<LassoDraft>,
    pub(super) drag: Option<MaskDrag>,
    /// Optional hover growth prototype; never a document field.
    pub(crate) grow_debug: bool,
    pub(super) grow_past: Vec<std::sync::Arc<Vec<Vec<[f32; 2]>>>>,
    pub(super) grow_last: Option<Pos2>,
    pub(super) prompt_mode: PromptMode,
    pub(super) serial: u64,
    pub(super) pending: Option<u64>,
    pub(super) queued: Option<(u64, PendingImage)>,
    pub(super) worker: Option<crossbeam_channel::Sender<(u64, PendingImage)>>,
    pub(super) done: Option<crossbeam_channel::Receiver<WorkerReply>>,
    pub(super) action_rect: Option<Rect>,
    pub(super) corridor: Option<Rect>,
    pub(super) error: Option<String>,
    pub(super) dismissed: Option<Pos2>,
}

#[derive(Clone)]
pub(super) struct MaskDrag {
    pub(super) tab: u64,
    pub(super) host: NodeId,
    pub(super) source_key: String,
    /// Tight home geometry is computed once when the pointer is claimed.
    pub(super) cutout: Option<Node>,
    /// Base-only preview; the source's cached layer overlay paints separately.
    pub(super) preview: Option<Node>,
    pub(super) start: Pos2,
    pub(super) point: [f32; 2],
    pub(super) alt: bool,
    pub(super) pinned_on_press: bool,
    pub(super) can_extract: bool,
    pub(super) moved: bool,
    /// The snapped world displacement used by both preview and placement.
    pub(super) delta: egui::Vec2,
}

#[derive(Clone)]
pub(super) struct LassoDraft {
    pub(super) tab: u64,
    pub(super) host: NodeId,
    pub(super) source_key: String,
    pub(super) points: Vec<[f32; 2]>,
    pub(super) last_click: Option<Instant>,
    pub(super) last_click_screen: Option<Pos2>,
}

#[derive(Clone, Copy, Default)]
pub(super) enum PromptMode {
    #[default]
    Include,
    Exclude,
}

pub(super) struct Hover {
    pub(super) tab: u64,
    pub(super) host: NodeId,
    pub(super) gen: u64,
    pub(super) source_key: String,
    pub(super) point_norm: [f32; 2],
    pub(super) prompts: Vec<PointPrompt>,
    pub(super) screen: Pos2,
    pub(super) since: Instant,
}

impl ImageSegmentRuntime {
    /// Tool changes end transient gestures while retaining the pinned mask.
    pub(crate) fn cancel_tool_gestures(&mut self) {
        self.lasso = None;
        self.drag = None;
    }

    /// A current result or pending request owns this hover context.
    pub(super) fn offered(&self) -> bool {
        self.pinned.is_some()
            || self.result.is_some()
            || self.pending.is_some()
            || self.error.is_some()
    }

    pub(super) fn clear(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        self.hover = None;
        self.result = None;
        self.cancel_tool_gestures();
        self.grow_past.clear();
        self.grow_last = None;
        self.pending = None;
        self.queued = None;
        self.action_rect = None;
        self.corridor = None;
    }

    pub(super) fn receive(&mut self) {
        let Some(rx) = &self.done else {
            return;
        };
        while let Ok((serial, result)) = rx.try_recv() {
            if self.pending != Some(serial) || self.serial != serial {
                continue;
            }
            self.pending = None;
            self.grow_past.clear();
            self.grow_last = None;
            let Some(hover) = &self.hover else {
                continue;
            };
            match result {
                Ok(prediction) => {
                    let result = make_result(
                        hover.host,
                        hover.gen,
                        hover.source_key.clone(),
                        prediction.candidates,
                    );
                    if let Some(pinned) = self.pinned.as_mut().filter(|pinned| {
                        pinned.tab == hover.tab
                            && pinned.host == hover.host
                            && pinned.source_key == hover.source_key
                    }) {
                        if let Some(result) = result {
                            pinned.result = result;
                            self.error = None;
                        } else {
                            self.error = Some("No object found here. Try another point.".into());
                        }
                    } else {
                        self.result = result;
                        self.error = self
                            .result
                            .is_none()
                            .then(|| "No object found here. Try another point.".into());
                    }
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let (Some(tx), Some(queued)) = (&self.worker, self.queued.take()) {
            match tx.try_send(queued) {
                Ok(()) => {}
                Err(crossbeam_channel::TrySendError::Full(queued)) => {
                    self.queued = Some(queued);
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    self.error = Some("The local segmentation worker stopped.".into());
                    self.pending = None;
                }
            }
        }
    }

    pub(super) fn submit(&mut self, serial: u64, request: PendingImage) {
        self.error = None;
        self.pending = Some(serial);
        if let Some(tx) = &self.worker {
            match tx.try_send((serial, request)) {
                Ok(()) => {}
                Err(crossbeam_channel::TrySendError::Full(queued)) => {
                    self.queued = Some(queued);
                }
                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                    self.pending = None;
                    self.error = Some("The local segmentation worker stopped.".into());
                }
            }
        }
    }

    pub(super) fn start_worker(&mut self, ctx: egui::Context) {
        let (tx, rx) = crossbeam_channel::bounded::<(u64, PendingImage)>(1);
        let (done_tx, done_rx) = crossbeam_channel::unbounded();
        self.worker = Some(tx);
        self.done = Some(done_rx);
        std::thread::spawn(move || {
            let mut model = None;
            while let Ok((serial, request)) = rx.recv() {
                let result =
                    request
                        .prepare()
                        .and_then(|(request, prompts)| match model.as_mut() {
                            Some(model) => Segmenter::segment_points(model, &request, &prompts),
                            None => Segmenter::start(&atlas_core::index::data_dir()).and_then(
                                |mut started| {
                                    let result = started.segment_points(&request, &prompts);
                                    model = Some(started);
                                    result
                                },
                            ),
                        });
                if result.is_err() {
                    model = None;
                }
                if done_tx.send((serial, result)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
    }
}

fn make_result(
    host: NodeId,
    gen: u64,
    source_key: String,
    candidates: Vec<Candidate>,
) -> Option<SegmentResult> {
    let selected = candidates
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.quality.total_cmp(&b.quality))
        .map(|(index, _)| index)?;
    let contours = candidates.get(selected)?.contours.clone();
    let path = slate_doc::image_paint::region_path(contours.clone())?;
    Some(SegmentResult {
        host,
        gen,
        source_key,
        local: region_node(path),
        mask: std::sync::Arc::new(contours),
        candidates: std::sync::Arc::new(candidates),
        selected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(size: [usize; 2], mirror: [bool; 2]) -> PendingImage {
        PendingImage {
            key: "resident".into(),
            source_key: "resident:crop".into(),
            pixels: std::sync::Arc::new(egui::ColorImage::new(size, egui::Color32::RED)),
            mirror,
            prompts: vec![PointPrompt {
                point: [0.5, 0.5],
                positive: true,
            }],
            window: vec![],
        }
    }

    #[test]
    fn worker_bounds_rgb_payload_and_preserves_aspect_ratio() {
        let (request, prompts) = std::thread::spawn(|| pending([4096, 2048], [false; 2]).prepare())
            .join()
            .unwrap()
            .unwrap();
        assert_eq!([request.width, request.height], [768, 384]);
        assert_eq!(request.rgb.len(), 768 * 384 * 3);
        assert_eq!(&request.rgb[..3], &[255, 0, 0]);
        assert_eq!(request.point, [0.5, 0.5]);
        assert_eq!(prompts.len(), 1);
    }

    #[test]
    fn worker_mirrors_both_axes_before_inference() {
        let mut request = pending([2, 2], [true, true]);
        std::sync::Arc::get_mut(&mut request.pixels).unwrap().pixels = vec![
            egui::Color32::RED,
            egui::Color32::GREEN,
            egui::Color32::BLUE,
            egui::Color32::WHITE,
        ];
        let rgb = request.prepare().unwrap().0.rgb;
        assert_eq!(rgb, [255, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0]);
    }

    #[test]
    fn worker_rejects_a_preview_without_two_dimensions() {
        assert!(pending([1, 16], [false; 2]).prepare().is_err());
    }
}
