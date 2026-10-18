//! Command reference adapts the canonical registry for the shared shell.
use super::{CommandEntry, SPECS};
use atlas_commands::Registry;
use eframe::egui::Ui;

/// The app's registry view over [`SPECS`].
pub fn registry() -> Registry {
    Registry::new(SPECS)
}

/// Reference table for Advanced settings, rendered from [`SPECS`] so the
/// window and the dispatcher can never disagree.
pub fn shortcuts_reference_ui(ui: &mut Ui) {
    let entries: Vec<CommandEntry> = SPECS
        .iter()
        .map(|s| CommandEntry {
            category: s.category,
            name: s.name,
            binding: s.binding,
        })
        .collect();
    atlas_shell::commands::shortcuts_reference_ui(ui, &entries, "apps/slate/src/app/commands.rs");
}
