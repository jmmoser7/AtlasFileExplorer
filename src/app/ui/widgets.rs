//! Shared egui widgets for toolbars and readouts.

use super::sidebar::{
    apply_sidebar_slider_style, sidebar_slider_rail_stroke, SidebarSliderStyle, SidebarTheme,
};
use crate::app::DateSliderMode;
use crate::types::{date_string, day_start};
use eframe::egui::{self, popup, Color32, CornerRadius, Id, PopupCloseBehavior, Sense, Ui, Vec2};

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

/// Sidebar numeric slider — same rail/handle scale as the date timeline.
/// Label row sits directly above the rail. Right-click to edit min/max domain.
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

/// Folder-span date timeline with optional single-day or range handles.
pub fn sidebar_date_timeline(
    ui: &mut Ui,
    id: Id,
    span_min: i64,
    span_max: i64,
    mode: &mut DateSliderMode,
    single_day: &mut i64,
    range_lo: &mut i64,
    range_hi: &mut i64,
    filter_engaged: &mut bool,
    theme: SidebarTheme,
) -> bool {
    if span_max < span_min {
        return false;
    }
    let mut changed = false;
    let span = (span_max - span_min).max(0);

    ui.horizontal(|ui| {
        ui.set_min_height(22.0);
        let toggle_label = if *mode == DateSliderMode::Range {
            "−"
        } else {
            "+"
        };
        if ui
            .small_button(toggle_label)
            .on_hover_text(if *mode == DateSliderMode::Range {
                "Switch to a single-day picker"
            } else {
                "Add a second handle to filter a date range"
            })
            .clicked()
        {
            changed = true;
            match *mode {
                DateSliderMode::SingleDay => {
                    *mode = DateSliderMode::Range;
                    *range_lo = (*single_day).clamp(span_min, span_max);
                    *range_hi = span_max;
                }
                DateSliderMode::Range => {
                    *mode = DateSliderMode::SingleDay;
                    *single_day = (*range_lo).clamp(span_min, span_max);
                    *filter_engaged = true;
                }
            }
        }

        let track_w = ui.available_width().max(40.0);
        let (track_rect, _track) = ui.allocate_exact_size(Vec2::new(track_w, 22.0), Sense::hover());
        let rail = egui::Rect::from_center_size(
            track_rect.center(),
            Vec2::new(track_rect.width(), SidebarSliderStyle::RAIL_HEIGHT),
        );
        let stroke = sidebar_slider_rail_stroke(theme);
        let painter = ui.painter();

        if span > 0 && *mode == DateSliderMode::Range {
            let x0 = day_to_x(*range_lo, rail, span_min, span_max);
            let x1 = day_to_x(*range_hi, rail, span_min, span_max);
            painter.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(x0.min(x1), rail.top()),
                    egui::pos2(x0.max(x1), rail.bottom()),
                ),
                0.0,
                theme.ink.gamma_multiply(0.18),
            );
        }

        painter.hline(rail.x_range(), rail.center().y, stroke);

        let mut handles: Vec<i64> = match *mode {
            DateSliderMode::SingleDay => vec![*single_day],
            DateSliderMode::Range => vec![*range_lo, *range_hi],
        };

        for (i, day) in handles.iter_mut().enumerate() {
            let handle_id = id.with("handle").with(i);
            let x = day_to_x(*day, rail, span_min, span_max);
            let handle_center = egui::pos2(x, rail.center().y);
            let handle_rect = egui::Rect::from_center_size(
                handle_center,
                Vec2::splat(SidebarSliderStyle::HANDLE_RADIUS * 2.0),
            );
            let resp = ui.interact(handle_rect, handle_id, Sense::drag());
            if resp.dragged() {
                let pointer = ui
                    .input(|inp| inp.pointer.latest_pos())
                    .unwrap_or(handle_center);
                *day = x_to_day(pointer.x, rail, span_min, span_max);
                changed = true;
                *filter_engaged = true;
            }
            painter.circle_filled(
                handle_center,
                SidebarSliderStyle::HANDLE_RADIUS,
                if resp.hovered() || resp.dragged() {
                    theme.ink
                } else {
                    theme.sub
                },
            );
        }

        match *mode {
            DateSliderMode::SingleDay => {
                *single_day = handles[0].clamp(span_min, span_max);
            }
            DateSliderMode::Range => {
                *range_lo = handles[0].clamp(span_min, span_max);
                *range_hi = handles[1].clamp(span_min, span_max);
                if *range_lo > *range_hi {
                    std::mem::swap(range_lo, range_hi);
                }
            }
        }
    });

    let caption = match *mode {
        DateSliderMode::SingleDay => date_string(day_start(*single_day)),
        DateSliderMode::Range => {
            if *range_lo == *range_hi {
                date_string(day_start(*range_lo))
            } else {
                format!(
                    "{} — {}",
                    date_string(day_start(*range_lo)),
                    date_string(day_start(*range_hi))
                )
            }
        }
    };
    ui.label(egui::RichText::new(caption).small().color(theme.sub));
    ui.add_space(SidebarSliderStyle::LABEL_GAP);
    changed
}

fn day_to_x(day: i64, rail: egui::Rect, span_min: i64, span_max: i64) -> f32 {
    let span = (span_max - span_min).max(1) as f32;
    rail.left() + ((day - span_min) as f32 / span) * rail.width()
}

fn x_to_day(x: f32, rail: egui::Rect, span_min: i64, span_max: i64) -> i64 {
    if rail.width() <= 0.0 {
        return span_min;
    }
    let t = ((x - rail.left()) / rail.width()).clamp(0.0, 1.0);
    let span = (span_max - span_min).max(0);
    span_min + (t * span as f32).round() as i64
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
