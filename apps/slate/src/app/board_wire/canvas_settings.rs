//! Canvas-wide adapter for the shared connector and color controls.
use super::*;
use crate::app::board_properties::Property;
use atlas_shell::selection_tools as chrome;
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests;

type SharedSettings = Arc<Mutex<CanvasSettings>>;

fn state_id() -> egui::Id {
    egui::Id::new("canvas_wire_settings")
}

struct CanvasSettings {
    tab: u64,
    at: Pos2,
    rgba: [u8; 4],
    width: f32,
    routing: WireRouting,
    color_changed: bool,
    width_changed: bool,
    routing_changed: bool,
    color: chrome::ColorState,
}

impl CanvasSettings {
    fn edits(&self) -> Vec<Property> {
        let mut edits = Vec::new();
        if self.color_changed {
            edits.push(Property::StrokeRgb([
                self.rgba[0],
                self.rgba[1],
                self.rgba[2],
            ]));
            edits.push(Property::StrokeAlpha(self.rgba[3]));
        }
        if self.width_changed {
            edits.push(Property::StrokeWidth(
                self.width / slate_doc::scene::CONNECTOR_WIDTH_SCALE,
            ));
        }
        if self.routing_changed {
            edits.push(Property::WireRouting(self.routing));
        }
        edits
    }
}

impl SlateApp {
    pub(crate) fn open_canvas_wire_settings(&mut self, ctx: &egui::Context) -> bool {
        let first = self
            .doc()
            .scene
            .nodes
            .iter()
            .find_map(|node| match &node.kind {
                NodeKind::Connector(connector) if !node.locked => Some(connector),
                _ => None,
            });
        let settings = CanvasSettings {
            tab: self.tab().id,
            at: ctx
                .pointer_latest_pos()
                .unwrap_or(self.canvas_rect.center()),
            rgba: first.map_or(self.palette().wire.to_array(), |c| {
                c.paint_color(super::super::board::to_rgba(self.palette().wire))
                    .0
            }),
            width: first.map_or(2.0, |c| c.stroke.width) * slate_doc::scene::CONNECTOR_WIDTH_SCALE,
            routing: first.map_or(self.board_wire_routing, |c| {
                c.effective_routing(self.board_wire_routing)
            }),
            color_changed: false,
            width_changed: false,
            routing_changed: false,
            color: Default::default(),
        };
        ctx.data_mut(|data| data.insert_temp(state_id(), Arc::new(Mutex::new(settings))));
        true
    }

    pub(crate) fn set_canvas_wire_sample(&mut self, ctx: &egui::Context, rgb: [u8; 3]) {
        if let Some(state) = ctx.data(|data| data.get_temp::<SharedSettings>(state_id())) {
            let mut state = state.lock().unwrap();
            if state.tab == self.tab().id {
                state.rgba[..3].copy_from_slice(&rgb);
                state.color_changed = true;
            }
        }
    }

    /// Only edited properties are applied. Unrelated wire styles and the
    /// board selection survive, and the whole canvas change is one undo.
    pub(crate) fn apply_canvas_wire_properties(&mut self, edits: Vec<Property>) -> bool {
        let commands = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|node| {
                if node.locked || !matches!(node.kind, NodeKind::Connector(_)) {
                    return None;
                }
                let mut after = node.clone();
                for edit in &edits {
                    edit.apply(&mut after, None);
                }
                (after != *node).then(|| SceneCmd::Patch {
                    before: Box::new(node.clone()),
                    after: Box::new(after),
                })
            })
            .collect::<Vec<_>>();
        if commands.is_empty() {
            return false;
        }
        self.commit_scene(commands)
    }

    pub(crate) fn canvas_wire_settings_ui(&mut self, ctx: &egui::Context) {
        let Some(shared) = ctx.data(|data| data.get_temp::<SharedSettings>(state_id())) else {
            return;
        };
        let mut state = shared.lock().unwrap();
        if state.tab != self.tab().id {
            ctx.data_mut(|data| data.remove::<SharedSettings>(state_id()));
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut sample = None;
        let theme = self.palette();
        let recent = self.doc().view.recent_colors.clone().unwrap_or_default();
        egui::Window::new("Canvas wire settings")
            .id(state_id().with("window"))
            .default_pos(state.at)
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label("Apply changes to all unlocked wires on this canvas.");
                ui.add_enabled_ui(!self.tab().read_only, |ui| {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(chrome::EDITOR_WIDTH, chrome::STROKE_HEIGHT),
                        egui::Sense::hover(),
                    );
                    let rgba = state.rgba;
                    let width = state.width;
                    let edit = chrome::color_editor(
                        ui,
                        rect,
                        rgba,
                        Some(width),
                        false,
                        &recent,
                        &mut state.color,
                        1.0,
                        theme,
                    );
                    if let Some(rgb) = edit.rgb {
                        state.rgba[..3].copy_from_slice(&rgb);
                        state.color_changed = true;
                    }
                    if let Some(alpha) = edit.alpha {
                        state.rgba[3] = alpha;
                        state.color_changed = true;
                    }
                    if let Some(width) = edit.width {
                        state.width = width;
                        state.width_changed = true;
                    }
                    sample = edit.sample;
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(chrome::EDITOR_WIDTH, chrome::CAPSULE_HEIGHT),
                        egui::Sense::hover(),
                    );
                    let selected = usize::from(state.routing == WireRouting::Orthogonal);
                    let next = chrome::segments(
                        ui,
                        rect,
                        state_id().with("routing"),
                        ["Bézier", "Orthogonal"],
                        selected,
                        1.0,
                        theme,
                    );
                    if next != selected {
                        state.routing = if next == 1 {
                            WireRouting::Orthogonal
                        } else {
                            WireRouting::Bezier
                        };
                        state.routing_changed = true;
                    }
                    apply = ui
                        .add_enabled(
                            !state.edits().is_empty(),
                            egui::Button::new("Apply to all wires"),
                        )
                        .clicked();
                });
            });
        let edits = apply.then(|| state.edits());
        drop(state);
        if let Some(gesture) = sample {
            use crate::app::board_color::DesktopDestination;
            use atlas_shell::desktop_color::PickMode;
            self.begin_desktop_sample(
                DesktopDestination::CanvasWires,
                match gesture {
                    chrome::SampleGesture::Click => PickMode::Click,
                    chrome::SampleGesture::Drag => PickMode::Drag,
                },
            );
        }
        if let Some(edits) = edits {
            self.apply_canvas_wire_properties(edits);
            open = false;
        }
        if !open || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            ctx.data_mut(|data| data.remove::<SharedSettings>(state_id()));
        }
    }
}
