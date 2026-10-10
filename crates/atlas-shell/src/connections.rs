//! Connections table painted once for both apps' Advanced windows.

use eframe::egui::{self, Color32, RichText, Ui};

/// One pack row. Apps fill this from the catalog; the shell only paints.
pub struct ConnectionRow {
    pub id: String,
    pub name: String,
    pub contract: String,
    pub health: String,
    pub install_note: String,
    /// What the probe looked for (P1.portal.health names it on failure).
    pub detail: String,
    pub trial: bool,
}

pub enum ConnectionAction {
    Refresh,
    Forget(String),
}

pub fn section(ui: &mut Ui, rows: &[ConnectionRow], sub: Color32) -> Option<ConnectionAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Connections").strong());
        if ui
            .small_button("Refresh")
            .on_hover_text("Probe every pack again on a background thread.")
            .clicked()
        {
            action = Some(ConnectionAction::Refresh);
        }
    });
    ui.label(
        RichText::new(
            "Every known pack is listed here, ready or not. Choosers list only \
             packs that answered this session, plus OpenAI when it only needs a key.",
        )
        .small()
        .color(sub),
    );
    ui.add_space(4.0);
    egui::Grid::new("pack_connections")
        .striped(true)
        .num_columns(5)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            for title in ["Name", "Contract", "Health", "Install", ""] {
                ui.label(RichText::new(title).small().strong().color(sub));
            }
            ui.end_row();
            for row in rows {
                ui.label(RichText::new(&row.name).small());
                ui.label(RichText::new(&row.contract).small().color(sub));
                let health = ui.label(RichText::new(&row.health).small());
                if !row.detail.is_empty() {
                    health.on_hover_text(&row.detail);
                }
                ui.label(RichText::new(&row.install_note).small().color(sub));
                if row.trial && ui.small_button("Forget").clicked() {
                    action = Some(ConnectionAction::Forget(row.id.clone()));
                } else if !row.trial {
                    ui.label("");
                }
                ui.end_row();
            }
        });
    action
}
