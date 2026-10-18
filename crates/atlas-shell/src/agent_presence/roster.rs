//! The shared participant corner and its single expanding conversation surface.
use super::{conversation, model::*, profile, state::*, ui as chrome, PresencePainter};
use crate::theme::Palette;
use eframe::egui::{self, Align2, Id, Pos2, Rect, Sense, Ui, Vec2};

pub(super) fn segment_width(ui: &Ui, name: &str, compact: bool) -> f32 {
    if compact {
        return 50.0;
    }
    let text = ui.painter().layout_no_wrap(
        name.into(),
        chrome::font(ui, 13.0),
        ui.visuals().text_color(),
    );
    48.0 + text.size().x.min(132.0) + 18.0
}

#[allow(clippy::too_many_arguments)]
pub fn participant_capsules(
    ui: &mut Ui,
    palette: &Palette,
    painter: &mut PresencePainter,
    state: &mut RosterState,
    participants: &[Participant<'_>],
    providers: &[ProviderOption<'_>],
    now: f64,
    reduced: bool,
) -> Vec<PresenceAction> {
    state.last_popover_rect = Rect::NOTHING;
    state.last_model_popup_rect = Rect::NOTHING;
    let palette = chrome::presentation_palette(palette);
    ui.scope(|ui| {
        chrome::configure(ui, state.font_family.as_ref(), &palette);
        capsules_ui(
            ui,
            &palette,
            painter,
            state,
            participants,
            providers,
            now,
            reduced,
        )
    })
    .inner
}

#[allow(clippy::too_many_arguments)]
fn capsules_ui(
    ui: &mut Ui,
    palette: &Palette,
    painter: &mut PresencePainter,
    state: &mut RosterState,
    participants: &[Participant<'_>],
    providers: &[ProviderOption<'_>],
    now: f64,
    reduced: bool,
) -> Vec<PresenceAction> {
    let mut actions = Vec::new();
    state
        .edits
        .retain(|id, _| participants.iter().any(|p| p.id == id));
    state
        .history_offsets
        .retain(|id, _| participants.iter().any(|p| p.id == id));
    for p in participants {
        let edit = state.edits.entry(p.id.into()).or_default();
        let focused = ui.ctx().memory(|m| m.focused()) == Some(Id::new(("presence-name", p.id)));
        if edit.name.is_empty() || (!focused && edit.name != p.name) {
            edit.name = p.name.into();
        }
        if edit.last_draft != p.draft {
            edit.message = p.draft.into();
            edit.last_draft = p.draft.into();
        }
    }
    let wanted: f32 = participants
        .iter()
        .map(|p| segment_width(ui, p.name, false))
        .sum::<f32>()
        + 44.0;
    let available = ui.available_width().max(90.0);
    let compact = wanted > available;
    let width = participants
        .iter()
        .map(|p| segment_width(ui, p.name, compact))
        .sum::<f32>()
        + 44.0;
    let (roster, _) = ui.allocate_exact_size(Vec2::new(width.min(available), 40.0), Sense::hover());
    chrome::surface(ui.painter(), roster, 20, palette, false);
    if width > available && ui.rect_contains_pointer(roster) {
        let delta = ui.input(|i| i.smooth_scroll_delta.x + i.smooth_scroll_delta.y);
        state.roster_scroll = (state.roster_scroll - delta).clamp(0.0, width - available);
        crate::menu_wheel::claim(ui.ctx(), roster);
    } else {
        state.roster_scroll = state.roster_scroll.min((width - available).max(0.0));
    }
    let mut x = roster.left() + 4.0 - state.roster_scroll;
    let clip = ui.clip_rect();
    ui.set_clip_rect(clip.intersect(Rect::from_min_max(
        roster.min,
        Pos2::new(roster.right() - 42.0, roster.bottom()),
    )));
    for (index, p) in participants.iter().enumerate() {
        let width = segment_width(ui, p.name, compact);
        let rect = Rect::from_min_size(Pos2::new(x, roster.top() + 4.0), Vec2::new(width, 32.0));
        let icon = Rect::from_min_size(rect.min, Vec2::splat(30.0));
        let name = Rect::from_min_max(Pos2::new(icon.right(), rect.top()), rect.max);
        let icon_hit = ui
            .interact(icon, Id::new(("presence-chat", p.id)), Sense::click())
            .on_hover_text(format!("Chat with {}", p.name));
        let name_hit = ui
            .interact(name, Id::new(("presence-profile", p.id)), Sense::click())
            .on_hover_text(if p.human {
                format!("{} profile", p.name)
            } else {
                format!("Chat with {}", p.name)
            });
        if icon_hit.hovered() {
            ui.painter().rect_filled(icon, 15, palette.card_hover);
        }
        if name_hit.hovered() {
            ui.painter().rect_filled(name, 15, palette.card_hover);
        }
        let row_clip = ui.clip_rect();
        ui.set_clip_rect(clip.intersect(Rect::from_min_max(
            Pos2::new(roster.left(), roster.top() - 14.0),
            Pos2::new(roster.right() - 42.0, roster.bottom()),
        )));
        chrome::participant_face(ui, painter, p, "roster", icon, now, reduced);
        ui.set_clip_rect(row_clip);
        if !compact {
            chrome::label(
                ui,
                name.shrink2(Vec2::new(7.0, 0.0)),
                p.name,
                13.0,
                palette.sub,
                Align2::LEFT_CENTER,
            );
        }
        chrome::chevron(
            ui.painter(),
            Pos2::new(name.right() - 9.0, name.center().y),
            palette.sub,
        );
        if icon_hit.clicked() {
            state.open_chat(p.id);
        }
        if name_hit.clicked() {
            if p.human {
                state.toggle(Expansion::Profile(p.id.into()), rect);
            } else {
                state.open_chat(p.id);
            }
        }
        if state.canvas_anchor.is_none()
            && matches!(
                state.expansion.as_ref(),
                Some(Expansion::Chat(id) | Expansion::Profile(id)) if id == p.id
            )
        {
            state.anchor = rect;
        }
        x += width;
        if index + 1 < participants.len() {
            ui.painter().line_segment(
                [
                    Pos2::new(x, roster.top() + 10.0),
                    Pos2::new(x, roster.bottom() - 10.0),
                ],
                egui::Stroke::new(1.0_f32, palette.border),
            );
        }
    }
    let add = Rect::from_min_max(
        Pos2::new(roster.right() - 38.0, roster.top() + 4.0),
        Pos2::new(roster.right() - 4.0, roster.bottom() - 4.0),
    );
    if matches!(state.expansion.as_ref(), Some(Expansion::Add)) {
        state.anchor = add;
    }
    ui.set_clip_rect(clip.intersect(roster));
    ui.painter().line_segment(
        [
            Pos2::new(add.left() - 2.0, roster.top() + 10.0),
            Pos2::new(add.left() - 2.0, roster.bottom() - 10.0),
        ],
        egui::Stroke::new(1.0_f32, palette.border),
    );
    if chrome::glyph_button(
        ui,
        Id::new("presence-add"),
        add,
        chrome::Glyph::Plus,
        "Add a collaborator",
        palette,
    )
    .clicked()
    {
        state.toggle(Expansion::Add, add);
    }
    ui.set_clip_rect(clip);
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        if state.model_menu_open {
            state.model_menu_open = false;
        } else if state.requested_provider.take().is_none() && state.popover.take().is_none() {
            state.close_chat();
        }
    }
    if let Some(expansion) = state.expansion.clone() {
        let participant = match &expansion {
            Expansion::Chat(id) | Expansion::Profile(id) => {
                participants.iter().find(|p| p.id == id)
            }
            _ => None,
        };
        if !matches!(expansion, Expansion::Add) && participant.is_none() {
            state.close_chat();
            return actions;
        }
        let attached = state
            .canvas_anchor
            .filter(|_| matches!(expansion, Expansion::Chat(_)));
        let screen = ui.ctx().screen_rect();
        let width = if attached.is_some() {
            300.0
        } else if matches!(expansion, Expansion::Chat(_)) {
            360.0
        } else {
            248.0
        };
        let mut top = Pos2::new(
            (state.anchor.right() - width)
                .max(screen.left() + 12.0)
                .min((screen.right() - width - 12.0).max(screen.left() + 12.0)),
            roster.bottom() + 10.0,
        );
        let available_height = (screen.bottom() - top.y - 16.0).max(0.0);
        let mut height = if available_height.is_nan() {
            480.0
        } else {
            available_height.min(480.0)
        };
        let zoom = attached.map_or(1.0, |(_, z)| z);
        if let Some((point, z)) = attached {
            top = Pos2::new(point.x / z, point.y / z);
            height = 300.0;
        }
        let area_id = Id::new("presence-expansion");
        let layer = egui::LayerId::new(egui::Order::Foreground, area_id);
        ui.ctx()
            .set_transform_layer(layer, egui::emath::TSTransform::from_scaling(zoom));
        // Area installs its constraint as the local clip even when movement
        // constraints are disabled. Screen bounds must enter this layer's space.
        let local_screen = egui::emath::TSTransform::from_scaling(zoom).inverse() * screen;
        let area = egui::Area::new(area_id)
            .order(egui::Order::Foreground)
            .fixed_pos(top)
            .constrain_to(local_screen)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                chrome::configure(ui, state.font_family.as_ref(), palette);
                ui.set_width(width.min((local_screen.width() - 24.0).max(0.0)));
                egui::Frame::new()
                    .fill(palette.card)
                    .corner_radius(24)
                    .inner_margin(8)
                    .shadow(chrome::shadow(palette))
                    .show(ui, |ui| match expansion {
                        Expansion::Chat(_) => conversation::show(
                            ui,
                            palette,
                            painter,
                            state,
                            participant.unwrap(),
                            providers,
                            now,
                            reduced,
                            height - 16.0,
                            attached.is_some(),
                            &mut actions,
                        ),
                        Expansion::Profile(_) => profile::show(
                            ui,
                            palette,
                            painter,
                            state,
                            participant.unwrap(),
                            now,
                            reduced,
                            &mut actions,
                        ),
                        Expansion::Add => profile::add(ui, palette, state, providers, &mut actions),
                    });
            });
        state.last_popup_rect = egui::emath::TSTransform::from_scaling(zoom) * area.response.rect;
        state.last_painted_pass = Some(ui.ctx().cumulative_pass_nr());
        crate::menu_wheel::claim(ui.ctx(), state.last_popup_rect);
        if !state.just_opened && ui.ctx().input(|i| i.pointer.any_pressed()) {
            if let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
                if matches!(expansion, Expansion::Profile(_) | Expansion::Add)
                    && !state.occludes_point(ui.ctx(), pos)
                    && !roster.contains(pos)
                {
                    state.close_chat();
                }
            }
        }
        state.just_opened = false;
    }
    actions
}

pub fn retired_profiles_ui(ui: &mut Ui, profiles: &[RetiredProfile<'_>]) -> Vec<PresenceAction> {
    let mut actions = Vec::new();
    ui.strong("Retired collaborators");
    ui.small("Restore identity, conversation, and saved access choices.");
    for profile in profiles {
        ui.horizontal(|ui| {
            ui.label(profile.name);
            ui.weak(profile.provider);
            if ui.small_button("Restore").clicked() {
                actions.push(PresenceAction::Restore {
                    id: profile.id.into(),
                });
            }
        });
    }
    if profiles.is_empty() {
        ui.weak("No retired collaborators.");
    }
    actions
}
pub fn presence_preferences_ui(ui: &mut Ui, reduced_motion: &mut bool) -> bool {
    let changed = ui
        .checkbox(reduced_motion, "Reduced motion for collaborators")
        .changed();
    ui.small("Keeps visible travel and progress; reduces expressions, prop loops and departures.");
    changed
}
#[cfg(test)]
#[path = "roster_tests.rs"]
mod tests;
