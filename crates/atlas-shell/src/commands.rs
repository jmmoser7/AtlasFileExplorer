//! Shared command-reference plumbing and canvas navigation helpers.
//!
//! **Rule:** every user-facing shortcut or input command in an Atlas app must
//! be registered in that app's `ENTRIES` table (a `&[CommandEntry]`) and will
//! appear automatically in Advanced → Commands & shortcuts via
//! [`shortcuts_reference_ui`]. See each app's `COMMANDS.md`.

use eframe::egui::{self, Pos2, Rect, Ui, Vec2};

/// One row in the Advanced settings command reference.
pub struct CommandEntry {
    pub category: &'static str,
    pub name: &'static str,
    pub binding: &'static str,
}

/// Speed multiplier for turbo pan: screen-space pull distance → px/frame.
pub const TURBO_PAN_GAIN: f32 = 0.12;
/// Minimum pull before turbo pan engages (distinguishes from a right-click tap).
pub const TURBO_PAN_ENGAGE_PX: f32 = 4.0;
/// Minimum movement before the pan axis locks to horizontal or vertical.
pub const TURBO_PAN_AXIS_LOCK_PX: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PanAxis {
    Horizontal,
    Vertical,
}

/// Ctrl + right-drag turbo pan state (per canvas interaction).
#[derive(Default)]
pub struct TurboPanState {
    anchor: Option<Pos2>,
    axis: Option<PanAxis>,
    /// Pull exceeded [`TURBO_PAN_ENGAGE_PX`]; suppresses the file context menu.
    engaged: bool,
    /// Set on release after an engaged turbo pan; cleared once the menu decision runs.
    suppress_menu: bool,
}

impl TurboPanState {
    pub fn should_suppress_context_menu(&self) -> bool {
        self.suppress_menu
    }

    pub fn acknowledge_context_menu(&mut self) {
        self.suppress_menu = false;
    }

    /// Apply turbo pan while Ctrl and the secondary button are held. Returns `true`
    /// when panning is active this frame (left-drag pan should be skipped).
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        canvas_rect: Rect,
        pointer: Option<Pos2>,
        cam_offset: &mut Vec2,
    ) -> bool {
        let (secondary_down, secondary_released, ctrl) = ctx.input(|i| {
            (
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.button_released(egui::PointerButton::Secondary),
                i.modifiers.ctrl,
            )
        });

        if secondary_released {
            let was_engaged = self.engaged;
            self.reset();
            if was_engaged {
                self.suppress_menu = true;
            }
            return was_engaged;
        }

        if self.anchor.is_some() && !ctrl {
            self.reset();
            return false;
        }

        let Some(p) = pointer else {
            return self.engaged;
        };
        if !canvas_rect.contains(p) {
            return self.engaged;
        }

        if secondary_down && ctrl && self.anchor.is_none() {
            self.anchor = Some(p);
            self.axis = None;
            self.engaged = false;
        }

        let Some(anchor) = self.anchor else {
            return false;
        };

        if !secondary_down || !ctrl {
            return false;
        }

        let delta = p - anchor;
        if !self.engaged && delta.length() >= TURBO_PAN_ENGAGE_PX {
            self.engaged = true;
        }

        if self.axis.is_none() && delta.length() >= TURBO_PAN_AXIS_LOCK_PX {
            self.axis = Some(if delta.x.abs() >= delta.y.abs() {
                PanAxis::Horizontal
            } else {
                PanAxis::Vertical
            });
        }

        if self.engaged {
            let (dx, dy) = match self.axis {
                Some(PanAxis::Horizontal) => (delta.x, 0.0),
                Some(PanAxis::Vertical) => (0.0, delta.y),
                None => (0.0, 0.0),
            };
            cam_offset.x += dx * TURBO_PAN_GAIN;
            cam_offset.y += dy * TURBO_PAN_GAIN;
            ctx.request_repaint();
        }

        self.engaged
    }

    fn reset(&mut self) {
        self.anchor = None;
        self.axis = None;
        self.engaged = false;
    }
}

/// Reference table for Advanced settings. `source_hint` names the file where
/// the app's `ENTRIES` table lives so contributors keep it complete.
pub fn shortcuts_reference_ui(ui: &mut Ui, entries: &[CommandEntry], source_hint: &str) {
    let visuals = ui.visuals();
    let hint = visuals.weak_text_color();
    let name_color = visuals.text_color();
    let binding_color = visuals.weak_text_color();

    ui.label(egui::RichText::new("Commands & shortcuts").small().strong());
    ui.label(
        egui::RichText::new(format!(
            "All bindings below are defined in {source_hint} — add new \
             commands there so this list stays complete."
        ))
        .small()
        .color(hint),
    );
    ui.add_space(6.0);

    let mut last_category = "";
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .id_salt("commands_reference")
        .show(ui, |ui| {
            for entry in entries {
                if entry.category != last_category {
                    if !last_category.is_empty() {
                        ui.add_space(6.0);
                    }
                    ui.label(egui::RichText::new(entry.category).small().strong());
                    last_category = entry.category;
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(entry.name).small().color(name_color));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(entry.binding)
                                .small()
                                .color(binding_color),
                        );
                    });
                });
            }
        });
}
