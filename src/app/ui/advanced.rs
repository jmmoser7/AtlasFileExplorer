//! Floating advanced settings (pre-warm, shared cache). Opened from the tools
//! gear menu — not a permanent rail panel.

use super::super::AtlasApp;
use eframe::egui::{self, Color32};

pub fn window(app: &mut AtlasApp, ctx: &egui::Context) {
    if !app.active_chrome().advanced_open {
        return;
    }
    let mut open = true;
    egui::Window::new("Advanced")
        .open(&mut open)
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Pre-warm builds thumbnails for a whole folder at the \
                     lowest priority (2 files at a time) — ideal overnight. \
                     Results publish into the shared project cache automatically \
                     as files are viewed or warmed.",
                )
                .small()
                .color(Color32::from_gray(120)),
            );
            ui.add_space(6.0);
            if ui.button("Pre-warm a folder…").clicked() {
                app.open_prewarm_dialog();
            }
            if app.prewarm_picker_rx.is_some() {
                ui.spinner();
            }
            let remaining = app.prewarm_remaining();
            if remaining > 0 {
                ui.label(
                    egui::RichText::new(format!("{remaining} files remaining"))
                        .small()
                        .color(Color32::from_gray(140)),
                );
            }
            if let Some(sc) = &app.shared_cache {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Shared project cache").small().strong());
                ui.label(
                    egui::RichText::new(sc.display().to_string())
                        .small()
                        .color(Color32::from_gray(110)),
                );
                if ui.small_button("Sync local thumbnails now").clicked() {
                    app.sync_shared_cache_from_local();
                    app.toast("Syncing local thumbnails to shared cache");
                }
            } else if app.root.is_some() {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "No project template anchor found — shared cache requires \
                         …\\02 DESIGN\\05 RESOURCES\\03 DATA in the project tree.",
                    )
                    .small()
                    .color(Color32::from_gray(130)),
                );
            }
        });
    if !open {
        app.active_chrome_mut().advanced_open = false;
    }
}
