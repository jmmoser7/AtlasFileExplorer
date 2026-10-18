//! Bounded transcript presentation. All stored turns remain reachable in explicit pages.
use super::{model::*, state::RosterState, ui as chrome, PresencePainter};
use crate::theme::Palette;
use eframe::egui::{self, Align2, Id, Pos2, Rect, RichText, Sense, Ui, Vec2};

pub(super) fn history_window(
    total: usize,
    offset: usize,
    attached: bool,
) -> std::ops::Range<usize> {
    if attached {
        return total.saturating_sub(4)..total;
    }
    let offset = offset.min(total.saturating_sub(1) / 64 * 64);
    let end = total.saturating_sub(offset);
    end.saturating_sub(64)..end
}

#[cfg(test)]
mod tests {
    use super::history_window;
    #[test]
    fn every_stored_turn_is_reachable_with_at_most_sixty_four_painted_rows() {
        let total = 513;
        let mut seen = vec![false; total];
        for offset in (0..total).step_by(64) {
            let page = history_window(total, offset, false);
            assert!(page.len() <= 64);
            for index in page {
                assert!(!seen[index]);
                seen[index] = true;
            }
        }
        assert!(seen.into_iter().all(|turn| turn));
        assert_eq!(history_window(100_000, 0, true), 99_996..100_000);
        assert_eq!(history_window(0, 0, false), 0..0);
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn show(
    ui: &mut Ui,
    palette: &Palette,
    painter: &mut PresencePainter,
    state: &mut RosterState,
    p: &Participant<'_>,
    now: f64,
    reduced: bool,
    attached: bool,
    message_height: f32,
    actions: &mut Vec<PresenceAction>,
) {
    let total = p.message_total;
    let mut offset = *state.history_offsets.entry(p.id.into()).or_insert(0);
    let paged = !attached && total > 64;
    if paged {
        ui.horizontal(|ui| {
            if ui
                .add_enabled(offset + 64 < total, egui::Button::new("← Earlier").small())
                .clicked()
            {
                offset += 64;
            }
            if ui
                .add_enabled(offset > 0, egui::Button::new("Newer →").small())
                .clicked()
            {
                offset = offset.saturating_sub(64);
            }
            if offset > 0 && ui.small_button("Latest").clicked() {
                offset = 0;
            }
            ui.label(
                RichText::new(format!("{} messages", total))
                    .size(10.0)
                    .color(palette.sub),
            );
        });
        state.history_offsets.insert(p.id.into(), offset);
    }
    let message_height = (message_height - if paged { 24.0 } else { 0.0 }).max(30.0);
    egui::ScrollArea::vertical()
        .id_salt(("participant-messages", p.id, attached))
        .max_height(message_height)
        .min_scrolled_height(message_height)
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_min_height(message_height);
            // The approved transcript gutter is separate from the enclosing shell.
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(if attached { 14 } else { 16 }, 0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(16.0);
                    if p.messages.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(22.0);
                            ui.label(
                                RichText::new(if p.local_profile {
                                    "A conversation with yourself"
                                } else {
                                    "Start a conversation"
                                })
                                .size(14.0)
                                .color(palette.sub),
                            );
                        });
                    }
                    for (index, message) in p.messages.iter().enumerate() {
                        if message.system {
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(message.text).size(10.0).color(palette.sub));
                            });
                        } else {
                            message_row(
                                ui, palette, painter, p, message, now, reduced, attached, actions,
                            );
                        }
                        if index + 1 < p.messages.len() {
                            ui.add_space(if attached { 16.0 } else { 23.0 });
                        }
                    }
                    ui.add_space(if attached { 14.0 } else { 20.0 });
                });
        });
}
#[allow(clippy::too_many_arguments)]
fn message_row(
    ui: &mut Ui,
    palette: &Palette,
    painter: &mut PresencePainter,
    p: &Participant<'_>,
    message: &ConversationMessage<'_>,
    now: f64,
    reduced: bool,
    attached: bool,
    actions: &mut Vec<PresenceAction>,
) {
    let width = ui.available_width();
    let avatar = if attached { 25.0 } else { 36.0 };
    let font = if attached { 14.0 } else { 18.0 };
    let padding = if attached {
        Vec2::new(13.0, 10.0)
    } else {
        Vec2::new(18.0, 14.0)
    };
    let bubble_width = if message.outgoing {
        width * if attached { 0.85 } else { 0.83 }
    } else if attached {
        width - 35.0
    } else {
        width - avatar - 28.0
    };
    let mut job = egui::text::LayoutJob::simple(
        message.text.into(),
        chrome::font(ui, font),
        if message.outgoing {
            palette.ink
        } else {
            palette.sub
        },
        (bubble_width - padding.x * 2.0).max(25.0),
    );
    job.sections[0].format.line_height = Some(font * if attached { 1.45 } else { 1.35 });
    let galley = ui.painter().layout_job(job);
    let size = galley.size() + padding * 2.0;
    let (row, _) = ui.allocate_exact_size(Vec2::new(width, size.y.max(avatar)), Sense::hover());
    let left = if message.outgoing {
        row.right() - size.x
    } else {
        row.left() + avatar + if attached { 9.0 } else { 15.0 }
    };
    let rect = Rect::from_min_size(Pos2::new(left, row.top()), size);
    chrome::surface(
        ui.painter(),
        rect,
        if attached { 17 } else { 24 },
        palette,
        true,
    );
    ui.painter().galley(
        rect.min + padding,
        galley,
        if message.outgoing {
            palette.ink
        } else {
            palette.sub
        },
    );
    if !message.outgoing {
        chrome::participant_face(
            ui,
            painter,
            p,
            &format!("message-{}", message.id),
            Rect::from_min_size(row.min + Vec2::new(0.0, 5.0), Vec2::splat(avatar)),
            now,
            reduced,
        );
    }
    if message.outgoing {
        let status = match message.status {
            "pending" => Some("Saving…"),
            "accepted" => Some("Awaiting receipt"),
            "delivered" => Some("Delivered"),
            "failed" => Some("Not delivered · retry"),
            "stopped" => Some("Stopped"),
            _ => None,
        };
        if let Some(status) = status {
            let (status_rect, _) = ui.allocate_exact_size(Vec2::new(width, 15.0), Sense::hover());
            chrome::label(
                ui,
                status_rect.shrink2(Vec2::new(8.0, 0.0)),
                status,
                10.0,
                palette.sub,
                Align2::RIGHT_CENTER,
            );
            if message.status == "failed"
                && ui
                    .interact(
                        Rect::from_min_max(
                            Pos2::new(status_rect.right() - 130.0, status_rect.top()),
                            status_rect.max,
                        ),
                        Id::new(("presence-retry", p.id, message.id)),
                        Sense::click(),
                    )
                    .on_hover_text("Retry the same message")
                    .clicked()
            {
                actions.push(PresenceAction::Retry {
                    id: p.id.into(),
                    message: message.id.into(),
                });
            }
        }
    }
}
