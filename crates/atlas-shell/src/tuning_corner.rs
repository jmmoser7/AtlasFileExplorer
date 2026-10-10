//! Tuner section for the bottom toasts and the suggestion-box button.

use crate::tokens::UiTokens;
use crate::tuning::enabled::scalar;
use eframe::egui::{self, RichText};
use std::sync::atomic::Ordering;

pub(crate) fn editor(ui: &mut egui::Ui, draft: &mut UiTokens) {
    let UiTokens {
        toast,
        readouts,
        dock,
        ..
    } = draft;
    egui::CollapsingHeader::new("Bottom toasts · Suggestion box")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                RichText::new(
                    "Toasts stack upward from just above the bottom tool palette. The \
                     suggestion box sits right of the readout chevron on the palette's \
                     baseline.",
                )
                .small(),
            );
            ui.add_space(4.0);
            let mut locked = crate::toast::TUNER_PREVIEW.load(Ordering::Relaxed);
            if ui
                .checkbox(&mut locked, "Lock sample toasts open")
                .changed()
            {
                crate::toast::TUNER_PREVIEW.store(locked, Ordering::Relaxed);
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Toast geometry").strong());
            scalar(ui, "Max width (px)", &mut toast.max_width_px, 200.0..=900.0);
            scalar(
                ui,
                "Max width (fraction of window)",
                &mut toast.max_width_fraction,
                0.35..=0.9,
            );
            scalar(
                ui,
                "Min wrap width (px)",
                &mut toast.min_width_px,
                160.0..=560.0,
            );
            scalar(
                ui,
                "Gap above palette (px)",
                &mut toast.above_palette_gap,
                4.0..=48.0,
            );
            scalar(
                ui,
                "Gap between toasts (px)",
                &mut toast.stack_gap,
                2.0..=24.0,
            );
            scalar(ui, "Pad X (px)", &mut toast.pad_x, 6.0..=32.0);
            scalar(ui, "Pad Y (px)", &mut toast.pad_y, 4.0..=24.0);
            scalar(
                ui,
                "Corner radius (px)",
                &mut toast.corner_radius,
                4.0..=16.0,
            );
            ui.label(RichText::new("Toast typography").strong());
            scalar(ui, "Text size (pt)", &mut toast.text_size, 10.0..=18.0);
            ui.add_space(6.0);
            ui.label(RichText::new("Suggestion box").strong());
            ui.label(
                RichText::new(format!("0 = palette icon size ({:.0} px).", dock.icon_size)).small(),
            );
            scalar(ui, "Size (px)", &mut readouts.suggestion_size, 0.0..=64.0);
            scalar(
                ui,
                "Gap after chevron (px)",
                &mut readouts.suggestion_gap_from_chevron,
                2.0..=32.0,
            );
            scalar(
                ui,
                "Inset from corner (px)",
                &mut readouts.suggestion_inset_x,
                0.0..=48.0,
            );
        });
}
