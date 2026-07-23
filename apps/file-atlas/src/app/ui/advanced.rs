//! Floating advanced settings (pre-warm, shared cache). Opened from the tools
//! gear menu — not a permanent rail panel.

use super::super::commands;
use super::super::{AtlasApp, PrewarmPortalMode};
use eframe::egui;

pub fn window(app: &mut AtlasApp, ctx: &egui::Context) {
    if !app.active_chrome().advanced_open {
        return;
    }
    let palette = app.palette();
    let mut open = true;
    egui::Window::new("Advanced")
        .open(&mut open)
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Pre-warm builds thumbnails for a whole folder at the \
                     lowest priority — ideal overnight. Every project found \
                     under the folder gets a shared .atlas-cache repository \
                     (created if missing), so results serve everyone. \
                     Progress, speed control and cancel live in the dashboard \
                     at the bottom of the window while a run is active.",
                )
                .small()
                .color(palette.sub),
            );
            ui.add_space(6.0);
            let running = app.prewarm.is_some();
            ui.add_enabled_ui(!running, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Large folders (>{} items) — often video frame dumps \
                         with near-identical thumbnails:",
                        app.portal_threshold
                    ))
                    .small(),
                );
                ui.horizontal(|ui| {
                    ui.radio_value(
                        &mut app.prewarm_portal_mode,
                        PrewarmPortalMode::Normal,
                        "Warm normally",
                    );
                    ui.radio_value(
                        &mut app.prewarm_portal_mode,
                        PrewarmPortalMode::Defer,
                        "Warm last",
                    )
                    .on_hover_text("Queued behind everything else in the run.");
                    ui.radio_value(
                        &mut app.prewarm_portal_mode,
                        PrewarmPortalMode::Skip,
                        "Skip",
                    )
                    .on_hover_text(
                        "Their files are not warmed at all; subfolders are still walked.",
                    );
                });
            });
            if ui
                .add_enabled(!running, egui::Button::new("Pre-warm a folder…"))
                .clicked()
            {
                app.open_prewarm_dialog();
            }
            if app.prewarm_picker_rx.is_some() {
                ui.spinner();
            }
            let job_info = app
                .prewarm
                .as_ref()
                .map(|j| (j.dir.display().to_string(), j.remaining()));
            if let Some((dir, remaining)) = job_info {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{remaining} files remaining"))
                            .small()
                            .color(palette.sub),
                    )
                    .on_hover_text(dir);
                    if ui.small_button("Cancel").clicked() {
                        app.cancel_prewarm();
                    }
                });
            }
            if let Some(sc) = &app.shared_cache {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Shared project cache").small().strong());
                ui.label(
                    egui::RichText::new(sc.display().to_string())
                        .small()
                        .color(palette.sub),
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
                    .color(palette.sub),
                );
            }
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            // Command history (Atlas keeps F2 = Assign; history lives here).
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Command history").small().strong());
                ui.label(
                    egui::RichText::new(format!("{} recorded", app.cmd_history.len()))
                        .small()
                        .color(palette.sub),
                );
                if ui
                    .small_button(if app.history_open { "Hide" } else { "Show" })
                    .clicked()
                {
                    app.history_open = !app.history_open;
                }
            });
            ui.add_space(6.0);
            commands::shortcuts_reference_ui(ui);
        });
    if !open {
        app.active_chrome_mut().advanced_open = false;
    }
}
