//! Connections table painted once for both apps' Advanced windows.

use eframe::egui::{self, Color32, RichText, Ui};

/// One pack row. Apps fill this from the catalog; the shell only paints.
pub struct ConnectionRow {
    pub id: String,
    pub name: String,
    pub contract: String,
    pub health: String,
    pub install_note: String,
    pub trial: bool,
}

pub enum ConnectionAction {
    Refresh,
    Forget(String),
}

pub fn section(ui: &mut Ui, rows: &[ConnectionRow], sub: Color32) -> Option<ConnectionAction> {
    ui.label(RichText::new("Connections").strong());
    ui.label(
        RichText::new(
            "A pack appears here even when it is not ready. The chooser only \
             lists packs that answered this session, plus a credential that \
             only needs a key.",
        )
        .small()
        .color(sub),
    );
    let mut action = None;
    if ui.small_button("Refresh").clicked() {
        action = Some(ConnectionAction::Refresh);
    }
    ui.add_space(4.0);
    egui::Grid::new("pack_connections")
        .striped(true)
        .num_columns(5)
        .show(ui, |ui| {
            for title in ["Name", "Contract", "Health", "Install", ""] {
                ui.label(RichText::new(title).small().strong().color(sub));
            }
            ui.end_row();
            for row in rows {
                ui.label(RichText::new(&row.name).small());
                ui.label(RichText::new(&row.contract).small().color(sub));
                ui.label(RichText::new(&row.health).small())
                    .on_hover_text(&row.install_note);
                ui.label(RichText::new(&row.install_note).small().color(sub));
                if row.trial && ui.small_button("Forget").clicked() {
                    action = Some(ConnectionAction::Forget(row.id.clone()));
                }
                ui.end_row();
            }
        });
    action
}
