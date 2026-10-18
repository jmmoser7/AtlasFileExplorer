//! Shared chrome and folder reveal for local crash diagnostics.

use crate::theme::Palette;
use eframe::egui::{self, RichText};
use std::{io, process::Command};

/// Returns the user's request to open the local reports folder.
pub fn section(ui: &mut egui::Ui, palette: &Palette) -> bool {
    ui.label(RichText::new("Crash reports").strong());
    ui.label(
        RichText::new(
            "Local reports record unexpected exits and Rust panics, with recent \
             session activity to help explain what happened. Some failures may \
             leave only partial information. Nothing is uploaded automatically.",
        )
        .small()
        .color(palette.sub),
    );
    ui.small_button("Open crash reports").clicked()
}

/// Creates and reveals the report folder. Call from a worker, never a paint path.
pub fn reveal(app: &str) -> io::Result<()> {
    let path = atlas_core::crash_report::reports_dir(app);
    std::fs::create_dir_all(&path)?;
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    command.arg(path).spawn()?;
    Ok(())
}
