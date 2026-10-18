//! Capture commands and asynchronous save dialog identity checks.
use super::super::super::PickerMsg;
use super::*;
use atlas_shell::file_picker::{first, PickRequest};

impl SlateApp {
    pub(crate) fn portal_capture_still_canvas(&mut self, node: NodeId) {
        if self.at_home || self.tab().read_only {
            self.toast("Capture to Canvas needs an editable workbook.");
            return;
        }
        if self.model_has_viewport(node) {
            self.export_model_screenshot_canvas(node);
        } else {
            self.start_portal_capture(node, false, None);
        }
    }

    pub(crate) fn portal_capture_still_dialog(&mut self, node: NodeId) {
        self.portal_capture_dialog(node);
    }

    fn portal_capture_dialog(&mut self, node: NodeId) {
        let Some(source) = self.portal_capture_source(node) else {
            return;
        };
        let request = PickRequest::save()
            .file_name("portal-capture.png")
            .filter("PNG", &["png"])
            .filter("JPEG", &["jpg", "jpeg"])
            .filter("WebP", &["webp"]);
        self.picker
            .open(request, move |picked| PickerMsg::PortalCaptureSave {
                tab_id: source.tab,
                node,
                identity: source.identity,
                video: false,
                path: first(picked),
            });
    }

    pub(crate) fn finish_portal_capture_dialog(
        &mut self,
        tab_id: u64,
        node: NodeId,
        identity: String,
        video: bool,
        path: PathBuf,
    ) {
        let source = Source {
            tab: tab_id,
            node,
            identity,
        };
        if !self.portal_capture_matches(&source) {
            self.toast("Capture source changed while the save dialog was open.");
            return;
        }
        if video {
            self.toast("Start a recording, then Pause to choose its destination.");
            return;
        }
        let mut path = path;
        if path.extension().is_none() {
            path.set_extension(if video { "mp4" } else { "png" });
        }
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if if video {
            extension != "mp4"
        } else {
            !matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp")
        } {
            self.toast("Choose MP4 for video, or PNG, JPEG, or WebP for a still image.");
            return;
        }
        if !video && self.model_has_viewport(node) {
            self.finish_model_screenshot_save(node, path);
        } else {
            self.start_portal_capture(node, video, Some(path));
        }
    }

    pub(super) fn start_portal_capture(
        &mut self,
        node: NodeId,
        video: bool,
        destination: Option<PathBuf>,
    ) {
        let Some(source) = self.portal_capture_source(node) else {
            return;
        };
        if video && !self.portal_capture_rotation_supported(node) {
            self.toast("Reset the 3D viewport frame rotation before recording.");
            return;
        }
        if !video && destination.is_none() && self.tab().read_only {
            self.toast("Capture to Canvas needs an editable workbook.");
            return;
        }
        if self.portal_chrome.capture.session.is_some()
            || self.portal_capture_active_status().is_some()
        {
            self.toast("Stop or cancel the current capture first.");
            return;
        }
        if self.portal_chrome.capture.writes.len() >= 2 {
            self.toast("Two captures are still saving. Try again when one finishes.");
            return;
        }
        if self.agent_capture_picker_open(node) {
            self.toast("Close the portal chooser before capturing its contents.");
            return;
        }
        let Some(gl) = self.gl.clone() else {
            self.toast("Portal capture needs an available GPU renderer.");
            return;
        };
        let path = if video {
            output::recording_path(node)
        } else {
            destination
                .clone()
                .unwrap_or_else(|| output::capture_path(self.tab().path.as_deref(), node, "png"))
        };
        let runtime = &mut self.portal_chrome.capture;
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.gpu.get_or_insert_with(|| GpuCapture::new(gl));
        runtime.session = Some(Session {
            source,
            path,
            canvas: video || destination.is_none(),
            video,
            generation: runtime.generation,
            started: Instant::now(),
            last_sample: None,
            last_frame: None,
            skip_first_paint: video,
            size: None,
            encoder: None,
            saving: false,
            clock: RecordingClock::default(),
            destination: None,
        });
    }
}
