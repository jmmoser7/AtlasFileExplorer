//! Bottom readout bar — workbook metrics and link health.

use super::super::chrome::ReadoutPanel;
use super::super::SlateApp;
use atlas_shell::widgets::{gear_menu, group_digits};
use eframe::egui::{self, RichText};
use slate_doc::{link_status, LinkStatus};

fn readouts_gear(app: &mut SlateApp, ui: &mut egui::Ui) {
    gear_menu(ui, "slate_readouts_gear", |ui| {
        ui.label(RichText::new("Visible readouts").small().strong());
        ui.separator();
        for panel in ReadoutPanel::ALL {
            let mut on = app.tab().chrome.readout(panel);
            if ui.checkbox(&mut on, panel.label()).changed() {
                app.tab_mut().chrome.set_readout(panel, on);
            }
        }
    });
}

pub fn status_bar(app: &mut SlateApp, ctx: &egui::Context) {
    let palette = app.palette();
    egui::TopBottomPanel::bottom("slate_readouts").show(ctx, |ui| {
        ui.horizontal(|ui| {
            readouts_gear(app, ui);

            if app.tab().chrome.readout(ReadoutPanel::Metrics) {
                let doc = app.doc();
                let total = doc.items.len();
                let uncategorized = doc.uncategorized_items().len();
                ui.label(format!(
                    "{} linked file(s) · {} tagged · {} uncategorized",
                    group_digits(total as u64),
                    group_digits((total - uncategorized) as u64),
                    group_digits(uncategorized as u64),
                ));
                if !app.selection.is_empty() {
                    ui.label(
                        RichText::new(format!("· {} selected", app.selection.len()))
                            .color(palette.staged),
                    );
                }
            }

            if app.tab().chrome.readout(ReadoutPanel::LinkHealth) {
                let missing = app
                    .doc()
                    .items
                    .iter()
                    .filter(|it| link_status(it) == LinkStatus::Missing)
                    .count();
                if missing > 0 {
                    ui.label(
                        RichText::new(format!("· {missing} missing link(s)"))
                            .color(egui::Color32::from_rgb(0xe0, 0x6c, 0x5c)),
                    )
                    .on_hover_text(
                        "Some linked files no longer exist at their saved path. \
                         Right-click an item to relink it.",
                    );
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{:.0}%", app.tab().cam.z * 100.0))
                        .color(egui::Color32::from_gray(110)),
                );
                if app.atlas.is_some() {
                    ui.label(
                        RichText::new("Atlas linked ·")
                            .small()
                            .color(palette.accent),
                    );
                }
            });
        });
    });
}
