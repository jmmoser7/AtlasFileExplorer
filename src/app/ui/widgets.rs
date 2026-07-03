//! Shared egui widgets for toolbars and readouts.

use eframe::egui::{self, Color32, CornerRadius, Sense, Ui};

pub fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let cut: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

pub fn chip(ui: &mut Ui, text: &str, active: bool, base: Color32) -> egui::Response {
    let fill = if active {
        base
    } else {
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), 90)
    };
    let btn = egui::Button::new(egui::RichText::new(text).size(11.0).color(Color32::WHITE))
        .fill(fill)
        .corner_radius(CornerRadius::same(10))
        .sense(Sense::click_and_drag());
    ui.add(btn)
}

pub fn thin_sidebar_slider(
    ui: &mut Ui,
    value: &mut usize,
    range: std::ops::RangeInclusive<usize>,
    label: &str,
    unit: &str,
    hover: &str,
) -> bool {
    let before = *value;
    ui.scope(|ui| {
        let width = ui.available_width();
        ui.spacing_mut().slider_width = width;
        ui.spacing_mut().slider_rail_height = 2.5;
        ui.spacing_mut().interact_size.y = 6.0;
        ui.add(
            egui::Slider::new(value, range)
                .show_value(false)
                .clamping(egui::SliderClamping::Always),
        )
    })
    .inner
    .on_hover_text(hover);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .small()
                .color(Color32::from_gray(120)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{} {}", *value, unit))
                    .small()
                    .color(Color32::from_gray(120)),
            );
        });
    });
    *value != before
}

pub fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}

/// Upper-left gear: opens a menu of optional sub-panels.
pub fn gear_menu<F>(ui: &mut Ui, _id: &str, build: F)
where
    F: FnOnce(&mut Ui),
{
    ui.menu_button("⚙", build)
        .response
        .on_hover_text("Choose visible panels");
}
