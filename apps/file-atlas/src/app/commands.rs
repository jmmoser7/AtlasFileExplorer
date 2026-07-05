//! Keyboard, mouse, and navigation commands.
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

/// Reference table for Advanced settings.
pub fn shortcuts_reference_ui(ui: &mut Ui) {
    atlas_shell::commands::shortcuts_reference_ui(
        ui,
        ENTRIES,
        "apps/file-atlas/src/app/commands.rs",
    );
}
