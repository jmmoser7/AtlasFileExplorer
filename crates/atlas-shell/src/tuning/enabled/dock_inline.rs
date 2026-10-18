//! Presentation controls for the shared dock's form and inline Tool hosts.
use super::{egui, rgba, scalar, DockTokens, RichText};

pub(super) fn show(ui: &mut egui::Ui, dock: &mut DockTokens) {
    ui.separator();
    ui.label(RichText::new("Primary ↔ secondary fill").strong());
    ui.label(
        RichText::new(
            "Link the primary dock plates to the secondary \
             capsule, then offset them. Mix 0 keeps the absolute \
             primary fill; 1 derives it from the capsule. \
             Offset: positive lightens in dark / darkens in light.",
        )
        .small(),
    );
    scalar(
        ui,
        "Primary from secondary",
        &mut dock.palette.primary_fill_mix,
        0.0..=1.0,
    );
    scalar(
        ui,
        "Primary fill offset",
        &mut dock.palette.primary_fill_offset,
        -0.6..=0.6,
    );
    rgba(ui, "Primary fill · dark", &mut dock.dark.icon_fill);
    rgba(ui, "Primary fill · light", &mut dock.light.icon_fill);

    ui.separator();
    ui.label(RichText::new("Bottom tool capsules").strong());
    scalar(
        ui,
        "Expansion / collapse duration",
        &mut dock.inline.open_duration,
        0.05..=0.8,
    );
    rgba(
        ui,
        "Primary fill · light mode",
        &mut dock.inline.primary_light_fill,
    );
    rgba(
        ui,
        "Primary glyph · light mode",
        &mut dock.inline.primary_light_ink,
    );
    rgba(
        ui,
        "Primary fill · dark mode",
        &mut dock.inline.primary_dark_fill,
    );
    rgba(
        ui,
        "Primary glyph · dark mode",
        &mut dock.inline.primary_dark_ink,
    );
    rgba(ui, "Feedback cherry", &mut dock.inline.feedback_fill);
    rgba(ui, "Feedback glyph", &mut dock.inline.feedback_ink);
}

pub(super) fn feedback(ui: &mut egui::Ui, t: &mut crate::tokens::FeedbackTokens) {
    egui::CollapsingHeader::new("Feedback windows").show(ui, |ui| {
        scalar(ui, "Width", &mut t.width, 320.0..=720.0);
        scalar(ui, "Content inset", &mut t.inset, 12.0..=32.0);
        ui.add(egui::Slider::new(&mut t.radius, 0..=24).text("Corner radius"));
        scalar(ui, "Canvas buffer", &mut t.canvas_buffer, 0.0..=32.0);
        scalar(ui, "Section gap", &mut t.section_gap, 8.0..=32.0);
        scalar(ui, "Label gap", &mut t.label_gap, 2.0..=16.0);
        scalar(
            ui,
            "Description height",
            &mut t.description_height,
            64.0..=240.0,
        );
        scalar(ui, "Links height", &mut t.links_height, 36.0..=120.0);
        rgba(ui, "Panel · light", &mut t.panel_light);
        rgba(ui, "Panel · dark", &mut t.panel_dark);
        rgba(ui, "Field · light", &mut t.field_light);
        rgba(ui, "Field · dark", &mut t.field_dark);
    });
}
