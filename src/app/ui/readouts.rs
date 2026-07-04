//! Bottom readout bar — metrics and future status panels.

use super::super::{AtlasApp, ScanMode};
use super::widgets::{gear_menu, group_digits};
use crate::app::chrome::ReadoutPanel;
use crate::types::human_size;
use eframe::egui::{self, Color32};

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

pub fn status_bar(app: &mut AtlasApp, ctx: &egui::Context) {
    if !app.active_chrome().readout(ReadoutPanel::Metrics) && app.root.is_none() {
        return;
    }
    let palette = app.palette();
    egui::TopBottomPanel::bottom("readouts").show(ctx, |ui| {
        ui.add_space(3.0);
        ui.horizontal(|ui| {
            readouts_gear(app, ui);
            ui.separator();

            if !app.active_chrome().readout(ReadoutPanel::Metrics) {
                return;
            }

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
                            .color(Color32::from_gray(150)),
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
                            .color(Color32::from_gray(130)),
                    );
                }
                if app.warm_pending > 0 {
                    ui.label(
                        egui::RichText::new(format!("· warming cache ({} left)", app.warm_pending))
                            .color(Color32::from_gray(110)),
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
                        egui::RichText::new(format!("· pre-warming ({} left)", prewarm))
                            .color(Color32::from_gray(110)),
                    )
                    .on_hover_text(
                        "Overnight pre-warm: filling the shared project \
                             cache at low priority (2 files at a time).",
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
                        .color(Color32::from_gray(100)),
                    )
                    .on_hover_text(sc.to_string_lossy());
                }
                ui.label(
                    egui::RichText::new(format!("· {:.0}%", app.cam.z * 100.0))
                        .color(Color32::from_gray(110)),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(root) = &app.root {
                    ui.label(
                        egui::RichText::new(root.to_string_lossy())
                            .small()
                            .color(Color32::from_gray(120)),
                    );
                }
            });
        });
        ui.add_space(3.0);
    });
}
