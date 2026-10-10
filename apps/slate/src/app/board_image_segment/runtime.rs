//! Derived hover state and bounded model worker for image highlights.

use super::{region_node, SegmentResult};
use atlas_segment::{Request, Segmenter};
use eframe::egui::{self, Pos2, Rect};
use slate_doc::NodeId;
use std::time::Instant;

type WorkerReply = (u64, Result<Vec<Vec<[f32; 2]>>, String>);

#[derive(Default)]
pub(crate) struct ImageSegmentRuntime {
    pub(super) hover: Option<Hover>,
    pub(super) result: Option<SegmentResult>,
    pub(super) serial: u64,
    pub(super) pending: Option<u64>,
    pub(super) worker: Option<crossbeam_channel::Sender<(u64, Request)>>,
    pub(super) done: Option<crossbeam_channel::Receiver<WorkerReply>>,
    pub(super) action_rect: Option<Rect>,
    pub(super) corridor: Option<Rect>,
    pub(super) error: Option<String>,
    pub(super) dismissed: Option<Pos2>,
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
    pub(super) fn clear(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        self.hover = None;
        self.result = None;
        self.pending = None;
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
        let (tx, rx) = crossbeam_channel::bounded::<(u64, Request)>(1);
        let (done_tx, done_rx) = crossbeam_channel::unbounded();
        self.worker = Some(tx);
        self.done = Some(done_rx);
        std::thread::spawn(move || {
            let mut model = None;
            while let Ok((serial, request)) = rx.recv() {
                let result = match model.as_mut() {
                    Some(model) => Segmenter::segment(model, &request),
                    None => {
                        Segmenter::start(&atlas_core::index::data_dir()).and_then(|mut started| {
                            let result = started.segment(&request);
                            model = Some(started);
                            result
                        })
                    }
                };
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
