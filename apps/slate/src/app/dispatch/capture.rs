//! All Capture entry points share one source selection and lifecycle owner.
use super::*;
use crate::app::board_properties::Panel;

/// The top-level router delegates the family instead of mirroring every ID.
pub(super) fn owns_command(command: &str) -> bool {
    command == "portal.capture"
        || command.starts_with("portal.capture.")
        || command.starts_with("board.model_screenshot")
}

impl SlateApp {
    pub(super) fn dispatch_portal_capture(
        &mut self,
        ctx: &egui::Context,
        command: &str,
        detail: Option<&str>,
    ) -> bool {
        ctx.request_repaint();
        match command {
            "portal.capture.stop" | "portal.capture.pause" => {
                self.portal_capture_pause();
                return true;
            }
            "portal.capture.resume" => {
                self.portal_capture_resume();
                return true;
            }
            "portal.capture.save.file" => {
                self.portal_capture_video_save_file();
                return true;
            }
            "portal.capture.save.canvas" => {
                self.portal_capture_video_save_canvas();
                return true;
            }
            "portal.capture.cancel" => {
                self.portal_capture_cancel();
                if self.shape_properties.panel == Some(Panel::Capture) {
                    self.shape_properties.panel = None;
                }
                return true;
            }
            _ => {}
        }
        let node = if command.starts_with("board.model_screenshot") {
            self.selected_model_viewport()
        } else if let Some(detail) = detail {
            detail.parse::<u64>().ok().map(NodeId)
        } else if self.board_sel.len() == 1 {
            self.board_sel.iter().next().copied()
        } else {
            None
        };
        let Some(node) = node.filter(|id| {
            self.doc().scene.node(*id).is_some_and(|n| {
                matches!(n.kind, NodeKind::Portal(_)) || self.model_has_viewport(*id)
            })
        }) else {
            return false;
        };
        match command {
            "portal.capture" | "board.model_screenshot" => {
                self.open_portal_capture_menu(ctx, node);
            }
            "portal.capture.still.canvas" | "board.model_screenshot_canvas" => {
                self.portal_capture_still_canvas(node);
            }
            "portal.capture.still.file" | "board.model_screenshot_save" => {
                self.portal_capture_still_dialog(node);
            }
            "portal.capture.video.canvas" => self.portal_capture_video_canvas(node),
            "portal.capture.video.file" => self.portal_capture_video_dialog(node),
            "portal.capture.video.start" => self.portal_capture_video_start(node),
            _ => return false,
        }
        true
    }
}
