//! Derived hover state and bounded model worker for image highlights.

use super::{region_node, SegmentResult};
use atlas_segment::{Request, Segmenter};
use eframe::egui::{self, Pos2, Rect};
use slate_doc::NodeId;
use std::time::Instant;

const SAMPLE_EDGE: usize = 768;

/// Sharing resident pixels makes dispatch constant-time; conversion stays off UI.
pub(super) struct PendingImage {
    pub key: String,
    pub pixels: std::sync::Arc<egui::ColorImage>,
    pub mirror: [bool; 2],
    pub point: [f32; 2],
    pub window: Vec<Vec<[f32; 2]>>,
}

impl PendingImage {
    fn prepare(self) -> Result<Request, String> {
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
        Ok(Request {
            key: self.key,
            width,
            height,
            rgb,
            point: self.point,
            window: self.window,
        })
    }
}

type WorkerReply = (u64, Result<Vec<Vec<[f32; 2]>>, String>);

/// Derived hover state for local image subject highlights; never journaled.
#[derive(Default)]
pub(crate) struct ImageSegmentRuntime {
    pub(super) hover: Option<Hover>,
    pub(super) result: Option<SegmentResult>,
    pub(super) serial: u64,
    pub(super) pending: Option<u64>,
    pub(super) worker: Option<crossbeam_channel::Sender<(u64, PendingImage)>>,
    pub(super) done: Option<crossbeam_channel::Receiver<WorkerReply>>,
    pub(super) action_rect: Option<Rect>,
    pub(super) corridor: Option<Rect>,
    pub(super) error: Option<String>,
    pub(super) dismissed: Option<Pos2>,
    /// Click on the highlight: capsule tag above the cursor. Derived.
    pub(super) tag_at: Option<Pos2>,
    /// The click that opened the tag must not dismiss it in the same paint.
    pub(super) tag_fresh: bool,
    /// Screen rects of the open tag. Review sheets read this.
    pub(crate) tag_rects: Vec<Rect>,
    /// Press on the highlight that may become a sticker drag. Derived.
    pub(super) grab: Option<Grab>,
    pub(super) drag_delta: Option<egui::Vec2>,
    /// Layer squircle at the top of the image. Derived; not a scene field.
    pub(crate) layers_menu: Option<NodeId>,
    pub(crate) layers_fresh: bool,
    pub(super) layer_rects: Vec<Rect>,
    /// `SLATE_SEGMENT_GROW=1` or a test sets this. Off by default.
    pub(crate) grow_debug: bool,
    pub(super) grow_past: Vec<std::sync::Arc<Vec<Vec<[f32; 2]>>>>,
    pub(super) grow_last: Option<Pos2>,
}

pub(super) struct Grab {
    pub(super) origin: Pos2,
    pub(super) world: Pos2,
    pub(super) moved: bool,
}

pub(super) struct Hover {
    pub(super) tab: u64,
    pub(super) host: NodeId,
    pub(super) gen: u64,
    pub(super) point_norm: [f32; 2],
    pub(super) screen: Pos2,
    pub(super) since: Instant,
}

impl ImageSegmentRuntime {
    /// A result, a running request, or an error is the visible offer.
    pub(super) fn offered(&self) -> bool {
        self.result.is_some() || self.pending.is_some() || self.error.is_some()
    }

    pub(super) fn clear(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        self.hover = None;
        self.result = None;
        self.pending = None;
        self.action_rect = None;
        self.corridor = None;
        self.tag_at = None;
        self.tag_fresh = false;
        self.tag_rects.clear();
        self.grab = None;
        self.drag_delta = None;
        self.grow_past.clear();
        self.grow_last = None;
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
            let Some(hover) = &self.hover else {
                continue;
            };
            match result {
                Ok(contours) => {
                    self.result =
                        slate_doc::image_paint::region_path(contours.clone()).map(|path| {
                            SegmentResult {
                                host: hover.host,
                                gen: hover.gen,
                                local: region_node(path),
                                mask: std::sync::Arc::new(contours),
                            }
                        });
                    // Empty masks do not spin inference on every frame.
                    if self.result.is_none() {
                        self.error = Some("No object found here. Try another point.".into());
                    }
                }
                Err(error) => self.error = Some(error),
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
                let result = request.prepare().and_then(|request| match model.as_mut() {
                    Some(model) => Segmenter::segment(model, &request),
                    None => {
                        Segmenter::start(&atlas_core::index::data_dir()).and_then(|mut started| {
                            let result = started.segment(&request);
                            model = Some(started);
                            result
                        })
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(size: [usize; 2], mirror: [bool; 2]) -> PendingImage {
        PendingImage {
            key: "resident".into(),
            pixels: std::sync::Arc::new(egui::ColorImage::new(size, egui::Color32::RED)),
            mirror,
            point: [0.5, 0.5],
            window: vec![],
        }
    }

    #[test]
    fn worker_bounds_rgb_payload_and_preserves_aspect_ratio() {
        let request = std::thread::spawn(|| pending([4096, 2048], [false; 2]).prepare())
            .join()
            .unwrap()
            .unwrap();
        assert_eq!([request.width, request.height], [768, 384]);
        assert_eq!(request.rgb.len(), 768 * 384 * 3);
        assert_eq!(&request.rgb[..3], &[255, 0, 0]);
        assert_eq!(request.point, [0.5, 0.5]);
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
        let rgb = request.prepare().unwrap().rgb;
        assert_eq!(rgb, [255, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0]);
    }

    #[test]
    fn worker_rejects_a_preview_without_two_dimensions() {
        assert!(pending([1, 16], [false; 2]).prepare().is_err());
    }
}
