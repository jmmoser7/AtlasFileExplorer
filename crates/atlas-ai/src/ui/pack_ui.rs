//! Pack surfaces both apps call: the Connections section of Advanced and the
//! OpenAI key window. The shell paints the table; this module wires it to the
//! session catalog.

use eframe::egui::{self, Color32, RichText};

use super::AiPanel;
use crate::packs::KEY_URL;
use atlas_shell::connections::{self, ConnectionAction, ConnectionRow};

/// The Connections section. Returns a message for the app's toast when an
/// action failed.
pub fn connections_section(panel: &mut AiPanel, ui: &mut egui::Ui, sub: Color32) -> Option<String> {
    let rows: Vec<ConnectionRow> = panel
        .packs
        .rows()
        .into_iter()
        .map(|row| ConnectionRow {
            id: row.id,
            name: row.name,
            contract: row.contract,
            ok: row.health == crate::packs::PackHealth::Ok.as_str(),
            health: row.health,
            install_note: row.install_note,
            detail: row.detail,
            trial: row.trial,
        })
        .collect();
    let workspace = panel.config.workspace_dir.clone();
    match connections::section(ui, &rows, sub)? {
        ConnectionAction::Refresh => {
            panel.packs.refresh(workspace);
            None
        }
        ConnectionAction::Forget(id) => panel.packs.forget(&id, workspace).err(),
    }
}

/// Key entry for the OpenAI pack. `Some(Ok)` when a key was stored for this
/// operating-system user; `Some(Err)` names what went wrong.
pub fn openai_key_window(panel: &mut AiPanel, ctx: &egui::Context) -> Option<Result<(), String>> {
    if !panel.packs.key_entry {
        return None;
    }
    let mut open = true;
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("OpenAI API key")
        .id(egui::Id::new("atlas-ai-openai-key"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            ui.label(
                "Generate with OpenAI image and text models. The key stays on this \
                 machine for this user and is never written into a workbook.",
            );
            ui.hyperlink_to("Get an API key at platform.openai.com", KEY_URL);
            ui.add_space(6.0);
            let field = ui.add(
                egui::TextEdit::singleline(&mut panel.packs.key_draft)
                    .password(true)
                    .hint_text("Paste the API key")
                    .desired_width(f32::INFINITY),
            );
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                save = ui.button(RichText::new("Save key").strong()).clicked() || enter;
                cancel = ui.button("Cancel").clicked();
            });
        });
    if !open || cancel {
        panel.packs.key_entry = false;
        panel.packs.key_draft.clear();
        return None;
    }
    save.then(|| {
        let workspace = panel.config.workspace_dir.clone();
        panel.packs.save_openai_key(workspace)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packs::PackHealth;
    use atlas_shell::sidebar::SidebarTheme;

    fn texts(panel: &mut AiPanel, body: impl Fn(&mut AiPanel, &mut egui::Ui)) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        let mut out = Vec::new();
        for _ in 0..2 {
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| body(panel, ui));
            });
            out.clear();
            output.shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        }
        out
    }

    fn theme() -> SidebarTheme {
        SidebarTheme {
            card: Color32::DARK_GRAY,
            border: Color32::GRAY,
            ink: Color32::WHITE,
            sub: Color32::LIGHT_GRAY,
        }
    }

    #[test]
    fn launch_cursor_shows_only_while_the_cursor_pack_is_ok() {
        let mut panel = AiPanel::new(&atlas_shell::file_picker::DialogGate::new());
        panel.packs.record("cursor", PackHealth::Missing);
        let shown = texts(&mut panel, |p, ui| super::super::ai_body(p, ui, theme()));
        assert!(!shown.iter().any(|t| t == "Launch Cursor"), "{shown:?}");
        assert!(shown.iter().any(|t| t == "AI workspace"), "{shown:?}");
        panel.packs.record("cursor", PackHealth::Ok);
        let shown = texts(&mut panel, |p, ui| super::super::ai_body(p, ui, theme()));
        assert!(shown.iter().any(|t| t == "Launch Cursor"), "{shown:?}");
    }

    #[test]
    fn connections_list_every_shipped_pack_ready_or_not() {
        let mut panel = AiPanel::new(&atlas_shell::file_picker::DialogGate::new());
        panel
            .packs
            .set_health_override("cursor", PackHealth::Missing);
        panel.packs.refresh(None);
        while panel.packs.probe_pending() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let shown = texts(&mut panel, |p, ui| {
            connections_section(p, ui, Color32::GRAY);
        });
        for name in [
            "Connections",
            "Cursor",
            "Codex",
            "Ollama",
            "ComfyUI",
            "OpenAI",
            "PDFium",
            "Highlight",
        ] {
            assert!(shown.iter().any(|t| t == name), "{name}: {shown:?}");
        }
        assert!(shown.iter().any(|t| t == "Missing"), "{shown:?}");
    }
}
