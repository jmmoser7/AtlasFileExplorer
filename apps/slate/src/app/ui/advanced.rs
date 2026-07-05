//! Floating advanced settings — command reference and workbook diagnostics.

use super::super::{commands, SlateApp};
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
            commands::shortcuts_reference_ui(ui);
        });
    if !open {
        app.tab_mut().chrome.advanced_open = false;
    }
}
