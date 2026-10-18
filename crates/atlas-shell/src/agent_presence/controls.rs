use super::{art, model::*, state::*, ui as chrome};
use crate::theme::Palette;
use eframe::egui::{self, Align2, Pos2, Rect, RichText, Sense, Ui, Vec2};

pub(super) fn show(
    ui: &mut Ui,
    palette: &Palette,
    state: &mut RosterState,
    p: &Participant<'_>,
    providers: &[ProviderOption<'_>],
    header: Rect,
    actions: &mut Vec<PresenceAction>,
) {
    let Some(
        popover @ (Popover::Advanced | Popover::Retirement | Popover::Provider | Popover::More),
    ) = state.popover
    else {
        return;
    };
    let ctx = ui.ctx();
    let transform = ctx
        .layer_transform_to_global(ui.layer_id())
        .unwrap_or_default();
    let local_screen = transform.inverse() * ctx.screen_rect();
    let more = popover == Popover::More;
    let width = if more { 211.0 } else { 238.0 };
    let top = header.bottom() + if more { -9.0 } else { 3.0 };
    let trigger = ctx
        .read_response(egui::Id::new((
            if popover == Popover::Provider {
                "provider-toggle"
            } else {
                "conversation-more"
            },
            p.id,
        )))
        .map(|response| {
            ctx.layer_transform_to_global(response.layer_id)
                .unwrap_or_default()
                * response.rect
        });
    let area_id = egui::Id::new(("presence-options", p.id));
    if popover != Popover::Provider || p.models.is_empty() {
        state.model_menu_open = false;
    }
    let area = crate::selection_tools::popup_area(ctx, area_id, ui.layer_id())
        .fixed_pos(Pos2::new(header.right() - width, top))
        .constrain_to(local_screen.shrink(8.0))
        .show(ctx, |ui| {
            chrome::configure(ui, state.font_family.as_ref(), palette);
            crate::menu::frame(palette.dark_mode)
                .fill(palette.card)
                .corner_radius(if more { 8 } else { 14 })
                .inner_margin(egui::Margin::symmetric(10, if more { 8 } else { 10 }))
                .shadow(chrome::shadow(palette))
                .show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    // Area remembers the previous content size. Let a newly
                    // expanded timing choice grow instead of clipping to it.
                    let available_height = local_screen.height() - 48.0;
                    let max_content_height = if available_height.is_nan() {
                        420.0
                    } else {
                        available_height.clamp(0.0, 420.0)
                    };
                    ui.set_max_height(max_content_height);
                    ui.spacing_mut().item_spacing.y = if more { 0.0 } else { 6.0 };
                    egui::ScrollArea::vertical()
                        .id_salt(("presence-options-scroll", p.id, popover))
                        .max_height(max_content_height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| match popover {
                            Popover::More => more_menu(ui, palette, state, p, actions),
                            Popover::Provider => {
                                provider_menu(ui, palette, state, p, providers, actions)
                            }
                            Popover::Advanced => advanced(ui, state, p, actions),
                            Popover::Retirement => retirement(ui, palette, state, p, actions),
                            Popover::Hats => unreachable!(),
                        });
                });
        });
    state.last_popover_rect = transform * area.response.rect;
    crate::menu_wheel::claim(ctx, state.last_popover_rect);
    let outside = ctx.input(|input| {
        input.pointer.any_pressed()
            && input.pointer.interact_pos().is_some_and(|point| {
                !state.last_popover_rect.contains(point)
                    && !state.last_model_popup_rect.contains(point)
                    && !trigger.is_some_and(|rect| rect.contains(point))
            })
    });
    // A model combo box owns its nested popup until that popup is dismissed.
    if outside && !state.model_menu_open {
        state.popover = None;
        state.requested_provider = None;
        state.model_menu_open = false;
    }
}
fn more_menu(
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
        if chrome::menu_row(ui, palette, chrome::Glyph::Lock, "Agent settings") {
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
            crate::menu::paint_icon(
                ui.painter(),
                crate::menu::MenuIcon::Check,
                Pos2::new(rect.right() - 10.0, rect.center().y),
                13.0,
                crate::menu::tokens().icon_stroke,
                palette.sub,
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
            let options: Vec<_> = p.models.iter().map(|model| model.label).collect();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(145.0, 24.0), Sense::hover());
            let menu = crate::selection_tools::bare_menu(
                ui,
                crate::selection_tools::BareMenu {
                    id: egui::Id::new(("presence-model", p.id)),
                    rect,
                    label: if p.model.is_empty() {
                        "Default"
                    } else {
                        p.model
                    },
                    options: &options,
                    selected: p
                        .models
                        .iter()
                        .position(|model| model.id == p.model)
                        .unwrap_or(usize::MAX),
                    open: &mut state.model_menu_open,
                    zoom: 1.0, // The complete world scale belongs to the popup layer.
                    theme: *palette,
                },
            );
            state.last_model_popup_rect = menu.popup.unwrap_or(Rect::NOTHING);
            if let Some(model) = menu
                .picked
                .and_then(|index| p.models.get(index))
                .filter(|model| model.id != p.model)
            {
                actions.push(PresenceAction::SetModel {
                    id: p.id.into(),
                    model: model.id.into(),
                });
            }
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
    ui.strong("Agent identity");
    super::profile::name_editor(ui, state, p, actions);
    ui.separator();
    if p.provider == "codex" && p.parent_name.is_none() {
        ui.strong("Provider access");
        let mut full_access = p.full_access;
        if ui.checkbox(&mut full_access, "Codex full access").changed() {
            actions.push(PresenceAction::SetFullAccess {
                id: p.id.into(),
                enabled: full_access,
            });
        }
        ui.small("Provider tools can run commands and write files without approval; board access remains scoped.");
        ui.separator();
    }
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
