//! Keyboard, mouse, and navigation commands.
//!
//! **Rule:** every user-facing shortcut or input command must be registered in
//! [`ENTRIES`] and will appear automatically in Advanced → Commands & shortcuts.
//! See `COMMANDS.md` before adding or changing bindings.

use eframe::egui::{self, Pos2, Rect, Ui, Vec2};

/// One row in the Advanced settings command reference.
pub struct CommandEntry {
    pub category: &'static str,
    pub name: &'static str,
    pub binding: &'static str,
}

/// Canonical list of documented commands. Keep sorted by category, then name.
pub const ENTRIES: &[CommandEntry] = &[
    CommandEntry {
        category: "Navigation",
        name: "Pan (precise)",
        binding: "Left-drag on canvas background",
    },
    CommandEntry {
        category: "Navigation",
        name: "Turbo pan",
        binding: "Right-drag on canvas — pull away from the click point; speed scales with \
                  distance; returns to zero at the origin; locked to horizontal or vertical",
    },
    CommandEntry {
        category: "Navigation",
        name: "Zoom",
        binding: "Scroll wheel (pinch on trackpad)",
    },
    CommandEntry {
        category: "Navigation",
        name: "Pan (scroll)",
        binding: "Shift + scroll wheel",
    },
    CommandEntry {
        category: "Navigation",
        name: "Zoom to point",
        binding: "Double-click empty canvas",
    },
    CommandEntry {
        category: "Navigation",
        name: "Fit view",
        binding: "F",
    },
    CommandEntry {
        category: "Navigation",
        name: "Zoom in / out",
        binding: "+ / −",
    },
    CommandEntry {
        category: "Files",
        name: "Open host document",
        binding: "Double-click thumbnail",
    },
    CommandEntry {
        category: "Files",
        name: "File context menu",
        binding: "Right-click file or folder (without dragging)",
    },
    CommandEntry {
        category: "Filters",
        name: "Date timeline fit",
        binding: "Double-click date timeline",
    },
    CommandEntry {
        category: "Filters",
        name: "Date timeline pan",
        binding: "Drag date timeline background",
    },
    CommandEntry {
        category: "Filters",
        name: "Date range scrub",
        binding: "Drag between the two date handles (keeps the window width)",
    },
    CommandEntry {
        category: "Filters",
        name: "Date timeline zoom",
        binding: "Ctrl + scroll wheel over date timeline (zooms around cursor)",
    },
    CommandEntry {
        category: "Filters",
        name: "Filter by date range",
        binding: "Drag either date handle on the timeline",
    },
    CommandEntry {
        category: "Selection",
        name: "Rubber-band select",
        binding: "Shift + left-drag on canvas",
    },
    CommandEntry {
        category: "Selection",
        name: "Toggle in selection",
        binding: "Ctrl + click file",
    },
    CommandEntry {
        category: "Selection",
        name: "Range select",
        binding: "Shift + click file",
    },
    CommandEntry {
        category: "Selection",
        name: "Select all visible",
        binding: "Ctrl + A",
    },
    CommandEntry {
        category: "Selection",
        name: "Clear selection / dismiss",
        binding: "Escape",
    },
    CommandEntry {
        category: "Workflow",
        name: "Open folder",
        binding: "Ctrl + O",
    },
    CommandEntry {
        category: "Workflow",
        name: "Tag / assign selection",
        binding: "F2",
    },
    CommandEntry {
        category: "Workflow",
        name: "Undo / redo",
        binding: "Ctrl + Z / Ctrl + Y (Ctrl + Shift + Z)",
    },
];

/// Speed multiplier for turbo pan: screen-space pull distance → px/frame.
pub const TURBO_PAN_GAIN: f32 = 0.12;
/// Minimum pull before turbo pan engages (distinguishes from a right-click).
pub const TURBO_PAN_ENGAGE_PX: f32 = 4.0;
/// Minimum movement before the pan axis locks to horizontal or vertical.
pub const TURBO_PAN_AXIS_LOCK_PX: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PanAxis {
    Horizontal,
    Vertical,
}

/// Right-drag turbo pan state (per canvas interaction).
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

    /// Apply turbo pan while the secondary button is held. Returns `true` when
    /// panning is active this frame (left-drag pan should be skipped).
    pub fn step(
        &mut self,
        ctx: &egui::Context,
        canvas_rect: Rect,
        pointer: Option<Pos2>,
        cam_offset: &mut Vec2,
    ) -> bool {
        let (secondary_down, secondary_released) = ctx.input(|i| {
            (
                i.pointer.button_down(egui::PointerButton::Secondary),
                i.pointer.button_released(egui::PointerButton::Secondary),
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

        let Some(p) = pointer else {
            return self.engaged;
        };
        if !canvas_rect.contains(p) {
            return self.engaged;
        }

        if secondary_down && self.anchor.is_none() {
            self.anchor = Some(p);
            self.axis = None;
            self.engaged = false;
        }

        let Some(anchor) = self.anchor else {
            return false;
        };

        if !secondary_down {
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

/// Reference table for Advanced settings.
pub fn shortcuts_reference_ui(ui: &mut Ui) {
    ui.label(egui::RichText::new("Commands & shortcuts").small().strong());
    ui.label(
        egui::RichText::new(
            "All bindings below are defined in src/app/commands.rs — add new \
             commands there so this list stays complete.",
        )
        .small()
        .color(egui::Color32::from_gray(120)),
    );
    ui.add_space(6.0);

    let mut last_category = "";
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .id_salt("commands_reference")
        .show(ui, |ui| {
            for entry in ENTRIES {
                if entry.category != last_category {
                    if !last_category.is_empty() {
                        ui.add_space(6.0);
                    }
                    ui.label(egui::RichText::new(entry.category).small().strong());
                    last_category = entry.category;
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(entry.name)
                            .small()
                            .color(egui::Color32::from_gray(200)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(entry.binding)
                                .small()
                                .color(egui::Color32::from_gray(130)),
                        );
                    });
                });
            }
        });
}
