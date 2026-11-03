//! Read-only public carried dialogue; apps supply only authorized message text.
use super::*;

pub fn carried_history_dialog(
    ctx: &egui::Context,
    id: Id,
    turns: &[(&str, &str)],
    dark: bool,
) -> bool {
    let ink = if dark {
        Color32::from_rgb(154, 166, 181)
    } else {
        Color32::from_rgb(91, 101, 114)
    };
    let line = if dark {
        Color32::from_rgb(46, 54, 65)
    } else {
        Color32::from_rgb(213, 219, 225)
    };
    let background = if dark {
        Color32::from_rgb(21, 26, 33)
    } else {
        Color32::from_rgb(237, 240, 243)
    };
    let mut close = false;
    let modal = egui::Modal::new(id)
        .backdrop_color(Color32::from_black_alpha(102))
        .frame(
            egui::Frame::NONE
                .fill(background)
                .stroke(Stroke::new(1.0_f32, line))
                .corner_radius(8)
                .inner_margin(22),
        )
        .show(ctx, |ui| {
            ui.set_width(354.0);
            let family = egui::FontFamily::Name("slate-segoe".into());
            let family = if ctx.fonts(|f| f.families().contains(&family)) {
                family
            } else {
                egui::FontFamily::Proportional
            };
            ui.style_mut().override_font_id = Some(egui::FontId::new(12.0, family.clone()));
            ui.visuals_mut().override_text_color = Some(ink);
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            ui.label(
                egui::RichText::new("Carried conversation history")
                    .font(egui::FontId::new(14.0, family)),
            );
            ui.add_space(12.0);
            egui::ScrollArea::vertical()
                .max_height((ctx.screen_rect().height() - 180.0).max(80.0))
                .show(ui, |ui| {
                    if turns.is_empty() {
                        ui.label("No messages carried yet.");
                    }
                    for (role, text) in turns {
                        ui.add_space(12.0);
                        ui.label(format!(
                            "{}: {text}",
                            if *role == "user" { "You" } else { "Agent" }
                        ));
                    }
                });
            ui.add_space(12.0);
            close = ui
                .add(
                    egui::Button::new("Close")
                        .min_size(Vec2::new(0.0, 30.0))
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.0_f32, line))
                        .corner_radius(4),
                )
                .clicked();
        });
    close || modal.should_close()
}
