//! Standard application windows; all actions use the command registry.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use atlas_commands::CommandId;
use eframe::egui;

use super::{entry_id, SlateApp};

pub(in crate::app) fn recovery_settings_ui(app: &mut SlateApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Autosave & recovery").strong());
    ui.label(
        egui::RichText::new(
            "Keep local recovery copies of every changed workbook, including workbooks \
         that have never been saved. Use Save to update the workbook file.",
        )
        .small()
        .color(app.palette().sub),
    );
    let mut prefs = app.settings.recovery.clone().clamped();
    let mut changed = ui
        .checkbox(&mut prefs.enabled, "Save recovery copies automatically")
        .changed();
    ui.add_enabled_ui(prefs.enabled, |ui| {
        changed |= ui
            .add(
                egui::Slider::new(&mut prefs.interval_seconds, 5..=300)
                    .text("Seconds between copies"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut prefs.retained_versions, 1..=10)
                    .text("Versions per workbook"),
            )
            .changed();
    });
    ui.small("First capture starts within 1 second of unsaved changes. Turning this off keeps existing copies.");
    if let Some(error) = &app.recovery.status {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("Recovery needs attention: {error}"),
        );
        ui.small("Existing copies are preserved. Slate retries the worker automatically.");
    } else if app.recovery.busy > 0 || app.recovery.scan_pending {
        ui.small("Recovery is working in the background…");
    } else if let Some(at) = app
        .recovery
        .schedule
        .key(app.tab().id)
        .and_then(|key| app.recovery.last_saved.get(&key))
    {
        ui.small(format!(
            "This workbook's recovery checked {} seconds ago",
            at.elapsed().as_secs()
        ));
    } else {
        ui.small("Recovery is ready; waiting for unsaved changes.");
    }
    if let Some(path) = app.recovery.folder.path.get() {
        ui.small("Recovery folder — select the path to copy:");
        ui.add(
            egui::Label::new(egui::RichText::new(path.display().to_string()).small())
                .selectable(true)
                .wrap(),
        );
    }
    let mut command = None;
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("Recover workbooks…").clicked() {
            command = Some("app.recovery.open");
        }
        if ui.small_button("Open recovery folder").clicked() {
            command = Some("app.recovery.folder");
        }
    });
    if changed {
        if let Ok(detail) = serde_json::to_string(&prefs) {
            app.dispatch(ui.ctx(), CommandId("app.recovery.configure"), Some(detail));
        }
    }
    if let Some(command) = command {
        app.dispatch(ui.ctx(), CommandId(command), None);
    }
}

pub(super) fn window(app: &mut SlateApp, ctx: &egui::Context) {
    if !app.recovery.window_open {
        return;
    }
    let mut open = true;
    let mut action = None;
    let busy = app.recovery.busy != 0 || app.recovery.scan_pending;
    egui::Window::new("Recover workbooks")
        .open(&mut open).default_width(540.0).default_height(330.0)
        .show(ctx, |ui| {
            ui.label("Recovery opens an unsaved copy. Choose Save as to save it where you want.");
            ui.small("Copies below belong to inactive Slate sessions. The source workbook may contain newer work.");
            if busy { ui.label("Working in the background…"); }
            if let Some(error) = &app.recovery.status {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                if app.recovery.entries.is_empty() && !busy {
                    ui.label("No recovery copies from previous sessions were found.");
                }
                for entry in &app.recovery.entries {
                    ui.separator();
                    ui.label(egui::RichText::new(&entry.name).strong());
                    ui.small(format!("Captured {}", capture_age(entry.saved_at_ms)));
                    match &entry.original_path {
                        Some(path) => { ui.small(format!("Original: {}", path.display())); }
                        None => { ui.small("Never saved to a workbook file"); }
                    }
                    if let Some(warning) = &entry.warning {
                        ui.colored_label(ui.visuals().warn_fg_color, warning);
                    }
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Recover as new workbook").clicked() {
                                action = Some(("app.recovery.restore", entry_id(entry)));
                            }
                            if ui.small_button("Discard…").clicked() {
                                action = Some(("app.recovery.discard", entry_id(entry)));
                            }
                        });
                    });
                }
                if !app.recovery.issues.is_empty() {
                    ui.separator();
                    ui.label("Some copies could not be read:");
                    for issue in &app.recovery.issues { ui.small(issue); }
                }
            });
            ui.separator();
            if ui.button("Keep for later").clicked() { app.recovery.window_open = false; }
        });
    app.recovery.window_open &= open;
    if let Some((command, detail)) = action {
        app.dispatch(ctx, CommandId(command), Some(detail));
    }
    discard_confirmation(app, ctx);
}

fn discard_confirmation(app: &mut SlateApp, ctx: &egui::Context) {
    let Some(id) = app.recovery.confirm_discard.clone() else {
        return;
    };
    let mut open = true;
    let mut confirmed = false;
    let mut cancelled = false;
    egui::Window::new("Discard recovery copies?")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("This permanently removes the recovery versions of this workbook.");
            ui.label("Recover the workbook first if you want to keep its contents.");
            ui.horizontal(|ui| {
                if ui.button("Keep copies").clicked() {
                    cancelled = true;
                }
                if ui
                    .add_enabled(
                        app.recovery.busy == 0 && !app.recovery.scan_pending,
                        egui::Button::new("Discard copies"),
                    )
                    .clicked()
                {
                    confirmed = true;
                }
            });
        });
    if confirmed {
        app.dispatch(
            ctx,
            CommandId("app.recovery.discard"),
            Some(format!("confirm:{id}")),
        );
    } else if cancelled || !open {
        app.recovery.confirm_discard = None;
    }
}

fn capture_age(saved_ms: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis();
    let seconds = now.saturating_sub(saved_ms as u128) / 1000;
    if seconds < 60 {
        format!("{seconds} seconds ago")
    } else if seconds < 3600 {
        format!("{} minutes ago", seconds / 60)
    } else if seconds < 86400 {
        format!("{} hours ago", seconds / 3600)
    } else {
        format!("{} days ago", seconds / 86400)
    }
}
