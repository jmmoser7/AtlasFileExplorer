//! Model strip actions keep their registered command dispatch unchanged.
use super::*;

impl SlateApp {
    pub(super) fn model_capture_context(&mut self, response: &egui::Response, still: bool) {
        let dark = self.dark_mode;
        let metrics = crate::app::board::board_menu::layout().metrics;
        let to_file = if still {
            &mut self.portal_chrome.capture_still_to_file
        } else {
            &mut self.portal_chrome.capture_video_to_file
        };
        let canvas = if still {
            "Save image to canvas"
        } else {
            "Save video to canvas"
        };
        response.context_menu(|ui| {
            use atlas_shell::menu::{MenuIcon, Row};

            metrics.prepare(ui, dark);
            for (label, file) in [(canvas, false), ("File", true)] {
                if metrics
                    .row(
                        ui,
                        Row::new(MenuIcon::None, label).checked(*to_file == Some(file)),
                        dark,
                    )
                    .clicked()
                {
                    *to_file = Some(file);
                    ui.close_menu();
                }
            }
        });
    }

    pub(super) fn model_connector_context(
        &self,
        response: &egui::Response,
        model: Option<NodeId>,
        enabled: bool,
    ) -> (Option<NodeId>, Option<NodeId>, Option<NodeId>) {
        let mut actions = (None, None, None);
        response.context_menu(|ui| {
            if ui.add_enabled(enabled && model.is_some(), egui::Button::new("Set up headless model")).clicked() {
                actions.0 = model;
                ui.close_menu();
            }
            let available = model.is_some_and(|id| self.attached_model_native_access(id).is_some());
            if ui.add_enabled(enabled && available, egui::Button::new("Board access…"))
                .on_hover_text("Explicit native source scope and Resume. Provider Full access does not grant model access.")
                .clicked() {
                actions.1 = model;
                ui.close_menu();
            }
            ui.separator();
            if ui.add_enabled(enabled && model.is_some(), egui::Button::new("Open isolated Rhino copy")).clicked() {
                actions.2 = model;
                ui.close_menu();
            }
        });
        actions
    }
    pub(super) fn run_model_strip_actions(
        &mut self,
        ctx: &egui::Context,
        requested_measure: bool,
        requested_connector: bool,
        requested_connector_draft: Option<NodeId>,
        requested_maximize: bool,
        requested_screenshot: bool,
    ) {
        if requested_measure {
            self.apply_shape_preview(ctx, true);
            self.shape_properties.number = None;
            if let Some(id) = self.shape_properties.ids.first().copied() {
                self.dispatch(
                    ctx,
                    CommandId("board.model_measure"),
                    Some(id.0.to_string()),
                );
            }
        }
        if requested_connector {
            if let Some(id) = self.shape_properties.ids.first().copied() {
                let command = if self.application_connector_active(id) {
                    "board.model.connector.deactivate"
                } else {
                    "board.model.connector.activate"
                };
                self.dispatch(ctx, CommandId(command), Some(id.0.to_string()));
            }
        }
        if let Some(id) = requested_connector_draft {
            self.dispatch(
                ctx,
                CommandId("board.model.connector.activate_draft"),
                Some(id.0.to_string()),
            );
        }
        if requested_maximize {
            self.dispatch(ctx, CommandId("portal.maximize"), None);
        }
        if requested_screenshot {
            let was_open = self.shape_properties.panel == Some(Panel::Capture);
            self.apply_shape_preview(ctx, true);
            self.shape_properties.number = None;
            if let Some(id) = self
                .shape_properties
                .ids
                .first()
                .copied()
                .filter(|_| !was_open)
            {
                let command = match self.portal_chrome.capture_still_to_file {
                    Some(true) if self.model_has_viewport(id) => "portal.capture.still.file",
                    Some(false) if self.model_has_viewport(id) => "portal.capture.still.canvas",
                    _ => "portal.capture",
                };
                self.dispatch(ctx, CommandId(command), Some(id.0.to_string()));
            }
        }
    }
}
