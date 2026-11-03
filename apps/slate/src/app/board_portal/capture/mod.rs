//! One capture lifecycle for every portal; pixels come from its actual paint.
use std::path::PathBuf;
use std::time::Instant;

use crossbeam_channel::Receiver;
use eframe::egui;
use slate_doc::scene::{NodeId, NodeKind};

use super::super::SlateApp;
use crate::app::items::ReadyFile;

mod actions;
mod clock;
mod encode;
mod gpu;
pub(crate) mod output;
mod pump;
#[cfg(test)]
mod tests;
mod video;

use clock::RecordingClock;
pub(crate) use encode::CapturedFrame;
use encode::VideoEncoder;
use gpu::GpuCapture;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CapturePhase {
    Starting,
    Recording,
    Paused,
    Saving,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CaptureStatus {
    pub node: NodeId,
    pub elapsed: f32,
    pub phase: CapturePhase,
}

struct VideoDestination {
    path: PathBuf,
    canvas: bool,
}

#[derive(Default)]
pub(crate) struct CaptureRuntime {
    gpu: Option<GpuCapture>,
    session: Option<Session>,
    writes: Vec<StillWrite>,
    generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct Source {
    tab: u64,
    node: NodeId,
    identity: String,
}

struct Session {
    source: Source,
    path: PathBuf,
    canvas: bool,
    video: bool,
    generation: u64,
    started: Instant,
    last_sample: Option<Instant>,
    last_frame: Option<Instant>,
    skip_first_paint: bool,
    size: Option<[u32; 2]>,
    encoder: Option<VideoEncoder>,
    saving: bool,
    clock: RecordingClock,
    destination: Option<VideoDestination>,
}

struct StillWrite {
    source: Source,
    canvas: bool,
    cancelled: bool,
    size: [u32; 2],
    result: Receiver<Result<ReadyFile, String>>,
    resolved: bool,
    video_elapsed: Option<f32>,
}

impl Drop for StillWrite {
    fn drop(&mut self) {
        if self.canvas && !self.resolved {
            // Retain a receiver across shutdown so a late publication always
            // has a cleanup owner. User-chosen File outputs are preserved.
            let result = self.result.clone();
            std::thread::spawn(move || {
                if let Ok(Ok(file)) = result.recv() {
                    let _ = std::fs::remove_file(file.path);
                }
            });
        }
    }
}

fn cancel_recording(mut session: Session) {
    if let Some(mut encoder) = session.encoder.take() {
        encoder.cancel();
        if session.canvas {
            // Cancellation can race the worker's final atomic publication.
            // Keep its result receiver until terminal success/failure.
            std::thread::spawn(move || loop {
                if let Some(result) = encoder.take_result() {
                    if let Ok(path) = result {
                        let _ = std::fs::remove_file(path);
                    }
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            });
        }
    }
}

impl Drop for CaptureRuntime {
    fn drop(&mut self) {
        if let Some(session) = self.session.take() {
            cancel_recording(session);
        }
    }
}

impl SlateApp {
    /// The framebuffer belongs to this source until its capture session ends.
    /// Publication workers no longer sample the viewport and do not own it.
    pub(crate) fn portal_capture_owns_viewport(&self, node: NodeId) -> bool {
        !self.at_home
            && !self.slate_nesting()
            && self
                .portal_chrome
                .capture
                .session
                .as_ref()
                .is_some_and(|session| {
                    session.source.tab == self.tab().id && session.source.node == node
                })
    }

    fn portal_capture_source(&self, node: NodeId) -> Option<Source> {
        if self.at_home {
            return None;
        }
        let n = self.doc().scene.node(node)?;
        let identity = match &n.kind {
            NodeKind::Portal(p) => {
                let agent = p.agent.as_ref().map(|a| (&a.provider, &a.session));
                serde_json::to_string(&(p.kind, &p.source, agent)).ok()?
            }
            NodeKind::Image(_)
                if self.model_has_viewport(node) || self.application_connector_active(node) =>
            {
                let model = self.model_node_info(node)?;
                format!("model:{}:{}", model.cache_key, model.path.display())
            }
            _ => return None,
        };
        Some(Source {
            tab: self.tab().id,
            node,
            identity,
        })
    }

    fn portal_capture_matches(&self, source: &Source) -> bool {
        self.portal_capture_source(source.node).as_ref() == Some(source)
    }

    pub(crate) fn portal_capture_can_video(&self, node: NodeId) -> bool {
        !self.application_connector_active(node)
            && self.portal_capture_source(node).is_some()
            && self.portal_capture_rotation_supported(node)
    }

    fn portal_capture_rotation_supported(&self, node: NodeId) -> bool {
        self.doc().scene.node(node).is_some_and(|node| {
            matches!(node.kind, NodeKind::Portal(_)) || {
                let angle = node.rotation_deg.rem_euclid(360.0);
                angle.is_finite() && !(0.001..=359.999).contains(&angle)
            }
        })
    }

    pub(crate) fn portal_capture_status(&self, node: NodeId) -> Option<CaptureStatus> {
        self.portal_capture_active_status()
            .filter(|status| status.node == node)
    }

    pub(crate) fn portal_capture_active_status(&self) -> Option<CaptureStatus> {
        let runtime = &self.portal_chrome.capture;
        if let Some(session) = runtime.session.as_ref().filter(|s| s.video) {
            if session.source.tab != self.tab().id || self.at_home {
                return None;
            }
            return Some(CaptureStatus {
                node: session.source.node,
                elapsed: session.clock.elapsed(Instant::now()).as_secs_f32(),
                phase: if session.saving {
                    CapturePhase::Saving
                } else if session.clock.is_paused() {
                    CapturePhase::Paused
                } else if session.size.is_some() {
                    CapturePhase::Recording
                } else {
                    CapturePhase::Starting
                },
            });
        }
        runtime.writes.iter().find_map(|write| {
            let elapsed = write.video_elapsed?;
            (!write.cancelled && write.source.tab == self.tab().id && !self.at_home).then_some(
                CaptureStatus {
                    node: write.source.node,
                    elapsed,
                    phase: CapturePhase::Saving,
                },
            )
        })
    }

    /// Legacy Stop now enters the destination phase, just like Pause.
    #[cfg(test)]
    pub(crate) fn portal_capture_stop(&mut self) {
        self.portal_capture_pause();
    }

    pub(crate) fn portal_capture_cancel(&mut self) {
        if let Some(session) = self.portal_chrome.capture.session.take() {
            cancel_recording(session);
        }
        if let Some(gpu) = self.portal_chrome.capture.gpu.as_ref() {
            gpu.cancel();
        }
        for write in &mut self.portal_chrome.capture.writes {
            if write.video_elapsed.is_some() {
                write.cancelled = true;
            }
        }
    }

    pub(crate) fn fail_portal_capture(&mut self, reason: impl AsRef<str>) {
        self.portal_capture_cancel();
        self.toast(format!("Capture: {}", reason.as_ref()));
    }

    pub(crate) fn portal_capture_shutdown(&mut self) {
        self.portal_capture_cancel();
        self.portal_chrome.capture.writes.clear();
        if let Some(gpu) = self.portal_chrome.capture.gpu.take() {
            gpu.destroy();
        }
    }
}

impl SlateApp {
    /// Foreign captures rejoin the existing publication and journal placement owner.
    pub(crate) fn finish_external_portal_capture(
        &mut self,
        node: NodeId,
        file: ReadyFile,
        size: [u32; 2],
        canvas: bool,
        author: slate_doc::scene::CmdAuthor,
    ) {
        let Some(source) = self.portal_capture_source(node) else {
            return;
        };
        self.complete_portal_capture_as(source, canvas, size, Ok(file), author);
    }
}
