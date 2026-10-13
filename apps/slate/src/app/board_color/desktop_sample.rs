//! Desktop eyedropper session and destination routing.

use super::BrushSettingUndo;
use crate::app::{board::BoardTool, board_properties, SlateApp};
use atlas_shell::desktop_color::PickMode;
use eframe::egui;
use slate_doc::NodeId;

pub(crate) enum DesktopDestination {
    CanvasWires,
    Tool {
        background: bool,
        alt_selects_background: bool,
        brush_before: Option<BrushSettingUndo>,
    },
    Nodes {
        ids: Vec<NodeId>,
        panel: board_properties::Panel,
        preview: bool,
    },
}

pub struct DesktopSample {
    picker: atlas_shell::desktop_color::DesktopColorPicker,
    tab: u64,
    destination: DesktopDestination,
}

impl SlateApp {
    pub(crate) fn begin_desktop_sample(&mut self, destination: DesktopDestination, mode: PickMode) {
        if self.desktop_sample.is_some() {
            return;
        }
        // Headless input tests never create native windows or consume desktop clicks.
        #[cfg(test)]
        {
            self.desktop_sample_requests.push(mode);
            let _ = destination;
        }
        #[cfg(not(test))]
        {
            self.desktop_sample = Some(DesktopSample {
                picker: atlas_shell::desktop_color::DesktopColorPicker::begin(mode),
                tab: self.tab().id,
                destination,
            });
        }
    }

    pub(crate) fn start_tool_desktop_sample(&mut self, background: bool, temporary: bool) {
        self.begin_desktop_sample(
            DesktopDestination::Tool {
                background,
                alt_selects_background: self.board_tool == BoardTool::Eyedropper,
                brush_before: None,
            },
            if temporary {
                PickMode::AltHeld
            } else {
                PickMode::Click
            },
        );
    }

    #[allow(dead_code)] // No caller yet.
    pub(crate) fn start_node_desktop_sample(
        &mut self,
        ids: &[NodeId],
        panel: board_properties::Panel,
    ) {
        self.begin_desktop_sample(
            DesktopDestination::Nodes {
                ids: ids.to_vec(),
                panel,
                preview: false,
            },
            PickMode::Click,
        );
    }

    pub(crate) fn desktop_sample_frame(&mut self, ctx: &egui::Context) {
        self.maybe_start_brush_alt_sample(ctx);
        let Some(session) = &self.desktop_sample else {
            return;
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
        let Some(result) = session.picker.poll() else {
            return;
        };
        let session = self.desktop_sample.take().unwrap();
        let sample = match result {
            Ok(Some(sample)) => sample,
            Ok(None) => return,
            Err(error) => {
                self.toast(&error);
                return;
            }
        };
        if self.tab().id != session.tab {
            self.toast("Color pick canceled: the requesting workbook changed");
            return;
        }
        match session.destination {
            DesktopDestination::CanvasWires => self.set_canvas_wire_sample(ctx, sample.rgb),
            DesktopDestination::Tool {
                background,
                alt_selects_background,
                brush_before,
            } => self.finish_tool_sample(
                background || (alt_selects_background && sample.alt),
                sample.rgb,
                brush_before,
            ),
            DesktopDestination::Nodes {
                ids,
                panel,
                preview,
            } => self.finish_node_sample(ctx, ids, panel, preview, session.tab, sample.rgb),
        }
    }

    fn maybe_start_brush_alt_sample(&mut self, ctx: &egui::Context) {
        if self.desktop_sample.is_some() {
            return;
        }
        if !self.alt_down {
            self.desktop_alt_latched = false;
        }
        let secondary = ctx.input(|input| {
            input.pointer.button_down(egui::PointerButton::Secondary)
                || input.pointer.button_pressed(egui::PointerButton::Secondary)
        });
        let over_window = ctx.input(|input| input.pointer.hover_pos().is_some());
        // Alt+right-drag is the size / color HUD, so the fullscreen sampler
        // opens only after the pointer leaves this window.
        if self.board_tool == BoardTool::Brush
            && self.alt_down
            && !self.desktop_alt_latched
            && !over_window
            && self.brush_hud.is_none()
            && !secondary
        {
            self.desktop_alt_latched = true;
            self.start_tool_desktop_sample(false, true);
        }
    }

    fn finish_tool_sample(
        &mut self,
        background: bool,
        rgb: [u8; 3],
        brush_before: Option<BrushSettingUndo>,
    ) {
        let color = if background {
            &mut self.board_colors.bg
        } else {
            &mut self.board_colors.fg
        };
        color.0[..3].copy_from_slice(&rgb);
        self.save_board_colors();
        self.remember_document_colors(vec![rgb]);
        if let Some(before) = brush_before {
            self.push_brush_setting_undo(before);
        }
    }

    fn finish_node_sample(
        &mut self,
        ctx: &egui::Context,
        ids: Vec<NodeId>,
        panel: board_properties::Panel,
        preview: bool,
        tab: u64,
        rgb: [u8; 3],
    ) {
        use board_properties::{Property, PropertyRequest};
        if self.tab().read_only
            || ids
                .iter()
                .any(|id| self.doc().scene.node(*id).is_none_or(|node| node.locked))
        {
            self.toast("Color pick canceled: its target is no longer editable");
            return;
        }
        let edit = if panel == board_properties::Panel::Fill {
            Property::FillRgb(rgb)
        } else if panel == board_properties::Panel::Text {
            Property::TextRgb(rgb)
        } else {
            Property::StrokeRgb(rgb)
        };
        if preview
            && self.shape_properties.panel == Some(panel)
            && self.shape_properties.tab == tab
            && self.shape_properties.ids == ids
        {
            self.preview_shape_property(edit);
        } else if preview {
            self.toast("Color pick canceled: its selection changed");
        } else {
            let request = PropertyRequest {
                ids,
                edits: vec![edit],
                points: Vec::new(),
            };
            self.dispatch(
                ctx,
                atlas_commands::CommandId("board.shape.edit"),
                serde_json::to_string(&request).ok(),
            );
        }
    }
}
