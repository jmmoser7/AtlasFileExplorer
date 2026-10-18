use super::{art, model::*, state::*, ui as chrome};
use crate::theme::Palette;
use eframe::egui::{self, Align2, Pos2, Rect, RichText, Sense, Ui, Vec2};

pub(super) fn show(
    ui: &mut Ui,
    palette: &Palette,
    state: &mut RosterState,
    p: &Participant<'_>,
    providers: &[ProviderOption<'_>],
    actions: &mut Vec<PresenceAction>,
) {
    let popover = state.popover;
    egui::ScrollArea::vertical()
        .id_salt(("presence-options", p.id, popover))
        .max_height(240.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                egui::Frame::new()
                    .fill(palette.window)
                    .corner_radius(14)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.set_width((ui.available_width() - 8.0).min(246.0));
                        match popover {
                            Some(Popover::More) => more(ui, palette, state, p, actions),
                            Some(Popover::Provider) => {
                                provider_menu(ui, palette, state, p, providers, actions)
                            }
                            Some(Popover::Advanced) => advanced(ui, state, p, actions),
                            Some(Popover::Retirement) => retirement(ui, palette, state, p, actions),
                            _ => {}
                        }
                    });
            });
        });
    ui.add_space(8.0);
}
fn more(
    ui: &mut Ui,
    palette: &Palette,
    state: &mut RosterState,
    p: &Participant<'_>,
    actions: &mut Vec<PresenceAction>,
) {
    if chrome::menu_row(ui, palette, chrome::Glyph::Chat, "Full conversation") {
        actions.push(PresenceAction::ShowConversation { id: p.id.into() });
        state.popover = None;
    }
    if !p.human {
        if chrome::menu_row(ui, palette, chrome::Glyph::Clone, "Clone agent") {
            actions.push(PresenceAction::Clone { id: p.id.into() });
            state.popover = None;
        }
        if chrome::menu_row(ui, palette, chrome::Glyph::Lock, "Access settings") {
            state.popover = Some(Popover::Advanced);
        }
        if chrome::menu_row(ui, palette, chrome::Glyph::Schedule, "Schedule…") {
            actions.push(PresenceAction::Schedule { id: p.id.into() });
            state.popover = None;
        }
        ui.separator();
        if chrome::menu_row(ui, palette, chrome::Glyph::Exit, "Retire agent") {
            state.popover = Some(Popover::Retirement);
        }
    }
}
fn provider_menu(
    ui: &mut Ui,
    palette: &Palette,
    state: &mut RosterState,
    p: &Participant<'_>,
    providers: &[ProviderOption<'_>],
    actions: &mut Vec<PresenceAction>,
) {
    for option in providers {
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 36.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 8, palette.card_hover);
        }
        chrome::provider(
            ui,
            Rect::from_center_size(
                Pos2::new(rect.left() + 14.0, rect.center().y),
                Vec2::splat(21.0),
            ),
            option.id,
            palette.ink,
        );
        chrome::label(
            ui,
            Rect::from_min_max(
                rect.min + Vec2::new(35.0, 0.0),
                rect.max - Vec2::new(22.0, 0.0),
            ),
            option.label,
            14.0,
            palette.ink,
            Align2::LEFT_CENTER,
        );
        if option.id == p.provider {
            chrome::label(
                ui,
                Rect::from_min_max(Pos2::new(rect.right() - 20.0, rect.top()), rect.max),
                "✓",
                13.0,
                palette.sub,
                Align2::CENTER_CENTER,
            );
        }
        if response.clicked() && option.id != p.provider {
            if p.busy {
                state.requested_provider = Some(option.id.into());
            } else {
                actions.push(PresenceAction::SetProvider {
                    id: p.id.into(),
                    provider: option.id.into(),
                });
                state.popover = None;
            }
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new("Model").small());
        if p.models.is_empty() {
            ui.weak(if p.model.is_empty() {
                "Default"
            } else {
                p.model
            });
        } else {
            egui::ComboBox::from_id_salt(("presence-model", p.id))
                .selected_text(if p.model.is_empty() {
                    "Default"
                } else {
                    p.model
                })
                .width(145.0)
                .show_ui(ui, |ui| {
                    for model in p.models {
                        if ui
                            .selectable_label(model.id == p.model, model.label)
                            .clicked()
                            && model.id != p.model
                        {
                            actions.push(PresenceAction::SetModel {
                                id: p.id.into(),
                                model: model.id.into(),
                            });
                        }
                    }
                });
        }
    });
    ui.label(RichText::new("Your conversation stays with this agent. The next message starts in the chosen harness.").size(10.0).color(palette.sub));
    if let Some(queued) = p.queued_provider {
        ui.separator();
        ui.small(format!("Switching to {queued} after this task"));
        if ui.small_button("Cancel switch").clicked() {
            actions.push(PresenceAction::CancelProviderSwitch { id: p.id.into() });
        }
    }
    if let Some(provider) = state.requested_provider.clone() {
        ui.separator();
        ui.small("A task is running. When should the harness change?");
        for (label, after_task) in [("After this task", true), ("Stop task and switch", false)] {
            if ui
                .add_sized(
                    Vec2::new(ui.available_width(), 31.0),
                    egui::Button::new(label),
                )
                .clicked()
            {
                actions.push(PresenceAction::SwitchProvider {
                    id: p.id.into(),
                    provider: provider.clone(),
                    after_task,
                });
                state.requested_provider = None;
                state.popover = None;
            }
        }
        if ui.small_button("Cancel").clicked() {
            state.requested_provider = None;
        }
        ui.label(RichText::new("Stopping cancels current work and pending proposals. Completed board edits remain; the task is not resent.").size(10.0).color(palette.sub));
    }
}
fn advanced(
    ui: &mut Ui,
    state: &mut RosterState,
    p: &Participant<'_>,
    actions: &mut Vec<PresenceAction>,
) {
    ui.strong("Board access");
    ui.small("This board and its named chats. Source files are never edited.");
    let mut autonomous = p.autonomous;
    if ui
        .checkbox(&mut autonomous, "Autonomous board edits")
        .changed()
    {
        actions.push(PresenceAction::SetAutonomy {
            id: p.id.into(),
            enabled: autonomous,
        });
    }
    ui.small("Off means propose changes for review.");
    let mut inputs = p.file_inputs;
    if ui
        .checkbox(&mut inputs, "Read board images and wired inputs")
        .changed()
    {
        actions.push(PresenceAction::SetFileInputs {
            id: p.id.into(),
            enabled: inputs,
        });
    }
    ui.small("Only already-readable board images and explicitly wired generator sources. Cloud-only files stay guarded; images are never attached in bulk.");
    let mut own_chats = p.own_chats;
    if ui
        .checkbox(&mut own_chats, "Allow chats created by this agent")
        .changed()
    {
        actions.push(PresenceAction::SetOwnChats {
            id: p.id.into(),
            enabled: own_chats,
        });
    }
    ui.small("This live session only. Unrelated chats stay outside its scope.");
    ui.separator();
    if ui.button("Copy @identity").clicked() {
        actions.push(PresenceAction::CopyIdentity { id: p.id.into() });
    }
    if matches!(p.provider, "local" | "cursor") && ui.button("Copy connection folder").clicked() {
        actions.push(PresenceAction::CopyLink { id: p.id.into() });
    }
    if p.parent_name.is_none() && ui.button("Add temporary helper").clicked() {
        actions.push(PresenceAction::SpawnHelper { id: p.id.into() });
    }
    if let Some(parent) = p.parent_name {
        ui.small(format!("Temporary helper of {parent}"));
    }
    if ui.button("Inspect public activity").clicked() {
        actions.push(PresenceAction::ShowActivity { id: p.id.into() });
        state.popover = None;
    }
    ui.separator();
    if ui.button("Revoke board access").clicked() {
        actions.push(PresenceAction::Revoke { id: p.id.into() });
        state.popover = None;
    }
}
fn retirement(
    ui: &mut Ui,
    palette: &Palette,
    state: &mut RosterState,
    p: &Participant<'_>,
    actions: &mut Vec<PresenceAction>,
) {
    ui.label(
        RichText::new("A little farewell")
            .size(15.0)
            .color(palette.ink),
    );
    for departure in &art::master().departures {
        ui.selectable_value(
            &mut state.retirement,
            departure.id.clone(),
            &departure.label,
        );
    }
    ui.small("Stops work and releases board access. Identity and conversation stay in Advanced → Retired collaborators.");
    if chrome::menu_row(ui, palette, chrome::Glyph::Exit, "Retire agent") {
        actions.push(PresenceAction::Retire {
            id: p.id.into(),
            style: state.retirement.clone(),
        });
        state.close_chat();
    }
    if ui.small_button("Keep working").clicked() {
        state.popover = None;
    }
}
