//! Per-frame capture work is bounded; encoding and publication stay off-thread.
use super::*;
use std::time::Duration;

impl SlateApp {
    pub(crate) fn maintain_portal_capture(&mut self, ctx: &egui::Context) {
        let invalid: Vec<bool> = self
            .portal_chrome
            .capture
            .writes
            .iter()
            .map(|write| !self.portal_capture_matches(&write.source))
            .collect();
        for (write, invalid) in self.portal_chrome.capture.writes.iter_mut().zip(invalid) {
            write.cancelled |= invalid;
        }
        self.finish_portal_capture_writes();
        if !self.portal_chrome.capture.writes.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        let Some(session) = self.portal_chrome.capture.session.as_ref() else {
            return;
        };
        if !self.portal_capture_matches(&session.source)
            || self.agent_capture_picker_open(session.source.node)
            || (session.video && !self.portal_capture_rotation_supported(session.source.node))
        {
            self.fail_portal_capture("Cancelled because the source tab or portal changed.");
            return;
        }
        if let Some(error) = self
            .portal_chrome
            .capture
            .gpu
            .as_ref()
            .and_then(GpuCapture::take_error)
        {
            self.fail_portal_capture(error);
            return;
        }
        let result = self
            .portal_chrome
            .capture
            .session
            .as_mut()
            .and_then(|s| s.encoder.as_mut())
            .and_then(VideoEncoder::take_result);
        if let Some(result) = result {
            let session = self.portal_chrome.capture.session.take().unwrap();
            if let Some(gpu) = self.portal_chrome.capture.gpu.as_ref() {
                gpu.cancel();
            }
            self.queue_portal_capture_publication(session, result);
            ctx.request_repaint_after(Duration::from_millis(33));
            return;
        }
        if let Some(frame) = self
            .portal_chrome
            .capture
            .gpu
            .as_mut()
            .and_then(GpuCapture::take_frame)
        {
            self.accept_portal_capture_frame(frame);
        }
        if !self.portal_chrome.capture.writes.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        let Some(session) = self.portal_chrome.capture.session.as_ref() else {
            return;
        };
        if !session.saving
            && !session.clock.is_paused()
            && ((session.size.is_none() && session.started.elapsed() > Duration::from_secs(5))
                || session
                    .last_frame
                    .is_some_and(|last| last.elapsed() > Duration::from_secs(1)))
        {
            self.fail_portal_capture(
                "No visible portal frame arrived. Keep the whole portal in view.",
            );
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(if session.clock.is_paused() {
            100
        } else {
            16
        }));
    }

    pub(crate) fn paint_portal_capture(
        &mut self,
        painter: &egui::Painter,
        node: NodeId,
        rect: egui::Rect,
    ) {
        // Embedded Slate boards have their own NodeId namespace.
        if self.slate_boards.nesting() {
            return;
        }
        // A loading live 3D viewport paints a fallback card, not its model.
        if self.model_has_viewport(node)
            && self
                .model3d
                .live
                .get(&node)
                .is_some_and(|view| view.rendered.is_none())
        {
            return;
        }
        let runtime = &mut self.portal_chrome.capture;
        let Some(session) = runtime.session.as_mut() else {
            return;
        };
        if session.source.node != node || session.saving || session.clock.is_paused() {
            return;
        }
        if std::mem::take(&mut session.skip_first_paint) {
            // A new maximized assembly is fitted on its next paint. Wait for
            // that source transform before cropping the first video frame.
            return;
        }
        let Some(gpu) = runtime.gpu.as_ref() else {
            return;
        };
        if session
            .last_sample
            .is_none_or(|last| last.elapsed() >= Duration::from_secs_f64(1.0 / 30.0))
        {
            gpu.request(
                session.generation,
                if session.video {
                    [1920, 1080]
                } else {
                    [4096, 4096]
                },
                session.size,
            );
            session.last_sample = Some(Instant::now());
        }
        gpu.paint(painter, rect);
    }

    pub(super) fn accept_portal_capture_frame(&mut self, mut frame: CapturedFrame) {
        let Some(session) = self.portal_chrome.capture.session.as_mut() else {
            return;
        };
        if session.generation != frame.generation || session.saving || session.clock.is_paused() {
            return;
        }
        session.last_frame = Some(frame.captured_at);
        if session.video {
            let Some(projected) = session.clock.project_frame(frame.captured_at) else {
                return;
            };
            frame.captured_at = projected;
        }
        session.size = Some([frame.w, frame.h]);
        if session.video {
            if session.encoder.is_none() {
                match VideoEncoder::start_at(
                    session.path.clone(),
                    frame.w,
                    frame.h,
                    30,
                    frame.captured_at,
                ) {
                    Ok(encoder) => session.encoder = Some(encoder),
                    Err(error) => {
                        self.fail_portal_capture(error);
                        return;
                    }
                }
            }
            session.encoder.as_mut().unwrap().try_frame(frame);
        } else {
            let session = self.portal_chrome.capture.session.take().unwrap();
            if let Some(gpu) = self.portal_chrome.capture.gpu.as_ref() {
                gpu.cancel();
            }
            let (tx, result) = crossbeam_channel::bounded(1);
            let path = session.path;
            let canvas = session.canvas;
            let spawn = std::thread::Builder::new()
                .name("portal-still-write".into())
                .spawn(move || {
                    let written =
                        output::write_still(&path, &frame).and_then(|_| ReadyFile::read(path));
                    if let Err(error) = tx.try_send(written) {
                        if canvas {
                            if let Ok(file) = error.into_inner() {
                                let _ = std::fs::remove_file(file.path);
                            }
                        }
                    }
                });
            match spawn {
                Ok(_) => self.portal_chrome.capture.writes.push(StillWrite {
                    source: session.source,
                    canvas: session.canvas,
                    cancelled: false,
                    size: session.size.unwrap_or([1, 1]),
                    result,
                    resolved: false,
                    video_elapsed: None,
                }),
                Err(error) => self.toast(format!("Capture could not start saving: {error}")),
            }
        }
    }

    fn finish_portal_capture_writes(&mut self) {
        let mut ready = Vec::new();
        self.portal_chrome.capture.writes.retain_mut(|write| {
            let result = match write.result.try_recv() {
                Ok(result) => result,
                Err(crossbeam_channel::TryRecvError::Empty) => return true,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    Err("Image writer stopped before saving.".into())
                }
            };
            ready.push((
                write.source.clone(),
                write.canvas,
                write.cancelled,
                write.size,
                result,
            ));
            write.resolved = true;
            false
        });
        for (source, canvas, cancelled, size, result) in ready {
            if canvas && cancelled {
                if let Ok(file) = result {
                    std::thread::spawn(move || {
                        let _ = std::fs::remove_file(file.path);
                    });
                }
            } else {
                self.complete_portal_capture(source, canvas, size, result);
            }
        }
    }

    pub(super) fn complete_portal_capture(
        &mut self,
        source: Source,
        canvas: bool,
        size: [u32; 2],
        result: Result<ReadyFile, String>,
    ) {
        match result {
            Ok(file) if canvas => {
                if !self.portal_capture_matches(&source) || self.tab().read_only {
                    // Only auto-generated Canvas captures belong to this cleanup.
                    std::thread::spawn(move || {
                        let _ = std::fs::remove_file(file.path);
                    });
                    self.toast(
                        "Capture finished after its source changed; no canvas item was added.",
                    );
                } else {
                    let item = self.item_for_ready_path(&file.path, file.size, file.mtime);
                    if let Some(id) = self.place_capture_pixels_beside(source.node, item, size) {
                        self.board_sel.clear();
                        self.board_sel.insert(id);
                        self.toast("Capture added to Canvas");
                    }
                }
            }
            Ok(file) => self.toast(format!("Capture saved to {}", file.path.display())),
            Err(error) => self.toast(format!("Capture failed: {error}")),
        }
    }

    fn queue_portal_capture_publication(
        &mut self,
        session: Session,
        result: Result<PathBuf, String>,
    ) {
        let source = session.source;
        let size = session.size.unwrap_or([1, 1]);
        let video_elapsed = Some(session.clock.elapsed(Instant::now()).as_secs_f32());
        let destination = session.destination;
        let canvas = destination.as_ref().map_or(session.canvas, |d| d.canvas);
        let path = match result {
            Ok(path) => path,
            Err(error) => {
                self.complete_portal_capture(source, canvas, size, Err(error));
                return;
            }
        };
        let (tx, result) = crossbeam_channel::bounded(1);
        let owned_path = destination.as_ref().map(|_| path.clone());
        let spawn = std::thread::Builder::new()
            .name("portal-capture-metadata".into())
            .spawn(move || {
                let ready = if let Some(destination) = destination {
                    let published =
                        output::publish_video(&path, destination.path).and_then(ReadyFile::read);
                    let _ = std::fs::remove_file(path);
                    published
                } else {
                    ReadyFile::read(path)
                };
                if let Err(error) = tx.try_send(ready) {
                    if canvas {
                        if let Ok(file) = error.into_inner() {
                            let _ = std::fs::remove_file(file.path);
                        }
                    }
                }
            });
        match spawn {
            Ok(_) => self.portal_chrome.capture.writes.push(StillWrite {
                source,
                canvas,
                cancelled: false,
                size,
                result,
                resolved: false,
                video_elapsed,
            }),
            Err(error) => {
                if let Some(path) = owned_path {
                    std::thread::spawn(move || {
                        let _ = std::fs::remove_file(path);
                    });
                }
                self.toast(format!("Capture could not finish: {error}"));
            }
        }
    }
}
