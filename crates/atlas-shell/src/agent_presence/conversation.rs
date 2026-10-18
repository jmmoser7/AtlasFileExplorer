use super::{controls, hats, model::*, state::*, ui as chrome, PresencePainter};
use crate::theme::Palette;
use eframe::egui::{self, Align2, Id, Pos2, Rect, Sense, Ui, Vec2};

#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut Ui,
    palette: &Palette,
    painter: &mut PresencePainter,
    state: &mut RosterState,
    p: &Participant<'_>,
    providers: &[ProviderOption<'_>],
    now: f64,
    reduced: bool,
    height: f32,
    attached: bool,
    actions: &mut Vec<PresenceAction>,
) {
    let width = ui.available_width();
    ui.set_min_height(height);
    let (header, _) = ui.allocate_exact_size(Vec2::new(width, 48.0), Sense::hover());
    header_ui(
        ui, palette, painter, state, p, providers, now, reduced, header, attached, actions,
    );
    let mut occupied = 56.0;
    if !p.status.trim().is_empty() {
        // Keep delivery, availability, and persistence feedback in the same
        // conversation as the message. A long diagnostic stays readable on hover.
        let response = ui.add(
            egui::Label::new(egui::RichText::new(p.status).size(11.0).color(palette.sub))
                .truncate(),
        );
        occupied += response.rect.height() + ui.spacing().item_spacing.y;
        response.on_hover_text(p.status);
    }
    if state.popover == Some(Popover::Hats) {
        let selected = hats::picker(ui, palette, p.hat, Id::new(("conversation-hats", p.id)));
        if let Some(hat) = selected {
            actions.push(PresenceAction::SetHat {
                id: p.id.into(),
                hat,
            });
        }
        occupied += 110.0;
    }
    let composer_height = (if attached { 62.0 } else { 74.0 })
        + if p.attachments.is_empty() || p.human {
            0.0
        } else {
            29.0
        };
    let message_height = (height - occupied - composer_height - 12.0).max(54.0);
    super::transcript::show(
        ui,
        palette,
        painter,
        state,
        p,
        now,
        reduced,
        attached,
        message_height,
        actions,
    );
    ui.add_space(6.0);
    let (line, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width() - 16.0, 1.0), Sense::hover());
    ui.painter().line_segment(
        [
            line.left_center() + Vec2::new(8.0, 0.0),
            line.right_center(),
        ],
        egui::Stroke::new(1.0_f32, palette.border),
    );
    ui.add_space(8.0);
    if !p.human && !p.attachments.is_empty() {
        egui::ScrollArea::horizontal()
            .id_salt(("presence-attachments", p.id))
            .max_height(27.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (index, attachment) in p.attachments.iter().enumerate() {
                        let (chip, _) =
                            ui.allocate_exact_size(Vec2::new(160.0, 25.0), Sense::hover());
                        ui.painter().rect_filled(chip, 12, palette.window);
                        ui.painter().text(
                            egui::pos2(chip.right() - 10.0, chip.center().y),
                            Align2::CENTER_CENTER,
                            "×",
                            chrome::font(ui, 12.0),
                            palette.sub,
                        );
                        chrome::label(
                            ui,
                            Rect::from_min_max(
                                chip.min + Vec2::new(7.0, 0.0),
                                Pos2::new(chip.right() - 23.0, chip.bottom()),
                            ),
                            attachment.name,
                            10.0,
                            palette.sub,
                            Align2::LEFT_CENTER,
                        );
                        if ui
                            .interact(
                                Rect::from_min_size(
                                    egui::pos2(chip.right() - 21.0, chip.top()),
                                    Vec2::new(21.0, chip.height()),
                                ),
                                Id::new(("presence-remove-attachment", p.id, index)),
                                Sense::click(),
                            )
                            .on_hover_text("Remove attachment")
                            .clicked()
                        {
                            actions.push(PresenceAction::RemoveAttachment {
                                id: p.id.into(),
                                index,
                            });
                        }
                    }
                });
            });
        ui.add_space(4.0);
    }
    ui.horizontal_top(|ui| {
        ui.add_space(8.0);
        let edit = state.edits.entry(p.id.into()).or_default();
        let response = ui.add_sized(
            Vec2::new((width - if p.human { 74.0 } else { 104.0 }).max(60.0), 44.0),
            egui::TextEdit::multiline(&mut edit.message)
                .id(Id::new(("presence-composer", p.id)))
                .frame(false)
                .font(chrome::font(ui, if attached { 14.0 } else { 17.0 }))
                .hint_text(egui::RichText::new("Message").color(palette.sub))
                .desired_rows(2),
        );
        if response.changed() {
            edit.last_draft = edit.message.clone();
            actions.push(PresenceAction::Draft {
                id: p.id.into(),
                text: edit.message.clone(),
            });
        }
        if !p.human {
            let (attach, _) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::hover());
            let attach_hit = ui
                .interact(attach, Id::new(("presence-attach", p.id)), Sense::click())
                .on_hover_text("Attach an image");
            ui.painter()
                .circle_filled(attach.center(), 15.0, palette.window);
            ui.painter().text(
                attach.center(),
                Align2::CENTER_CENTER,
                "+",
                chrome::font(ui, 20.0),
                palette.ink,
            );
            if attach_hit.clicked() {
                actions.push(PresenceAction::Attach { id: p.id.into() });
            }
        }
        let (send, _) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::hover());
        let enabled = !edit.message.trim().is_empty() || !p.attachments.is_empty();
        ui.painter().circle_filled(
            send.center(),
            19.0,
            if enabled { palette.sub } else { palette.border },
        );
        let hit = ui
            .interact(send, Id::new(("presence-send", p.id)), Sense::click())
            .on_hover_text(if p.busy {
                "Send message while the task continues"
            } else if p.paused {
                "Send message"
            } else {
                "Send message · Enter"
            });
        chrome::draw_glyph(
            ui.painter(),
            send.shrink(9.0),
            chrome::Glyph::Send,
            palette.card,
        );
        let enter = response.has_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
        if enabled && (hit.clicked() || enter) {
            // The app clears its durable draft only after accepting this send.
            // A busy or unavailable harness must leave the user's text intact.
            let text = edit.message.trim().to_owned();
            actions.push(PresenceAction::Send {
                id: p.id.into(),
                text,
            });
            response.request_focus();
        }
    });
    controls::show(ui, palette, state, p, providers, header, actions);
}
#[allow(clippy::too_many_arguments)]
fn header_ui(
    ui: &mut Ui,
    palette: &Palette,
    painter: &mut PresencePainter,
    state: &mut RosterState,
    p: &Participant<'_>,
    providers: &[ProviderOption<'_>],
    now: f64,
    reduced: bool,
    header: Rect,
    attached: bool,
    actions: &mut Vec<PresenceAction>,
) {
    let right = header.right();
    let compact = header.width() < 420.0;
    let slot = if attached || compact { 26.0 } else { 31.0 };
    let name_width = ui
        .painter()
        .layout_no_wrap(p.name.into(), chrome::font(ui, 15.0), palette.sub)
        .size()
        .x
        .min(if p.human { 220.0 } else { 95.0 });
    let controls_reserve = if p.human {
        40.0
    } else if compact {
        232.0
    } else {
        267.0
    };
    let identity_end =
        (header.left() + 39.0 + name_width + 5.0).min(header.right() - controls_reserve);
    let capsule = Rect::from_min_max(
        header.min + Vec2::new(0.0, 4.0),
        Pos2::new(
            identity_end + if p.human { 0.0 } else { slot + 2.0 },
            header.bottom() - 4.0,
        ),
    );
    ui.painter().rect_filled(capsule, 20, palette.window);
    ui.painter().rect_stroke(
        capsule,
        20,
        egui::Stroke::new(0.7_f32, palette.border.gamma_multiply(0.5)),
        egui::StrokeKind::Inside,
    );
    let icon = Rect::from_min_size(header.min + Vec2::new(1.0, 7.0), Vec2::splat(33.0));
    chrome::participant_face(ui, painter, p, "conversation-header", icon, now, reduced);
    let identity = Rect::from_min_max(
        Pos2::new(icon.right() + 5.0, header.top() + 4.0),
        Pos2::new(identity_end, header.bottom() - 4.0),
    );
    chrome::label(ui, identity, p.name, 15.0, palette.sub, Align2::LEFT_CENTER);
    let profile_response = ui.interact(
        identity,
        Id::new(("conversation-profile", p.id)),
        if p.human {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if p.human && profile_response.on_hover_text("Profile").clicked() {
        state.toggle(Expansion::Profile(p.id.into()), state.anchor);
    }
    if p.human {
        chrome::label(
            ui,
            Rect::from_min_max(
                Pos2::new(identity_end, header.top()),
                Pos2::new(right - 40.0, header.bottom()),
            ),
            if p.local_profile {
                "Personal notes"
            } else {
                "Direct conversation"
            },
            11.0,
            palette.sub,
            Align2::RIGHT_CENTER,
        );
    } else {
        let mut x = identity_end;
        for (glyph, label) in [
            (chrome::Glyph::Locate, "Find on board"),
            (
                if p.paused {
                    chrome::Glyph::Play
                } else {
                    chrome::Glyph::Pause
                },
                if p.paused { "Resume" } else { "Pause" },
            ),
            (chrome::Glyph::Stop, "Stop task"),
        ] {
            let r = Rect::from_min_size(
                Pos2::new(x, header.center().y - slot * 0.5),
                Vec2::splat(slot),
            );
            if label != "Find on board" {
                let shadow = egui::epaint::Shadow {
                    offset: [0, 2],
                    blur: 6,
                    spread: 0,
                    color: palette.ink.gamma_multiply(0.08),
                };
                ui.painter()
                    .add(shadow.as_shape(r, egui::CornerRadius::same((slot / 2.0) as u8)));
                ui.painter()
                    .circle_filled(r.center(), slot * 0.5, palette.window);
            }
            if chrome::glyph_button(ui, Id::new((p.id, label)), r, glyph, label, palette).clicked()
            {
                actions.push(match label {
                    "Find on board" => PresenceAction::Locate { id: p.id.into() },
                    "Stop task" => PresenceAction::Cancel { id: p.id.into() },
                    _ => PresenceAction::Pause {
                        id: p.id.into(),
                        paused: !p.paused,
                    },
                });
            }
            x += slot + 3.0;
        }
        if !attached {
            let r = Rect::from_min_size(
                Pos2::new(x, header.center().y - 15.0),
                Vec2::new(34.0, 30.0),
            );
            hats::paint_icon(ui.painter(), p.hat, r, palette.sub);
            if ui
                .interact(r, Id::new(("hat-toggle", p.id)), Sense::click())
                .on_hover_text("Choose a hat")
                .clicked()
            {
                state.toggle_popover(Popover::Hats);
            }
            x += if compact { 38.0 } else { 43.0 };
            let provider = Rect::from_min_max(
                Pos2::new(x, header.top() + 6.0),
                Pos2::new(right - 64.0, header.bottom() - 6.0),
            );
            chrome::provider(
                ui,
                Rect::from_center_size(
                    Pos2::new(
                        if provider.width() < 65.0 {
                            provider.center().x
                        } else {
                            provider.left() + 9.0
                        },
                        provider.center().y,
                    ),
                    Vec2::splat(17.0),
                ),
                p.provider,
                palette.ink,
            );
            let label = providers
                .iter()
                .find(|o| o.id == p.provider)
                .map_or(p.provider, |o| o.label);
            if provider.width() > 65.0 {
                chrome::label(
                    ui,
                    Rect::from_min_max(
                        Pos2::new(provider.left() + 23.0, provider.top()),
                        Pos2::new(provider.right() - 12.0, provider.bottom()),
                    ),
                    label,
                    13.0,
                    palette.ink,
                    Align2::LEFT_CENTER,
                );
            }
            if provider.width() > 35.0 {
                chrome::chevron(
                    ui.painter(),
                    Pos2::new(provider.right() - 5.0, provider.center().y),
                    palette.sub,
                );
            }
            if p.queued_provider.is_some() {
                crate::icons::paint(
                    ui.painter(),
                    Rect::from_min_size(
                        Pos2::new(provider.right() - 14.0, provider.top() - 1.0),
                        Vec2::splat(8.0),
                    ),
                    crate::icons::Icon::Clock,
                    palette.sub,
                );
            }
            if ui
                .interact(provider, Id::new(("provider-toggle", p.id)), Sense::click())
                .on_hover_text("Harness and model")
                .clicked()
            {
                state.toggle_popover(Popover::Provider);
            }
        }
    }
    if !p.human && !attached {
        let more = Rect::from_center_size(
            Pos2::new(right - 47.0, header.center().y),
            Vec2::splat(24.0),
        );
        if chrome::glyph_button(
            ui,
            Id::new(("conversation-more", p.id)),
            more,
            chrome::Glyph::More,
            "Conversation options",
            palette,
        )
        .clicked()
        {
            state.toggle_popover(Popover::More);
        }
    }
    let close = Rect::from_center_size(
        Pos2::new(right - 15.0, header.center().y),
        Vec2::splat(26.0),
    );
    if chrome::glyph_button(
        ui,
        Id::new(("conversation-close", p.id)),
        close,
        chrome::Glyph::Close,
        "Close conversation",
        palette,
    )
    .clicked()
    {
        state.close_chat();
    }
}
