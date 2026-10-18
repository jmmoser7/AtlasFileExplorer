use super::{hats, model::*, state::*, ui as chrome, PresencePainter};
use crate::theme::Palette;
use eframe::egui::{self, Id, Rect, RichText, Sense, Ui, Vec2};

#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut Ui,
    palette: &Palette,
    painter: &mut PresencePainter,
    state: &mut RosterState,
    p: &Participant<'_>,
    now: f64,
    reduced: bool,
    actions: &mut Vec<PresenceAction>,
) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(if p.local_profile {
                "YOUR PROFILE"
            } else {
                "PROFILE"
            })
            .size(10.0)
            .color(palette.sub),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("×").clicked() {
                state.close_chat();
            }
        });
    });
    ui.add_space(16.0);
    ui.vertical_centered(|ui| {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(68.0), Sense::click());
        chrome::participant_face(ui, painter, p, "profile", rect, now, reduced);
        if response
            .on_hover_text(if p.local_profile {
                "Add or change profile picture"
            } else {
                "Open conversation"
            })
            .clicked()
        {
            actions.push(if p.local_profile {
                PresenceAction::SetPhoto { id: p.id.into() }
            } else {
                PresenceAction::ShowConversation { id: p.id.into() }
            });
        }
        ui.add_space(9.0);
        if p.local_profile {
            if ui
                .link(if p.photo_texture.is_some() {
                    "Change profile picture"
                } else {
                    "Add profile picture"
                })
                .clicked()
            {
                actions.push(PresenceAction::SetPhoto { id: p.id.into() });
            }
            if p.photo_texture.is_some() && ui.link("Remove profile picture").clicked() {
                actions.push(PresenceAction::RemovePhoto { id: p.id.into() });
            }
        }
    });
    ui.add_space(14.0);
    if p.local_profile {
        name_editor(ui, state, p, actions);
    } else {
        ui.vertical_centered(|ui| {
            ui.heading(p.name);
        });
    }
    if !p.human {
        ui.add_space(12.0);
        if chrome::menu_row(ui, palette, chrome::Glyph::Hats, "Identity hat") {
            state.toggle_popover(Popover::Hats);
        }
        if state.popover == Some(Popover::Hats) {
            if let Some(hat) = hats::picker(ui, palette, p.hat, Id::new(("profile-hats", p.id))) {
                actions.push(PresenceAction::SetHat {
                    id: p.id.into(),
                    hat,
                });
            }
        }
    }
    ui.add_space(12.0);
    if chrome::menu_row(ui, palette, chrome::Glyph::Chat, "Conversation") {
        state.open_chat(p.id);
    }
    ui.add_space(5.0);
}

pub(super) fn name_editor(
    ui: &mut Ui,
    state: &mut RosterState,
    p: &Participant<'_>,
    actions: &mut Vec<PresenceAction>,
) {
    let edit = state.edits.entry(p.id.into()).or_default();
    let response = ui.add_sized(
        Vec2::new(ui.available_width(), 36.0),
        egui::TextEdit::singleline(&mut edit.name)
            .id(Id::new(("presence-name", p.id)))
            .font(chrome::font(ui, 14.0))
            .char_limit(32),
    );
    if response.changed() && !edit.name.trim().is_empty() {
        actions.push(PresenceAction::Rename {
            id: p.id.into(),
            name: edit.name.trim().into(),
        });
    }
}

pub(super) fn add(
    ui: &mut Ui,
    palette: &Palette,
    state: &mut RosterState,
    providers: &[ProviderOption<'_>],
    actions: &mut Vec<PresenceAction>,
) {
    ui.horizontal(|ui| {
        ui.strong("Add an agent");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("×").clicked() {
                state.close_chat();
            }
        });
    });
    ui.add_space(8.0);
    let choices: Vec<_> = providers
        .iter()
        .filter(|provider| matches!(provider.id, "codex" | "cursor" | "dot"))
        .collect();
    if !state.workspace_ready {
        ui.label(
            RichText::new("Choose an AI workspace before adding an agent.")
                .size(11.0)
                .color(palette.sub),
        );
        if ui.link("Set AI workspace…").clicked() {
            actions.push(PresenceAction::SetupWorkspace);
        }
        ui.add_space(6.0);
    }
    for provider in choices.iter().copied() {
        let (rect, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::hover());
        let response = ui.interact(
            rect,
            Id::new(("presence-add-provider", provider.id)),
            if state.workspace_ready {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                state.workspace_ready,
                provider.label,
            )
        });
        if response.hovered() {
            ui.painter().rect_filled(rect, 10, palette.card_hover);
        }
        chrome::provider(
            ui,
            Rect::from_center_size(
                egui::pos2(rect.left() + 20.0, rect.center().y),
                Vec2::splat(24.0),
            ),
            provider.id,
            palette.ink,
        );
        chrome::label(
            ui,
            Rect::from_min_max(egui::pos2(rect.left() + 42.0, rect.top()), rect.max),
            provider.label,
            14.0,
            palette.ink,
            egui::Align2::LEFT_CENTER,
        );
        if response.clicked() {
            actions.push(PresenceAction::Add {
                name: provider.label.into(),
                hat: "cap".into(),
                provider: provider.id.into(),
            });
            state.close_chat();
        }
    }
}
