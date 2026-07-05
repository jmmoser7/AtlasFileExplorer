//! Shared egui widgets for toolbars and readouts.

use crate::sidebar::SidebarTheme;
use atlas_core::types::{
    snap_to_step, timeline_range_caption, timeline_tick_label, SECS_PER_DAY, SECS_PER_HOUR,
};
use eframe::egui::{
    self, Color32, CornerRadius, FontId, Id, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2,
};

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
    sub_color: Color32,
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
        ui.label(egui::RichText::new(label).small().color(sub_color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("{} {}", *value, unit))
                    .small()
                    .color(sub_color),
            );
        });
    });
    *value != before
}

#[derive(Clone, Copy)]
struct TimelineView {
    view_lo: f64,
    view_hi: f64,
    span_lo: i64,
    span_hi: i64,
}

/// After Effects–style zoomable folder timeline with dual handles and dynamic scale.
pub fn sidebar_date_timeline(
    ui: &mut Ui,
    id: Id,
    span_lo: i64,
    span_hi: i64,
    range_lo: &mut i64,
    range_hi: &mut i64,
    theme: SidebarTheme,
) -> bool {
    if span_hi <= span_lo {
        return false;
    }

    let mut changed = false;
    let span_secs = (span_hi - span_lo).max(1) as f64;
    let view_id = id.with("view");

    let mut view = ui
        .data(|d| d.get_temp::<TimelineView>(view_id))
        .unwrap_or(TimelineView {
            view_lo: span_lo as f64,
            view_hi: span_hi as f64,
            span_lo,
            span_hi,
        });
    if view.span_lo != span_lo || view.span_hi != span_hi {
        view = TimelineView {
            view_lo: span_lo as f64,
            view_hi: span_hi as f64,
            span_lo,
            span_hi,
        };
    }

    let track_h = 20.0;
    let scale_h = 26.0;
    let total_h = track_h + scale_h;
    let width = ui.available_width().max(48.0);
    let (block, block_resp) = ui.allocate_exact_size(Vec2::new(width, total_h), Sense::hover());
    let block_resp = block_resp.on_hover_text(
        "Scroll to zoom · drag background to pan · double-click to fit · drag handles to filter",
    );

    let visible = (view.view_hi - view.view_lo).max(SECS_PER_HOUR as f64 / 4.0);
    let snap_secs = snap_unit(visible);
    let tick_step = tick_step_secs(visible, block.width());

    // --- zoom (scroll wheel) ---
    if block_resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y + i.raw_scroll_delta.y);
        if scroll.abs() > 0.0 {
            let pointer = ui
                .input(|i| i.pointer.hover_pos())
                .unwrap_or(block.center());
            let anchor_t = time_at_x(pointer.x, block, view.view_lo, view.view_hi);
            let factor = 1.15_f64.powf(-scroll as f64 * 0.05);
            let mut new_w = visible * factor;
            let min_w = SECS_PER_HOUR as f64;
            let max_w = span_secs;
            new_w = new_w.clamp(min_w, max_w);
            let rel = if visible > 0.0 {
                (anchor_t - view.view_lo) / visible
            } else {
                0.5
            };
            view.view_lo = anchor_t - rel * new_w;
            view.view_hi = view.view_lo + new_w;
            clamp_view(&mut view, span_lo, span_hi);
            changed = true;
        }
    }

    if block_resp.double_clicked() {
        view.view_lo = span_lo as f64;
        view.view_hi = span_hi as f64;
        changed = true;
    }

    let rail = Rect::from_min_max(
        Pos2::new(block.left(), block.top() + 6.0),
        Pos2::new(block.right(), block.top() + track_h - 4.0),
    );
    let rail_mid = (rail.top() + rail.bottom()) * 0.5;

    // --- pan (drag rail or scale strip; handles take priority when overlapping) ---
    let scale_rect =
        Rect::from_min_max(Pos2::new(block.left(), block.bottom() - scale_h), block.max);
    for (pan_id, rect) in [
        (id.with("rail_pan"), rail),
        (id.with("scale_pan"), scale_rect),
    ] {
        let pan = ui.interact(rect, pan_id, Sense::drag());
        if pan.dragged() {
            let delta = ui.input(|i| i.pointer.delta().x);
            if delta.abs() > 0.0 {
                let dt = (delta as f64 / block.width() as f64) * visible;
                view.view_lo -= dt;
                view.view_hi -= dt;
                clamp_view(&mut view, span_lo, span_hi);
                changed = true;
            }
        }
    }

    let stroke = Stroke::new(1.5, theme.border.gamma_multiply(0.9));
    let painter = ui.painter_at(block);

    // selection fill
    let x0 = time_to_x(*range_lo as f64, block, view.view_lo, view.view_hi);
    let x1 = time_to_x(*range_hi as f64, block, view.view_lo, view.view_hi);
    if x1 > x0 {
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(x0, rail.top()), Pos2::new(x1, rail.bottom())),
            0.0,
            theme.ink.gamma_multiply(0.18),
        );
    }
    painter.hline(rail.x_range(), rail_mid, stroke);

    // --- handles ---
    let mut handles = [*range_lo, *range_hi];
    for (i, handle) in handles.iter_mut().enumerate() {
        let handle_id = id.with("handle").with(i);
        let x = time_to_x(*handle as f64, block, view.view_lo, view.view_hi);
        let center = Pos2::new(x, rail_mid);
        let handle_rect = Rect::from_center_size(center, Vec2::splat(12.0));
        let resp = ui.interact(handle_rect, handle_id, Sense::drag());
        if resp.dragged() {
            let pointer = ui.input(|inp| inp.pointer.latest_pos()).unwrap_or(center);
            let t = time_at_x(pointer.x, block, view.view_lo, view.view_hi);
            *handle = snap_to_step(t.round() as i64, snap_secs);
            changed = true;
        }
        painter.circle_filled(
            center,
            4.5,
            if resp.hovered() || resp.dragged() {
                theme.ink
            } else {
                theme.sub
            },
        );
    }

    *range_lo = handles[0].clamp(span_lo, span_hi);
    *range_hi = handles[1].clamp(span_lo, span_hi);
    if *range_lo > *range_hi {
        std::mem::swap(range_lo, range_hi);
    }

    // --- scale ---
    draw_timeline_scale(
        &painter,
        block,
        scale_h,
        view.view_lo,
        view.view_hi,
        tick_step,
        theme,
    );

    ui.ctx().data_mut(|d| d.insert_temp(view_id, view));

    let caption = timeline_range_caption(*range_lo, *range_hi, snap_secs);
    ui.label(RichText::new(caption).small().color(theme.sub));
    ui.add_space(4.0);
    changed
}

fn clamp_view(view: &mut TimelineView, span_lo: i64, span_hi: i64) {
    let span_w = (span_hi - span_lo).max(1) as f64;
    let w = (view.view_hi - view.view_lo).clamp(SECS_PER_HOUR as f64, span_w);
    if w >= span_w {
        view.view_lo = span_lo as f64;
        view.view_hi = span_hi as f64;
        return;
    }
    if view.view_lo < span_lo as f64 {
        view.view_lo = span_lo as f64;
        view.view_hi = view.view_lo + w;
    }
    if view.view_hi > span_hi as f64 {
        view.view_hi = span_hi as f64;
        view.view_lo = view.view_hi - w;
    }
    view.view_lo = view.view_lo.max(span_lo as f64);
    view.view_hi = view.view_hi.min(span_hi as f64);
}

fn snap_unit(visible_secs: f64) -> i64 {
    if visible_secs <= 2.0 * SECS_PER_HOUR as f64 {
        900
    } else if visible_secs <= 36.0 * SECS_PER_HOUR as f64 {
        SECS_PER_HOUR
    } else if visible_secs <= 21.0 * SECS_PER_DAY as f64 {
        SECS_PER_DAY
    } else if visible_secs <= 120.0 * SECS_PER_DAY as f64 {
        7 * SECS_PER_DAY
    } else if visible_secs <= 900.0 * SECS_PER_DAY as f64 {
        30 * SECS_PER_DAY
    } else {
        365 * SECS_PER_DAY
    }
}

fn tick_step_secs(visible_secs: f64, rail_width: f32) -> i64 {
    let target = (visible_secs * (56.0 / rail_width.max(1.0) as f64)).max(1.0);
    const STEPS: [i64; 10] = [
        900,
        SECS_PER_HOUR,
        6 * SECS_PER_HOUR,
        12 * SECS_PER_HOUR,
        SECS_PER_DAY,
        7 * SECS_PER_DAY,
        30 * SECS_PER_DAY,
        90 * SECS_PER_DAY,
        365 * SECS_PER_DAY,
        5 * 365 * SECS_PER_DAY,
    ];
    STEPS
        .iter()
        .copied()
        .find(|&s| (s as f64) >= target)
        .unwrap_or(5 * 365 * SECS_PER_DAY)
}

fn draw_timeline_scale(
    painter: &egui::Painter,
    block: Rect,
    scale_h: f32,
    view_lo: f64,
    view_hi: f64,
    step: i64,
    theme: SidebarTheme,
) {
    let scale_top = block.bottom() - scale_h;
    let baseline = scale_top + 4.0;
    let label_y = scale_top + 10.0;
    let tick_color = theme.sub.gamma_multiply(0.85);
    let minor = Stroke::new(1.0, tick_color.gamma_multiply(0.45));
    let major = Stroke::new(1.0, tick_color);

    let first = (view_lo.floor() as i64 / step) * step;
    let mut t = first;
    let mut last_label_x = f32::MIN;
    while (t as f64) <= view_hi + step as f64 {
        let x = time_to_x(t as f64, block, view_lo, view_hi);
        if x >= block.left() - 2.0 && x <= block.right() + 2.0 {
            let is_major = if step >= SECS_PER_DAY {
                t % step == 0
            } else {
                t % step == 0
            };
            let h = if is_major { 6.0 } else { 3.0 };
            painter.line_segment(
                [Pos2::new(x, baseline), Pos2::new(x, baseline + h)],
                if is_major { major } else { minor },
            );
            if is_major && x - last_label_x > 28.0 {
                let label = timeline_tick_label(t, step);
                painter.text(
                    Pos2::new(x, label_y),
                    egui::Align2::CENTER_TOP,
                    label,
                    FontId::proportional(9.0),
                    theme.sub,
                );
                last_label_x = x;
            }
        }
        t += step;
    }
}

fn time_to_x(t: f64, block: Rect, view_lo: f64, view_hi: f64) -> f32 {
    let w = (view_hi - view_lo).max(1.0);
    block.left() + ((t - view_lo) / w * block.width() as f64) as f32
}

fn time_at_x(x: f32, block: Rect, view_lo: f64, view_hi: f64) -> f64 {
    if block.width() <= 0.0 {
        return view_lo;
    }
    let t = ((x - block.left()) / block.width()).clamp(0.0, 1.0) as f64;
    view_lo + t * (view_hi - view_lo).max(1.0)
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
