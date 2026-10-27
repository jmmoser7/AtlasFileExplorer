//! Capture bodies hosted by the shared object-property apron.
use super::*;
use crate::app::board_portal::capture::{CapturePhase, CaptureStatus};
use crate::app::board_properties::{EditorHost, Panel};
use atlas_commands::CommandId;
use atlas_shell::selection_tools::{
    self as chrome,
    capture::{self, Action, Phase},
};

impl SlateApp {
    pub(crate) fn open_portal_capture_menu(&mut self, ctx: &egui::Context, node: NodeId) {
        self.apply_shape_preview(ctx, true);
        if self.portal_chrome.maximized != Some(node) {
            self.board_sel = std::iter::once(node).collect();
        }
        self.open_object_panel(Panel::Capture);
        ctx.request_repaint();
    }

    /// The property selection owns launch lifetime, including tab/target changes.
    pub(crate) fn portal_capture_launch_source(&self) -> Option<NodeId> {
        if self.shape_properties.panel != Some(Panel::Capture)
            || self.shape_properties.tab != self.tab().id
            || self.portal_capture_active_status().is_some()
        {
            return None;
        }
        let [node] = self.shape_properties.ids.as_slice() else {
            return None;
        };
        self.doc().scene.node(*node).map(|_| *node)
    }

    pub(crate) fn paint_portal_capture_launch(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        node: NodeId,
        z: f32,
    ) {
        let response = capture::launch(
            ui,
            rect,
            self.portal_capture_can_video(node),
            z,
            self.palette(),
        );
        record_controls(ui.ctx(), &response);
        let owns = response.action.is_some() || capture_contains(ui.ctx(), rect, self.canvas_rect);
        let pass = ui.ctx().cumulative_pass_nr();
        ui.ctx().data_mut(|data| {
            data.insert_temp(Id::new("portal_capture_launch_pass"), (pass, node, owns))
        });
        #[cfg(test)]
        ui.ctx()
            .data_mut(|data| data.insert_temp(Id::new("portal_capture_launch_rect"), rect));
        if let Some(action) = response.action {
            let ctx = ui.ctx().clone();
            self.apply_shape_preview(&ctx, true);
            self.dispatch_capture_action(&ctx, node, action);
            if action == Action::Start
                && self
                    .portal_capture_active_status()
                    .is_some_and(|status| status.node == node)
            {
                self.open_object_panel(Panel::Capture);
            }
        }
    }

    /// Normal launch is painted by the property owner. Maximized launch and a
    /// selection-independent recording call the same attached-apron owner.
    pub(crate) fn paint_model_screenshot_popup(&mut self, ctx: &egui::Context) -> bool {
        let pass = ctx.cumulative_pass_nr();
        if let Some(status) = self.portal_capture_active_status() {
            if let Some((painted, node, owns)) = ctx.data(|data| {
                data.get_temp::<(u64, NodeId, bool)>(Id::new("portal_capture_transport_pass"))
            }) {
                if painted == pass && node == status.node {
                    return owns;
                }
            }
            return self.paint_portal_capture_transport(ctx, status);
        }
        if let Some((painted, _, owns)) = ctx.data(|data| {
            data.get_temp::<(u64, NodeId, bool)>(Id::new("portal_capture_launch_pass"))
        }) {
            if painted == pass {
                return owns;
            }
        }
        let Some(node) = self
            .portal_capture_launch_source()
            .filter(|node| self.portal_is_maximized(*node))
        else {
            return false;
        };
        let Some((bounds, z)) = self.portal_capture_source_geometry(node) else {
            return false;
        };
        let Some(host) = self.doc().scene.node(node).cloned() else {
            return false;
        };
        let height =
            capture::launch_height(self.portal_capture_can_video(node), bounds.width() / z);
        let surface = EditorHost {
            node: Some(&host),
            bounds,
            strip: chrome::blister_tab_rect(bounds, z, 1),
            zoom: z,
            blister: true,
            canvas: self.canvas_rect,
            theme: self.palette(),
        };
        let shown = surface.show(
            ctx,
            Id::new("shape_property_editor"),
            egui::LayerId::background(),
            (
                chrome::ImagePanelKind::Custom {
                    main_height: height,
                },
                height,
            ),
            true,
            |ui, rect, _| self.paint_portal_capture_launch(ui, rect, node, z),
        );
        shown.is_some_and(|shown| capture_contains(ctx, shown.area, self.canvas_rect))
    }

    fn paint_portal_capture_transport(
        &mut self,
        ctx: &egui::Context,
        status: CaptureStatus,
    ) -> bool {
        let pass = ctx.cumulative_pass_nr();
        let Some((source, z)) = self.portal_capture_source_geometry(status.node) else {
            return false;
        };
        let Some(host) = self.doc().scene.node(status.node).cloned() else {
            return false;
        };
        let mut bounds = source;
        let occupied = self.shape_properties.ids.as_slice() == [status.node]
            && self
                .shape_properties
                .panel
                .is_some_and(|panel| panel != Panel::Capture && panel != Panel::Pages)
            && !self.portal_is_maximized(status.node);
        if occupied {
            let panel = self.shape_properties.panel.unwrap();
            let painter = ctx.layer_painter(egui::LayerId::background());
            let kind = self.property_editor_kind(&painter, panel, source.width() / z);
            bounds.max.y = chrome::image_presentation_layout(source, z, kind)
                .overall
                .bottom();
        }
        let phase = phase(status.phase);
        let height = capture::active_height(phase, source.width() / z);
        let surface = EditorHost {
            node: (!occupied).then_some(&host),
            bounds,
            strip: chrome::blister_tab_rect(bounds, z, 1),
            zoom: z,
            blister: true,
            canvas: self.canvas_rect,
            theme: self.palette(),
        };
        let theme = self.palette();
        let shown = surface.show(
            ctx,
            Id::new(("portal_capture_transport", self.tab().id, status.node.0)),
            egui::LayerId::background(),
            (
                chrome::ImagePanelKind::Custom {
                    main_height: height,
                },
                height,
            ),
            true,
            |ui, rect, _| capture::active(ui, rect, phase, status.elapsed, z, theme),
        );
        let owns = shown.as_ref().is_some_and(|shown| {
            shown.inner.action.is_some() || capture_contains(ctx, shown.area, self.canvas_rect)
        });
        ctx.data_mut(|data| {
            data.insert_temp(
                Id::new("portal_capture_transport_pass"),
                (pass, status.node, owns),
            )
        });
        if let Some(shown) = shown {
            record_controls(ctx, &shown.inner);
            #[cfg(test)]
            ctx.data_mut(|data| {
                data.insert_temp(
                    Id::new("portal_capture_transport"),
                    (status.node, status.phase, shown.inner.bounds()),
                );
                data.insert_temp(
                    Id::new(("portal_capture_area", status.node.0, 0usize)),
                    shown.area,
                );
            });
            if let Some(action) = shown.inner.action {
                self.dispatch_capture_action(ctx, status.node, action);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        owns
    }

    fn dispatch_capture_action(&mut self, ctx: &egui::Context, node: NodeId, action: Action) {
        self.dispatch(ctx, CommandId(command(action)), Some(node.0.to_string()));
    }
}

fn command(action: Action) -> &'static str {
    match action {
        Action::StillCanvas => "portal.capture.still.canvas",
        Action::StillFile => "portal.capture.still.file",
        Action::Start => "portal.capture.video.start",
        Action::Pause => "portal.capture.pause",
        Action::Resume => "portal.capture.resume",
        Action::SaveFile => "portal.capture.save.file",
        Action::SaveCanvas => "portal.capture.save.canvas",
        Action::Discard => "portal.capture.cancel",
    }
}

fn phase(phase: CapturePhase) -> Phase {
    match phase {
        CapturePhase::Starting => Phase::Starting,
        CapturePhase::Recording => Phase::Recording,
        CapturePhase::Paused => Phase::Paused,
        CapturePhase::Saving => Phase::Saving,
    }
}

fn capture_contains(ctx: &egui::Context, rect: Rect, canvas: Rect) -> bool {
    ctx.input(|input| {
        let press = input
            .pointer
            .press_origin()
            .filter(|_| input.pointer.any_down());
        [input.pointer.latest_pos(), press]
            .into_iter()
            .flatten()
            .any(|point| rect.contains(point) && canvas.contains(point))
    })
}

fn record_controls(ctx: &egui::Context, response: &capture::Response) {
    #[cfg(test)]
    for (action, rect) in response.controls() {
        ctx.data_mut(|data| {
            data.insert_temp(Id::new(("portal_capture_action", command(*action))), *rect)
        });
    }
    #[cfg(not(test))]
    let _ = (ctx, response);
}

#[cfg(test)]
mod tests;
