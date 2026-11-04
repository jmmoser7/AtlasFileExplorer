//! Model strip actions keep their registered command dispatch unchanged.
use super::*;

impl SlateApp {
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
                self.dispatch(ctx, CommandId("portal.capture"), Some(id.0.to_string()));
            }
        }
    }
}
