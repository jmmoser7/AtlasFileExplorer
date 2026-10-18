use super::{hats, model::*, state::*, ui as chrome, PresencePainter};
use crate::theme::Palette;
use eframe::egui::{self, Id, RichText, Sense, Ui, Vec2};

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
    if p.local_profile || !p.human {
        let edit = state.edits.entry(p.id.into()).or_default();
        let response = ui.add_sized(
            Vec2::new(ui.available_width(), 36.0),
            egui::TextEdit::singleline(&mut edit.name)
                .id(Id::new(("presence-name", p.id)))
                .horizontal_align(egui::Align::Center)
                .font(chrome::font(ui, 16.0))
                .char_limit(32),
        );
        if response.changed() && !edit.name.trim().is_empty() {
            actions.push(PresenceAction::Rename {
                id: p.id.into(),
                name: edit.name.trim().into(),
            });
        }
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("Changes save automatically")
                    .size(10.0)
                    .color(palette.sub),
            );
        });
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
pub(super) fn add(
    ui: &mut Ui,
    palette: &Palette,
    state: &mut RosterState,
    providers: &[ProviderOption<'_>],
    actions: &mut Vec<PresenceAction>,
) {
    ui.add_space(6.0);
    ui.strong("Add a collaborator");
    ui.add_space(8.0);
    ui.add(
        egui::TextEdit::singleline(&mut state.add_name)
            .hint_text("Name")
            .char_limit(32)
            .desired_width(ui.available_width()),
    );
    if state.add_provider.is_empty() {
        state.add_provider = providers.first().map_or("", |p| p.id).into();
    }
    ui.add_space(8.0);
    for provider in providers {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
            chrome::provider(ui, rect, provider.id, palette.ink);
            ui.selectable_value(&mut state.add_provider, provider.id.into(), provider.label);
        });
    }
    if let Some(hat) = hats::picker(ui, palette, &state.add_hat, Id::new("new-agent-hats")) {
        state.add_hat = hat;
    }
    ui.label(RichText::new("Grants access to this board and named chats. Source files stay private; edits begin as proposals. Save this workbook before requesting board edits.").size(11.0).color(palette.sub));
    ui.add_space(8.0);
    if ui
        .add_enabled(
            !state.add_name.trim().is_empty() && !state.add_provider.is_empty(),
            egui::Button::new("Add collaborator").min_size(Vec2::new(ui.available_width(), 34.0)),
        )
        .clicked()
    {
        actions.push(PresenceAction::Add {
            name: state.add_name.trim().into(),
            hat: state.add_hat.clone(),
            provider: state.add_provider.clone(),
        });
        state.close_chat();
    }
    ui.add_space(4.0);
    ui.separator();
    if chrome::menu_row(ui, palette, chrome::Glyph::Chat, "Connect people…") {
        actions.push(PresenceAction::ConnectPeople);
        state.close_chat();
    }
}
