//! Slate keyboard, mouse, and navigation commands.
//!
//! **Rule:** every user-facing shortcut or input command must be registered in
//! [`ENTRIES`] and will appear automatically in Advanced → Commands & shortcuts.
//! See `COMMANDS.md` before adding or changing bindings.

use eframe::egui::Ui;

pub use atlas_shell::commands::{CommandEntry, TurboPanState};

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
        name: "Item context menu (assign tags)",
        binding: "Right-click thumbnail (without dragging)",
    },
    CommandEntry {
        category: "Files",
        name: "Add files to workbook",
        binding: "Drop files onto the window, or Workbook → Add files…",
    },
    CommandEntry {
        category: "Selection",
        name: "Toggle in selection",
        binding: "Ctrl + click thumbnail",
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
        category: "Workbook",
        name: "New workbook tab",
        binding: "Ctrl + T",
    },
    CommandEntry {
        category: "Workbook",
        name: "Open workbook",
        binding: "Ctrl + O",
    },
    CommandEntry {
        category: "Workbook",
        name: "Save workbook",
        binding: "Ctrl + S",
    },
    CommandEntry {
        category: "Workbook",
        name: "Save workbook as…",
        binding: "Ctrl + Shift + S",
    },
    CommandEntry {
        category: "Board",
        name: "Select tool",
        binding: "V (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Frame tool (slides)",
        binding: "F, then click or drag (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Shapes / Curve / Text tools",
        binding: "R / O / L / T (Board view); hover create toolbar for presets",
    },
    CommandEntry {
        category: "Board",
        name: "Text tool",
        binding: "T, then click (Board view); double-click text to edit",
    },
    CommandEntry {
        category: "Board",
        name: "Duplicate",
        binding: "Alt + drag selection, or Ctrl + D",
    },
    CommandEntry {
        category: "Board",
        name: "Delete objects",
        binding: "Delete or Backspace",
    },
    CommandEntry {
        category: "Board",
        name: "Nudge objects",
        binding: "Arrow keys (Shift = ×10)",
    },
    CommandEntry {
        category: "Board",
        name: "Marquee select",
        binding: "Left-drag on empty board",
    },
    CommandEntry {
        category: "Board",
        name: "Pan board",
        binding: "Middle-drag, or Space + left-drag",
    },
    CommandEntry {
        category: "Board",
        name: "Fit board content",
        binding: "Home (Board view)",
    },
    CommandEntry {
        category: "Board",
        name: "Object menu (z-order, tags, delete)",
        binding: "Right-click object",
    },
    CommandEntry {
        category: "Board",
        name: "Undo / Redo board edit",
        binding: "Ctrl + Z / Ctrl + Y (or Ctrl + Shift + Z)",
    },
    CommandEntry {
        category: "Presentation",
        name: "Present frames as slides",
        binding: "F5, or ▶ Present in the board toolbar",
    },
    CommandEntry {
        category: "Presentation",
        name: "Navigate slides",
        binding: "← → / Space / PageUp / PageDown / Home / End; click sides",
    },
    CommandEntry {
        category: "Presentation",
        name: "Exit presentation",
        binding: "Escape",
    },
    CommandEntry {
        category: "Presentation",
        name: "Export HTML artifact",
        binding: "Ctrl + E, or Workbook → Export artifact…",
    },
];

/// Reference table for Advanced settings.
pub fn shortcuts_reference_ui(ui: &mut Ui) {
    atlas_shell::commands::shortcuts_reference_ui(ui, ENTRIES, "apps/slate/src/app/commands.rs");
}
