//! Recording transport and deferred destination selection share one session.
use super::super::super::PickerMsg;
use super::*;
use atlas_shell::file_picker::{first, PickRequest};

impl SlateApp {
    pub(crate) fn portal_capture_video_start(&mut self, node: NodeId) {
        self.start_portal_capture(node, true, None);
    }

    /// Compatibility entry points now use the same deferred destination flow.
    pub(crate) fn portal_capture_video_canvas(&mut self, node: NodeId) {
        self.portal_capture_video_start(node);
    }

    pub(crate) fn portal_capture_video_dialog(&mut self, node: NodeId) {
        self.portal_capture_video_start(node);
    }

    pub(crate) fn portal_capture_pause(&mut self) {
        let Some(session) = self.portal_chrome.capture.session.as_mut() else {
            return;
        };
        if !session.video || session.saving || session.clock.is_paused() {
            return;
        }
        session.clock.pause(Instant::now());
        if let Some(gpu) = self.portal_chrome.capture.gpu.as_ref() {
            gpu.cancel();
        }
    }

    pub(crate) fn portal_capture_resume(&mut self) {
        let runtime = &mut self.portal_chrome.capture;
        let Some(session) = runtime.session.as_mut() else {
            return;
        };
        if !session.video || session.saving || !session.clock.is_paused() {
            return;
        }
        let now = Instant::now();
        session.clock.resume(now);
        session.started = now;
        session.last_sample = None;
        session.last_frame = session.size.map(|_| now);
        session.skip_first_paint = true;
        // Old readbacks and old save-dialog completions cannot enter this span.
        runtime.generation = runtime.generation.wrapping_add(1);
        session.generation = runtime.generation;
        if let Some(encoder) = session.encoder.as_mut() {
            encoder.resume_generation(session.generation);
        }
    }

    pub(crate) fn portal_capture_video_save_canvas(&mut self) {
        if self.at_home || self.tab().read_only {
            self.toast("Capture to Canvas needs an editable workbook.");
            return;
        }
        let Some(session) = self.portal_chrome.capture.session.as_ref() else {
            return;
        };
        let path = output::capture_path(self.tab().path.as_deref(), session.source.node, "mp4");
        self.choose_portal_capture_video_destination(path, true);
    }

    pub(crate) fn portal_capture_video_save_file(&mut self) {
        let Some(session) = self.portal_chrome.capture.session.as_ref() else {
            return;
        };
        if !session.video || session.saving || !session.clock.is_paused() {
            return;
        }
        let source = session.source.clone();
        let generation = session.generation;
        self.picker.open(
            PickRequest::save()
                .file_name("portal-capture.mp4")
                .filter("MP4 video", &["mp4"]),
            move |picked| PickerMsg::PortalCaptureVideoSave {
                tab_id: source.tab,
                node: source.node,
                identity: source.identity,
                generation,
                path: first(picked),
            },
        );
    }

    pub(crate) fn finish_portal_capture_video_dialog(
        &mut self,
        tab_id: u64,
        node: NodeId,
        identity: String,
        generation: u64,
        mut path: PathBuf,
    ) {
        let source = Source {
            tab: tab_id,
            node,
            identity,
        };
        let matches = self.portal_capture_matches(&source)
            && self
                .portal_chrome
                .capture
                .session
                .as_ref()
                .is_some_and(|s| {
                    s.source == source
                        && s.generation == generation
                        && s.clock.is_paused()
                        && !s.saving
                });
        if !matches {
            self.toast("The paused recording changed while the save dialog was open.");
            return;
        }
        if path.extension().is_none() {
            path.set_extension("mp4");
        }
        if !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("mp4"))
        {
            self.toast("Choose MP4 for a portal video recording.");
            return;
        }
        self.choose_portal_capture_video_destination(path, false);
    }

    fn choose_portal_capture_video_destination(&mut self, path: PathBuf, canvas: bool) {
        let Some(session) = self.portal_chrome.capture.session.as_mut() else {
            return;
        };
        if !session.video || session.saving || !session.clock.is_paused() {
            return;
        }
        let Some(encoder) = session.encoder.as_mut() else {
            self.toast("The portal has not produced a frame to save yet. Resume and try again.");
            return;
        };
        encoder.finish_at(session.clock.stopped_at(Instant::now()));
        session.destination = Some(VideoDestination { path, canvas });
        session.saving = true;
    }
}
