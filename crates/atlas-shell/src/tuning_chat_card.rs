//! Tuner section for the Slate chat-card header.

use crate::tokens::{self, ChatCardTokens};
use crate::tuning::enabled::scalar;
use eframe::egui::{self, RichText};

pub(crate) fn editor(ui: &mut egui::Ui, card: &mut ChatCardTokens) {
    egui::CollapsingHeader::new("Chat card · Header")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Board world units. Paint multiplies them by the camera zoom. \
                     The title's x-height offset is measured down from the output dot's top.",
                )
                .small(),
            );
            ui.add_space(4.0);
            let mut locked = tokens::chat_header_preview_locked();
            if ui
                .checkbox(&mut locked, "Lock chevron preview open")
                .changed()
            {
                tokens::set_chat_header_preview_locked(locked);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Typography").strong());
            scalar(
                ui,
                "Title x-height below dot top",
                &mut card.title_x_height_offset,
                -24.0..=24.0,
            );
            scalar(ui, "Title size", &mut card.title_size, 8.0..=28.0);
            ui.add_space(6.0);
            ui.label(RichText::new("Geometry").strong());
            scalar(ui, "Dot diameter", &mut card.dot_diameter, 2.0..=16.0);
            scalar(ui, "Dot inset from edges", &mut card.dot_inset, 0.0..=32.0);
            scalar(ui, "Dot top offset", &mut card.dot_top, 0.0..=48.0);
            scalar(ui, "Chevron size", &mut card.chevron_size, 0.8..=8.0);
            scalar(ui, "Chevron stroke", &mut card.chevron_stroke, 0.3..=2.4);
            scalar(
                ui,
                "Chevron X (left of •••)",
                &mut card.chevron_x,
                0.0..=40.0,
            );
            scalar(
                ui,
                "••• dot diameter",
                &mut card.menu_dot_diameter,
                1.0..=12.0,
            );
            scalar(
                ui,
                "••• dot spacing",
                &mut card.menu_dot_spacing,
                2.0..=24.0,
            );
            ui.add_space(6.0);
            ui.label(RichText::new("Effects").strong());
            scalar(
                ui,
                "Chevron cross-fade (ms)",
                &mut card.chevron_fade_ms,
                0.0..=800.0,
            );
        });
}
