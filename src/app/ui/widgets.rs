//! Shared egui widgets for toolbars and readouts.

use super::sidebar::{apply_sidebar_slider_style, SidebarSliderStyle};
use eframe::egui::{self, popup, Color32, CornerRadius, Id, PopupCloseBehavior, Sense, Ui};

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

/// Sidebar numeric slider — unified rail/handle sizing; label row sits directly above the rail.
/// Right-click opens a popup to edit the slider domain (min/max).
pub fn thin_sidebar_slider(
    ui: &mut Ui,
    id: Id,
    value: &mut usize,
    range: &mut std::ops::RangeInclusive<usize>,
    label: &str,
    unit: &str,
    hover: &str,
    sub_color: Color32,
) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small().color(sub_color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{} {}", *value, unit))
                    .small()
                    .color(sub_color),
            );
        });
    });
    ui.add_space(SidebarSliderStyle::LABEL_GAP);

    let slider_resp = ui
        .scope(|ui| {
            apply_sidebar_slider_style(ui);
            ui.add(
                egui::Slider::new(value, range.clone())
                    .show_value(false)
                    .clamping(egui::SliderClamping::Always),
            )
        })
        .inner
        .on_hover_text(hover);

    if slider_resp.secondary_clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(id.with("domain")));
    }

    popup::popup_below_widget(
        ui,
        id.with("domain"),
        &slider_resp,
        PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(160.0);
            ui.label(egui::RichText::new("Slider range").small().strong());
            ui.label(
                egui::RichText::new("Right-click any display slider to adjust limits.")
                    .small()
                    .color(sub_color),
            );
            let mut min_v = *range.start();
            let mut max_v = *range.end();
            ui.horizontal(|ui| {
                ui.label("min");
                ui.add(egui::DragValue::new(&mut min_v).speed(1));
                ui.label("max");
                ui.add(egui::DragValue::new(&mut max_v).speed(1));
            });
            if ui.button("Apply").clicked() {
                if min_v <= max_v {
                    *range = min_v..=max_v;
                    *value = value.clamp(min_v, max_v);
                }
                ui.memory_mut(|mem| mem.close_popup());
            }
        },
    );

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
    let icon = egui::RichText::new("⚙").size(8.0);
    ui.menu_button(icon, build)
        .response
        .on_hover_text("Choose visible panels");
}
