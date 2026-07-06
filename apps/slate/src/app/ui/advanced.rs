//! Floating advanced settings — canvas preview tuning, command reference,
//! and workbook diagnostics.

use super::super::{commands, settings, SlateApp};
use atlas_core::preview::{MAX_PX_MAX, MAX_PX_MIN};
use eframe::egui::{self, Color32};

pub fn window(app: &mut SlateApp, ctx: &egui::Context) {
    if !app.tab().chrome.advanced_open {
        return;
    }
    let mut open = true;
    egui::Window::new("Advanced")
        .open(&mut open)
        .default_width(340.0)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "Slate workbooks store links to files — never copies — plus \
                     the tag structure and view state. Thumbnails are shared \
                     with File Atlas's cache, so anything Atlas has seen loads \
                     instantly here.",
                )
                .small()
                .color(Color32::from_gray(120)),
            );
            if let Some(path) = &app.tab().path {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Workbook file").small().strong());
                ui.label(
                    egui::RichText::new(path.display().to_string())
                        .small()
                        .color(Color32::from_gray(110)),
                );
            }
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            preview_section(app, ui);
            ui.add_space(12.0);
            ui.separator();
            ui.add_space(6.0);
            commands::shortcuts_reference_ui(ui);
        });
    if !open {
        app.tab_mut().chrome.advanced_open = false;
    }
}

/// Canvas previews: the lazy full-resolution tier above the thumbnails.
/// Changes persist to `slate-settings.json` immediately.
fn preview_section(app: &mut SlateApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Canvas previews").strong());
    ui.label(
        egui::RichText::new(
            "Every view paints instantly from cached thumbnails, then \
             sharpens what you zoom into with full-resolution decodes — \
             images, PDF pages, and anything else with a rich preview. \
             Higher resolution looks better up close; the memory budget \
             caps how many sharp previews stay loaded at once (least \
             recently viewed fall back to thumbnails first).",
        )
        .small()
        .color(Color32::from_gray(120)),
    );
    ui.add_space(4.0);

    let mut changed = false;
    let s = &mut app.settings.preview;
    changed |= ui
        .checkbox(&mut s.enabled, "Load full-resolution previews")
        .on_hover_text("Off = thumbnails only (lowest memory, softest zoom)")
        .changed();
    ui.add_enabled_ui(s.enabled, |ui| {
        changed |= ui
            .add(
                egui::Slider::new(&mut s.max_px, MAX_PX_MIN..=MAX_PX_MAX)
                    .logarithmic(true)
                    .suffix(" px")
                    .text("Max resolution"),
            )
            .on_hover_text(
                "Longest-edge cap for full-resolution decodes. 2048 px is \
                 sharp on most displays; raise it for large monitors or deep \
                 zooms, at the cost of memory per image.",
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(
                    &mut s.budget_mb,
                    settings::BUDGET_MB_MIN..=settings::BUDGET_MB_MAX,
                )
                .logarithmic(true)
                .suffix(" MB")
                .text("Memory budget"),
            )
            .on_hover_text(
                "How much RAM decoded previews may hold. When exceeded, the \
                 least recently viewed previews unload back to thumbnails.",
            )
            .changed();
    });
    if changed {
        app.settings.save();
    }

    let (entries, bytes) = app.preview_cache_stats();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{entries} preview(s) loaded · {:.0} MB",
                bytes as f64 / (1024.0 * 1024.0)
            ))
            .small()
            .color(Color32::from_gray(140)),
        );
        if entries > 0 && ui.small_button("Unload all").clicked() {
            app.clear_preview_cache();
        }
    });
}
