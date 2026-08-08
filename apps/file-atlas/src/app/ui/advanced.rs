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
            let cloud_left = app.cloud_remaining();
            if cloud_left > 0 {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "Downloading cloud files — {cloud_left} remaining"
                        ))
                        .small()
                        .color(palette.sub),
                    );
                    if ui.small_button("Cancel").clicked() {
                        app.cancel_cloud_download();
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
            skip_list_ui(app, ui, palette.sub);
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

/// Folder names no walk enters. Vendored asset libraries are the case that
/// earns this a place in the UI: a Megascans `Downloaded` tree is thousands of
/// per-surface `Thumbs` folders and a plugin source tree, each read costing a
/// second or two on a share, so a scan can spend minutes on scaffolding after
/// the real content has already arrived.
fn skip_list_ui(app: &mut AtlasApp, ui: &mut egui::Ui, sub: egui::Color32) {
    ui.label(
        egui::RichText::new("Folders never scanned")
            .small()
            .strong(),
    );
    ui.label(
        egui::RichText::new(
            "One name per line, matched at any depth and ignoring case. \
             Caches and build scaffolding belong here — anything listed is \
             invisible to scanning, pre-warming, and cover art.",
        )
        .small()
        .color(sub),
    );
    ui.add_space(4.0);
    let edit = egui::TextEdit::multiline(&mut app.skip_edit)
        .desired_rows(6)
        .desired_width(ui.available_width())
        .code_editor();
    ui.add(edit);
    let saved = atlas_core::skiplist::effective();
    let pending: Vec<&str> = app
        .skip_edit
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let dirty = pending.len() != saved.names.len()
        || !pending
            .iter()
            .zip(saved.names.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b));
    ui.horizontal(|ui| {
        if ui
            .add_enabled(dirty, egui::Button::new("Save list"))
            .on_hover_text("Applies to the next scan.")
            .clicked()
        {
            app.apply_skip_list();
            app.toast("Skip list saved — rescan to apply it to this folder");
        }
        if ui
            .add_enabled(app.root.is_some(), egui::Button::new("Save and rescan"))
            .on_hover_text("Re-reads the open folder so the change takes effect now.")
            .clicked()
        {
            app.apply_skip_list();
            app.rescan_root();
        }
        if ui.small_button("Defaults").clicked() {
            app.skip_edit = atlas_core::skiplist::default_names().join("\n");
        }
    });
}
