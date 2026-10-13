//! Images in a shared feedback form; clipboard/capture stay in FeedbackHub.
use super::FeedbackHub;
use crate::{
    icons::{self, Icon},
    theme::Palette,
};
use eframe::egui::{self, Margin, RichText, Stroke, Vec2};

pub(super) fn show(ui: &mut egui::Ui, p: &Palette, hub: &mut FeedbackHub, ctx: &egui::Context) {
    let content_width = (ui.available_width() - 28.0).max(0.0);
    egui::Frame::new()
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(6)
        .inner_margin(Margin::same(13))
        .show(ui, |ui| {
            ui.set_min_width(content_width);
            ui.label(
                RichText::new("Drop an image here or paste from clipboard.")
                    .size(12.0)
                    .color(p.sub),
            );
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let waiting = hub.screenshot_pending || hub.capture_rx.is_some();
                if ui
                    .add_enabled(
                        !waiting,
                        egui::Button::new(if waiting {
                            "Capturing…"
                        } else {
                            "Capture window"
                        }),
                    )
                    .clicked()
                {
                    hub.request_screenshot(ctx);
                }
                if ui.button("Paste image").clicked() {
                    hub.attach_clipboard_image();
                }
            });
        });
    let paste_key = ui.ui_contains_pointer()
        && ctx.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))
                || (i.modifiers.command && i.key_pressed(egui::Key::V))
        })
        && !ctx.wants_keyboard_input();
    if paste_key {
        hub.attach_clipboard_image();
    }
    let dropped = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .collect::<Vec<_>>()
    });
    if !dropped.is_empty() {
        hub.attach_dropped(&dropped);
    }
    let mut remove = None;
    for (i, (path, _)) in hub.attachments.iter().enumerate() {
        ui.add_space(9.0);
        egui::Frame::new()
            .fill(p.border.gamma_multiply(0.20))
            .corner_radius(6)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(32.0, 32.0), egui::Sense::hover());
                    icons::paint(ui.painter(), rect.shrink(7.0), Icon::Image, p.sub);
                    let remaining = (ui.available_width() - 32.0).max(20.0);
                    ui.allocate_ui(Vec2::new(remaining, 32.0), |ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new(
                                    path.file_name().unwrap_or_default().to_string_lossy(),
                                )
                                .size(12.0),
                            )
                            .wrap(),
                        );
                    });
                    if ui.small_button("×").on_hover_text("Remove image").clicked() {
                        remove = Some(i);
                    }
                });
            });
    }
    if let Some(i) = remove {
        hub.attachments.remove(i);
    }
    if let Some(note) = &hub.attach_note {
        ui.add_space(8.0);
        ui.label(RichText::new(note).size(12.0).color(p.sub));
    }
}
