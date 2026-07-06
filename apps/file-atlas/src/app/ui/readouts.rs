//! Bottom readout bar — metrics and future status panels.

use super::super::{AtlasApp, DateFilterField, ScanMode};
use super::activity_heatmap::{draw_activity_heatmap, ActivityHeatmap};
use crate::app::chrome::ReadoutPanel;
use atlas_core::types::human_size;
use atlas_shell::widgets::{gear_menu, group_digits};
use eframe::egui;

fn readouts_gear(app: &mut AtlasApp, ui: &mut egui::Ui) {
    gear_menu(ui, "readouts_gear", |ui| {
        ui.label(egui::RichText::new("Visible readouts").small().strong());
        ui.separator();
        for panel in ReadoutPanel::ALL {
            let mut on = app.active_chrome().readout(panel);
            if ui.checkbox(&mut on, panel.label()).changed() {
                app.active_chrome_mut().set_readout(panel, on);
            }
        }
    });
}

fn date_field_label(field: DateFilterField) -> &'static str {
    match field {
        DateFilterField::Created => "created",
        DateFilterField::Modified => "modified",
    }
}

fn activity_source_label(app: &AtlasApp) -> &'static str {
    if !app.selection.is_empty() {
        "selection"
    } else if app.any_filter {
        "filtered canvas"
    } else {
        "canvas"
    }
}

fn metrics_row(app: &mut AtlasApp, ui: &mut egui::Ui) {
    let palette = app.palette();

    if let Some(scan) = &app.scan_ui {
        let files = app
            .scan_handle
            .as_ref()
            .map(|h| h.files_found.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0);
        ui.spinner();
        match scan.mode {
            ScanMode::Fresh => {
                ui.label(format!(
                    "Scanning… {} files found ({:.1}s)",
                    group_digits(files),
                    scan.started.elapsed().as_secs_f32()
                ));
            }
            ScanMode::Refresh => {
                ui.label(
                    egui::RichText::new(format!(
                        "Showing saved index · re-verifying… {} files",
                        group_digits(files)
                    ))
                    .color(palette.sub),
                );
            }
        }
    } else if app.pending_load.is_some() {
        ui.spinner();
        ui.label("Opening index…");
    } else if app.root.is_some() {
        let dirs = app.tree.as_ref().map(|t| t.total_dirs).unwrap_or(0);
        ui.label(format!(
            "{} files · {} folders · {}",
            group_digits(app.alive_count as u64),
            group_digits(dirs as u64),
            human_size(app.total_bytes)
        ));
        if app.any_filter {
            ui.label(
                egui::RichText::new(format!(
                    "· {} match ({})",
                    group_digits(app.shown_count as u64),
                    human_size(app.shown_bytes)
                ))
                .color(palette.select),
            );
        }
        if !app.selection.is_empty() {
            ui.label(
                egui::RichText::new(format!("· {} selected", app.selection.len()))
                    .color(palette.staged),
            );
        }
        if app.thumbs_pending > 0 {
            ui.label(
                egui::RichText::new(format!("· {} thumbs loading", app.thumbs_pending))
                    .color(palette.sub),
            );
        }
        if app.warm_pending > 0 {
            ui.label(
                egui::RichText::new(format!("· warming cache ({} left)", app.warm_pending))
                    .color(palette.sub),
            )
            .on_hover_text(
                "Pre-generating thumbnails in the background so \
                 cold folders open instantly. Throttled to stay \
                 polite to the network.",
            );
        }
        let prewarm = app.prewarm_remaining();
        if prewarm > 0 {
            ui.label(
                egui::RichText::new(format!("· pre-warming ({} left)", prewarm)).color(palette.sub),
            )
            .on_hover_text(
                "Overnight pre-warm: filling the shared project \
                 cache at low priority. Full progress and controls \
                 are in the dashboard above.",
            );
        }
        if let Some(sc) = &app.shared_cache {
            ui.label(
                egui::RichText::new(format!(
                    "· shared cache {}",
                    sc.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "active".into())
                ))
                .small()
                .color(palette.sub),
            )
            .on_hover_text(sc.to_string_lossy());
        }
        ui.label(egui::RichText::new(format!("· {:.0}%", app.cam.z * 100.0)).color(palette.sub));
    }
}

/// Temporary dashboard shown only while an explicit pre-warm run is active
/// (Advanced settings → "Pre-warm a folder…"). Stacks above the persistent
/// readout bar and disappears when the run completes or is cancelled.
/// Live readouts: file discovery, thumbnail progress, transfer speed; user
/// controls: parallel-job speed adjustment and cancel.
pub fn prewarm_dashboard(app: &mut AtlasApp, ctx: &egui::Context) {
    use std::sync::atomic::Ordering::Relaxed;

    let Some(job) = &app.prewarm else {
        return;
    };
    // Snapshot everything up front so the panel closure doesn't hold a
    // borrow of `app` (the controls below need `&mut app` afterwards).
    let dir_full = job.dir.display().to_string();
    let dir_name = job
        .dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir_full.clone());
    let discovering = !job.walk_done_now();
    let queued = job.queued_now();
    let done = job.done;
    let remaining = job.remaining();
    let bytes_queued = job.bytes_queued.load(Relaxed);
    let bytes_done = job.bytes_done;
    let repos = job.repos.load(Relaxed);
    let (files_per_s, bytes_per_s) = job.speed();
    let elapsed = job.started.elapsed().as_secs();
    let limit = app.thumbs.slow_limit();
    let palette = app.palette();

    let mut cancel = false;
    let mut new_limit: Option<usize> = None;

    egui::TopBottomPanel::bottom("prewarm_dashboard").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Pre-warm").strong());
            ui.label(egui::RichText::new(&dir_name).small().color(palette.sub))
                .on_hover_text(&dir_full);
            ui.separator();

            // Phase 1 readout: file discovery (background walk).
            if discovering {
                ui.spinner();
                ui.label(format!(
                    "Discovering… {} files ({})",
                    group_digits(queued as u64),
                    human_size(bytes_queued)
                ));
            } else {
                ui.label(format!(
                    "{} files ({})",
                    group_digits(queued as u64),
                    human_size(bytes_queued)
                ));
            }
            ui.separator();

            // Phase 2 readout: thumbnail creation progress.
            let frac = if queued > 0 {
                (done as f32 / queued as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            ui.add_sized(
                [160.0, 14.0],
                egui::ProgressBar::new(frac).text(
                    egui::RichText::new(format!(
                        "{} / {}{}",
                        group_digits(done as u64),
                        group_digits(queued as u64),
                        if discovering { "+" } else { "" }
                    ))
                    .small(),
                ),
            )
            .on_hover_text(format!(
                "Thumbnails built ({} of {} read)",
                human_size(bytes_done),
                human_size(bytes_queued)
            ));

            // Transfer speed + ETA over the last few seconds of completions.
            if files_per_s > 0.0 {
                ui.label(
                    egui::RichText::new(format!(
                        "{:.1} files/s · {}/s",
                        files_per_s,
                        human_size(bytes_per_s as u64)
                    ))
                    .color(palette.sub),
                );
                if !discovering && files_per_s > 0.01 {
                    let eta = (remaining as f32 / files_per_s) as u64;
                    ui.label(
                        egui::RichText::new(format!("~{} left", fmt_secs(eta)))
                            .small()
                            .color(palette.sub),
                    );
                }
            }

            if repos > 0 {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!(
                        "{} shared cache repositor{}",
                        repos,
                        if repos == 1 { "y" } else { "ies" }
                    ))
                    .small()
                    .color(palette.sub),
                )
                .on_hover_text(
                    "Projects found under the pre-warmed folder get a shared \
                     .atlas-cache repository so thumbnails serve everyone.",
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button("Cancel")
                    .on_hover_text("Stop this pre-warm — thumbnails already built are kept")
                    .clicked()
                {
                    cancel = true;
                }
                ui.separator();
                // Speed control: how many thumbnails build in parallel.
                if ui
                    .add_enabled(
                        limit < atlas_core::thumbs::SLOW_CONCURRENCY_MAX,
                        egui::Button::new("+").small(),
                    )
                    .on_hover_text("Faster (more parallel jobs, more network load)")
                    .clicked()
                {
                    new_limit = Some(limit + 1);
                }
                ui.label(egui::RichText::new(format!("{limit}")).strong());
                if ui
                    .add_enabled(
                        limit > atlas_core::thumbs::SLOW_CONCURRENCY_MIN,
                        egui::Button::new("−").small(),
                    )
                    .on_hover_text("Gentler (fewer parallel jobs)")
                    .clicked()
                {
                    new_limit = Some(limit - 1);
                }
                ui.label(egui::RichText::new("Speed").small().color(palette.sub))
                    .on_hover_text(
                        "Parallel thumbnail jobs for this pre-warm. Lower is \
                     gentler on the network; on-demand views always win.",
                    );
                ui.label(
                    egui::RichText::new(format!("· {} elapsed", fmt_secs(elapsed)))
                        .small()
                        .color(palette.sub),
                );
            });
        });
        ui.add_space(4.0);
    });

    if let Some(n) = new_limit {
        app.thumbs.set_slow_limit(n);
    }
    if cancel {
        app.cancel_prewarm();
    }
}

fn fmt_secs(s: u64) -> String {
    if s >= 3600 {
        format!("{}h {:02}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m {:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

pub fn status_bar(app: &mut AtlasApp, ctx: &egui::Context) {
    let show_metrics = app.active_chrome().readout(ReadoutPanel::Metrics);
    let show_heatmap = app.active_chrome().readout(ReadoutPanel::ActivityHeatmap);
    if !show_metrics && !show_heatmap && app.root.is_none() {
        return;
    }

    egui::TopBottomPanel::bottom("readouts").show(ctx, |ui| {
        let palette = app.palette();
        ui.add_space(3.0);
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.horizontal(|ui| {
                readouts_gear(app, ui);
                ui.separator();

                if show_metrics {
                    metrics_row(app, ui);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(root) = &app.root {
                        ui.label(
                            egui::RichText::new(root.to_string_lossy())
                                .small()
                                .color(palette.sub),
                        );
                    }
                });
            });

            if show_heatmap && app.root.is_some() && app.scan_ui.is_none() {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(2.0);
                let heatmap = ActivityHeatmap::from_timestamps(app.activity_timestamps());
                draw_activity_heatmap(
                    ui,
                    &heatmap,
                    date_field_label(app.date_field),
                    activity_source_label(app),
                    app.dark_mode,
                    palette.sub,
                );
            }
        });
        ui.add_space(3.0);
    });
}
