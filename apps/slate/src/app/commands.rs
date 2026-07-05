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
];

/// Reference table for Advanced settings.
pub fn shortcuts_reference_ui(ui: &mut Ui) {
    atlas_shell::commands::shortcuts_reference_ui(ui, ENTRIES, "apps/slate/src/app/commands.rs");
}
