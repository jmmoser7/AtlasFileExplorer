//! Shared egui widgets for toolbars and readouts.

use crate::theme::Palette;
use crate::tokens;
use eframe::egui::{self, Color32, CornerRadius, Id, Pos2, Rect, Sense, Stroke, Ui, Vec2};

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

/// Painted grip radius for thin sidebar sliders. The egui `Slider` handle
/// used previously rendered at ~5.6 px radius; the grips are deliberately
/// 60% smaller than that.
const THIN_SLIDER_HANDLE_RADIUS: f32 = 2.2;
/// Visible rail thickness (matches the previous `slider_rail_height`).
const THIN_SLIDER_RAIL_HEIGHT: f32 = 2.5;
/// Allocated (visual) height of the rail strip.
const THIN_SLIDER_HEIGHT: f32 = 8.0;
/// Extra vertical hit slop so the thin strip stays easy to grab.
const THIN_SLIDER_HIT_SLOP: f32 = 3.0;
/// Gap between the rail strip and its label/value row. Kept tight so the
/// text reads as belonging to the slider above it, never the one below.
const THIN_SLIDER_LABEL_GAP: f32 = 1.0;
/// Separation above each rail — keeps a stacked slider clearly apart from
/// the previous slider's label row.
const THIN_SLIDER_TOP_GAP: f32 = 5.0;

/// Shared rail + grip painting and pointer handling for thin sliders.
/// `frac` is the normalized handle position in `0..=1`.
fn thin_slider_rail(ui: &mut Ui, frac: &mut f32, hover: &str) -> bool {
    ui.add_space(THIN_SLIDER_TOP_GAP);
    let width = ui.available_width();
    let (rect, alloc) =
        ui.allocate_exact_size(Vec2::new(width, THIN_SLIDER_HEIGHT), Sense::hover());
    let resp = ui
        .interact(
            rect.expand2(Vec2::new(0.0, THIN_SLIDER_HIT_SLOP)),
            alloc.id.with("thin_slider"),
            Sense::click_and_drag(),
        )
        .on_hover_text(hover);

    let x0 = rect.left() + THIN_SLIDER_HANDLE_RADIUS;
    let x1 = (rect.right() - THIN_SLIDER_HANDLE_RADIUS).max(x0 + 1.0);
    let mut changed = false;
    if resp.is_pointer_button_down_on() || resp.dragged() {
        // Don't let a parent ScrollArea bury the thin grip.
        ui.input_mut(|i| {
            i.smooth_scroll_delta = Vec2::ZERO;
            i.raw_scroll_delta = Vec2::ZERO;
        });
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = ((pos.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            if t != *frac {
                *frac = t;
                changed = true;
            }
        }
    }

    let painter = ui.painter();
    let rail = Rect::from_center_size(
        rect.center(),
        Vec2::new(rect.width(), THIN_SLIDER_RAIL_HEIGHT),
    );
    painter.rect_filled(
        rail,
        THIN_SLIDER_RAIL_HEIGHT * 0.5,
        ui.visuals().widgets.inactive.bg_fill,
    );
    let visuals = ui.style().interact(&resp);
    let cx = x0 + (x1 - x0) * frac.clamp(0.0, 1.0);
    painter.circle(
        Pos2::new(cx, rect.center().y),
        THIN_SLIDER_HANDLE_RADIUS + visuals.expansion,
        visuals.bg_fill,
        visuals.fg_stroke,
    );
    changed
}

fn thin_slider_label_row(ui: &mut Ui, label: &str, value_text: &str, sub_color: Color32) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small().color(sub_color));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value_text).small().color(sub_color));
        });
    });
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
    let (lo, hi) = (*range.start(), *range.end());
    let before = *value;
    *value = (*value).clamp(lo, hi);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = THIN_SLIDER_LABEL_GAP;
        let mut frac = if hi > lo {
            (*value - lo) as f32 / (hi - lo) as f32
        } else {
            0.0
        };
        if thin_slider_rail(ui, &mut frac, hover) {
            *value = lo + (frac * (hi - lo) as f32).round() as usize;
        }
        thin_slider_label_row(ui, label, &format!("{} {}", *value, unit), sub_color);
    });
    *value != before
}

/// Signed variant of [`thin_sidebar_slider`] for ranges spanning zero
/// (e.g. hue rotation in degrees). Same rail, grip, and label layout.
pub fn thin_sidebar_slider_i32(
    ui: &mut Ui,
    value: &mut i32,
    range: std::ops::RangeInclusive<i32>,
    label: &str,
    unit: &str,
    hover: &str,
    sub_color: Color32,
) -> bool {
    let (lo, hi) = (*range.start(), *range.end());
    let before = *value;
    *value = (*value).clamp(lo, hi);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = THIN_SLIDER_LABEL_GAP;
        let mut frac = if hi > lo {
            (*value - lo) as f32 / (hi - lo) as f32
        } else {
            0.0
        };
        if thin_slider_rail(ui, &mut frac, hover) {
            *value = lo + (frac * (hi - lo) as f32).round() as i32;
        }
        thin_slider_label_row(ui, label, &format!("{} {}", *value, unit), sub_color);
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
    let icon = egui::RichText::new("⚙").size(8.0);
    let dark = ui.visuals().dark_mode;
    ui.menu_button(icon, |ui| {
        crate::menu::prepare(ui, dark);
        build(ui);
    })
    .response
    .on_hover_text("Choose visible panels");
}

/// Windows-style menu toggle: checkmark prefix, never a square checkbox.
/// Returns `true` when the user toggles the row.
pub fn menu_check_row(ui: &mut Ui, on: &mut bool, label: &str) -> bool {
    let dark = ui.visuals().dark_mode;
    if crate::menu::toggle(ui, *on, label, dark).clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

/// State the app feeds into [`canvas_mini_menu`] each frame.
pub struct MiniMenuModel {
    /// Camera zoom in percent; `None` hides the zoom cluster (views that own
    /// their camera separately, e.g. the Slate board).
    pub zoom_pct: Option<f32>,
    /// Whether the bottom readout strip is currently hidden.
    pub fullscreen: bool,
}

pub enum MiniMenuAction {
    ZoomOut,
    /// Reset zoom to 100%.
    ZoomReset,
    ZoomIn,
    /// Fit the content in view.
    Fit,
    /// Collapse or expand the bottom readout strip.
    ToggleFullscreen,
}

/// Lower-left canvas chrome: a tight readout-collapse chevron plus optional
/// zoom controls. Shared so File Atlas and Slate stay identical.
pub fn canvas_mini_menu(
    ctx: &egui::Context,
    palette: &Palette,
    id: &str,
    canvas: Rect,
    model: MiniMenuModel,
) -> Option<MiniMenuAction> {
    let mut action = None;
    let t = tokens::current().readouts;
    let hit = t.chevron_hit.max(t.chevron_size + 6.0);
    let pos = canvas.left_bottom() + Vec2::new(t.chevron_inset_x, -t.chevron_inset_y);
    egui::Area::new(Id::new(("readout_chevron", id)))
        .fixed_pos(pos)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(hit), Sense::click());
            let hovered = resp.hovered();
            if hovered {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(4),
                    palette.ink.gamma_multiply(t.chevron_hover_fill),
                );
                let top = [
                    Pos2::new(rect.left() + 3.0, rect.top() + 1.0),
                    Pos2::new(rect.right() - 3.0, rect.top() + 1.0),
                ];
                ui.painter().line_segment(
                    top,
                    Stroke::new(
                        1.0_f32,
                        Color32::from_white_alpha((t.chevron_emboss * 255.0) as u8),
                    ),
                );
            }
            let color = palette.ink.gamma_multiply(if hovered {
                t.chevron_hover_opacity
            } else {
                t.chevron_idle_opacity
            });
            paint_readout_chevron(
                ui.painter(),
                rect.center(),
                t.chevron_size,
                t.chevron_stroke,
                color,
                !model.fullscreen,
            );
            let hint = if model.fullscreen {
                "Show the bottom readout bar (F11)"
            } else {
                "Hide the bottom readout bar (F11)"
            };
            if resp.on_hover_text(hint).clicked() {
                action = Some(MiniMenuAction::ToggleFullscreen);
            }
        });

    if let Some(pct) = model.zoom_pct {
        let zoom_pos = pos + Vec2::new(hit + 6.0, 0.0);
        egui::Area::new(Id::new(("canvas_zoom_cluster", id)))
            .fixed_pos(zoom_pos)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.horizontal(|ui| {
                    if ui.small_button("−").clicked() {
                        action = Some(MiniMenuAction::ZoomOut);
                    }
                    if ui
                        .small_button(format!("{pct:.0}%"))
                        .on_hover_text("Reset to 100%")
                        .clicked()
                    {
                        action = Some(MiniMenuAction::ZoomReset);
                    }
                    if ui.small_button("+").clicked() {
                        action = Some(MiniMenuAction::ZoomIn);
                    }
                    if ui.small_button("Fit").clicked() {
                        action = Some(MiniMenuAction::Fit);
                    }
                });
            });
    }
    action
}

fn paint_readout_chevron(
    painter: &egui::Painter,
    c: Pos2,
    size: f32,
    stroke: f32,
    color: Color32,
    down: bool,
) {
    let w = size * 0.55;
    let h = size * 0.32;
    let dir = if down { 1.0 } else { -1.0 };
    let tip = c + Vec2::new(0.0, dir * h);
    let left = c + Vec2::new(-w, -dir * h);
    let right = c + Vec2::new(w, -dir * h);
    let s = Stroke::new(stroke, color);
    painter.line_segment([left, tip], s);
    painter.line_segment([tip, right], s);
}
