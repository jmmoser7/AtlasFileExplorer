//! Pack surfaces both apps call: the Connections section of Advanced and the
//! OpenAI key window. The shell paints the table; this module wires it to the
//! session catalog.

use eframe::egui::{self, Color32, RichText};

use super::AiPanel;
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
        reset_credential_ui(ctx, egui::Id::new("atlas-ai-openai-key"));
        return None;
    }
    match credential_key_dialog(
        ctx,
        egui::Id::new("atlas-ai-openai-key"),
        "OpenAI API",
        panel.packs.credential_present("openai-text") == Some(true),
        &mut panel.packs.key_draft,
    ) {
        CredentialAction::Save => {
            let workspace = panel.config.workspace_dir.clone();
            Some(panel.packs.save_openai_key(workspace))
        }
        CredentialAction::Cancel => {
            panel.packs.key_entry = false;
            None
        }
        CredentialAction::None => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CredentialAction {
    None,
    Cancel,
    Save,
}
#[derive(Clone, Copy)]
struct CredentialTopSample {
    pass: u64,
    ordered_top: Option<egui::LayerId>,
}

fn reset_credential_ui(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|data| {
        data.remove::<bool>(id.with("focus"));
        data.remove::<CredentialTopSample>(id.with("ordered-top"));
    });
}

/// One explicit masked form; callers retain their existing secure storage owner.
pub fn credential_key_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    provider_name: &str,
    present: bool,
    draft: &mut String,
) -> CredentialAction {
    let layer = egui::LayerId::new(atlas_shell::selection_tools::POPUP_ORDER, id);
    let focus_id = id.with("focus");
    let pass = ctx.cumulative_pass_nr();
    let (owns_top, ordered_top) = ctx.memory(|memory| {
        let areas = memory.areas();
        let ordered_top = memory
            .layer_ids()
            .filter(|candidate| candidate.order == layer.order && areas.is_visible(candidate))
            .last();
        (
            memory.top_modal_layer().is_none_or(|top| top == layer),
            ordered_top,
        )
    });
    // Modal registration precedes Area order updates in egui. Let a changed
    // ordered top settle for one contiguous pass before promoting over it.
    let stable_top = ctx.data_mut(|data| {
        let previous = data.get_temp::<CredentialTopSample>(id.with("ordered-top"));
        let contiguous = previous.is_some_and(|sample| sample.pass.checked_add(1) == Some(pass));
        if !contiguous {
            data.remove::<bool>(focus_id);
        }
        data.insert_temp(
            id.with("ordered-top"),
            CredentialTopSample { pass, ordered_top },
        );
        contiguous && previous.is_some_and(|sample| sample.ordered_top == ordered_top)
    });
    let eligible = owns_top && (ordered_top == Some(layer) || stable_top);
    let escape = eligible
        && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if eligible {
        ctx.move_to_top(layer);
    }
    let mut save = false;
    let mut cancel = false;
    let title = if present {
        "Change API key"
    } else {
        "Add API key"
    };
    let dark = ctx.style().visuals.dark_mode;
    let ink = if dark {
        Color32::from_rgb(154, 166, 181)
    } else {
        Color32::from_rgb(91, 101, 114)
    };
    let line = if dark {
        Color32::from_rgb(46, 54, 65)
    } else {
        Color32::from_rgb(213, 219, 225)
    };
    let background = if dark {
        Color32::from_rgb(21, 26, 33)
    } else {
        Color32::from_rgb(237, 240, 243)
    };
    let take_focus = eligible
        && ctx.data_mut(|data| {
            let first = !data.get_temp::<bool>(focus_id).unwrap_or(false);
            data.insert_temp(focus_id, true);
            first
        });
    let modal = egui::Modal::new(id)
        .area(egui::Modal::default_area(id).order(layer.order))
        .backdrop_color(Color32::from_black_alpha(102))
        .frame(egui::Frame::NONE.fill(background).stroke(egui::Stroke::new(1.0_f32, line)).corner_radius(8).inner_margin(22))
        .show(ctx, |ui| {
            ui.set_width(354.0);
            let dialog_family = egui::FontFamily::Name("slate-segoe".into());
            let dialog_family = if ctx.fonts(|fonts| fonts.families().contains(&dialog_family)) { dialog_family } else { egui::FontFamily::Proportional };
            for style in [egui::TextStyle::Body, egui::TextStyle::Button, egui::TextStyle::Small, egui::TextStyle::Heading] {
                ui.style_mut().text_styles.insert(style, egui::FontId::new(12.0, dialog_family.clone()));
            }
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
            ui.visuals_mut().override_text_color = Some(ink);
            let card = if dark {Color32::from_rgb(28,32,38)} else {Color32::WHITE};
            let primary = if dark {Color32::from_rgb(221,226,232)} else {Color32::from_rgb(27,30,34)};
            let visuals = ui.visuals_mut();
            for widget in [&mut visuals.widgets.inactive,&mut visuals.widgets.hovered,&mut visuals.widgets.active] {
                widget.bg_fill=card;widget.weak_bg_fill=card;widget.bg_stroke=egui::Stroke::new(1.0_f32,line);
                widget.corner_radius=egui::CornerRadius::same(4);widget.fg_stroke.color=ink;
            }
            ui.label(RichText::new(title).size(14.0));
            ui.add_space(12.0);
            ui.label(RichText::new(provider_name).size(11.0));
            ui.add_space(11.0);
            ui.label(RichText::new("API key").size(12.0));
            ui.add_space(7.0);
            let field = ui.add(
                egui::TextEdit::singleline(draft)
                    .interactive(eligible)
                    .password(true)
                    .background_color(card)
                    .font(egui::FontId::proportional(14.0))
                    .margin(egui::vec2(11.0, 10.0))
                    .hint_text("Enter an API key")
                    .desired_width(354.0),
            );
            if take_focus { field.request_focus(); }
            let enter = eligible && field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_space(11.0);
            ui.label(RichText::new("The key stays on this machine for this Windows user. It is never stored in the workbook or sent in a chat.").size(11.0));
            ui.add_space(30.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().button_padding = egui::vec2(10.0, 5.0);
                let ready = !draft.trim().is_empty();
                let save_clicked = ui.add_enabled(ready, egui::Button::new(RichText::new("Save key").size(12.0).color(card)).fill(primary).stroke(egui::Stroke::new(1.0_f32,primary)).min_size(egui::vec2(0.0, 30.0))).clicked();
                save = eligible && ready && (save_clicked || enter);
                cancel = ui.add(egui::Button::new(RichText::new("Cancel").size(12.0).color(ink)).fill(Color32::TRANSPARENT).stroke(egui::Stroke::new(1.0_f32,line)).min_size(egui::vec2(0.0, 30.0))).clicked() && eligible;
            });
        });
    if (eligible && modal.should_close()) || cancel || escape {
        draft.clear();
        reset_credential_ui(ctx, id);
        return CredentialAction::Cancel;
    }
    if save {
        reset_credential_ui(ctx, id);
        CredentialAction::Save
    } else {
        CredentialAction::None
    }
}
#[cfg(test)]
mod credential_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_credential_form_masks_input_and_escape_cancels_without_a_storage_call() {
        let id = egui::Id::new("credential-mask-cancel");
        let mut draft = "SYNTHETIC_MASK_TEST".to_owned();
        for provider in ["OpenAI API", "Cursor API"] {
            let ctx = egui::Context::default();
            let mut result = CredentialAction::None;
            let mut settled = None;
            for _ in 0..3 {
                settled = Some(ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(900.0, 700.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| result = credential_key_dialog(ctx, id, provider, false, &mut draft),
                ));
            }
            fn texts(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                    egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| texts(shape, out)),
                    _ => {}
                }
            }
            let output = settled.unwrap();
            let mut painted = Vec::new();
            output
                .shapes
                .iter()
                .for_each(|shape| texts(&shape.shape, &mut painted));
            for label in ["Add API key", provider, "API key", "Save key", "Cancel"] {
                assert!(
                    painted.iter().any(|text| text == label),
                    "settled actual form missing {label}: {painted:?}"
                );
            }
            let mask: String = std::iter::repeat_n(
                egui::epaint::text::PASSWORD_REPLACEMENT_CHAR,
                draft.chars().count(),
            )
            .collect();
            assert!(
                painted.iter().any(|text| text == &mask),
                "actual password field must positively paint its mask"
            );
            assert!(!painted
                .iter()
                .any(|text| text.contains("SYNTHETIC_MASK_TEST")));
            assert!(matches!(result, CredentialAction::None));
            let _ = ctx.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                },
                |ctx| result = credential_key_dialog(ctx, id, provider, false, &mut draft),
            );
            assert!(matches!(result, CredentialAction::Cancel));
            assert!(draft.is_empty());
            draft = "SYNTHETIC_MASK_TEST".into();
        }
    }
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
