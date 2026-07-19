//! Shared floating canvas docks: spaced squircle icons floating directly on
//! the canvas, with popover panels anchored to the hovered/pinned icon.
//!
//! Apps provide data ([`DockItem`]s) and panel bodies (a per-frame callback);
//! this module owns all chrome: squircle painting, placement, hover-open,
//! click-pin, popover framing, and close behavior. See `DOCK.md`.
//!
//! Extension model: adding a tool = adding one [`DockItem`] (id + label +
//! icon + kind) and one arm in the app's body callback. Renaming = changing
//! `label`. App-specific icons use [`DockIcon::Custom`] with a painter fn.

use crate::theme::Palette;
use crate::tokens::{DockThemeTokens, DockTokens};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Pos2, Rect, RichText, ScrollArea, Sense, Shadow, Shape,
    Stroke, Vec2,
};

/// Vector icon painter: draw into `rect` using `color`. Keeps icons crisp at
/// any DPI and lets app crates supply custom icons without shell changes.
pub type DockIconPainter = fn(&egui::Painter, Rect, Color32);

/// Built-in monochrome line icons, plus [`DockIcon::Custom`] for app-specific
/// painters (e.g. Slate's board tool icons).
#[derive(Clone, Copy)]
pub enum DockIcon {
    Filters,
    Display,
    Workflow,
    Ai,
    Tags,
    Selection,
    View,
    Lens,
    Custom(DockIconPainter),
}

/// How an icon responds to interaction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockItemKind {
    /// Hover opens a popover panel; click pins it (and also reports a click).
    Panel,
    /// Click fires an action; no popover. Label appears as a tooltip.
    Action,
}

pub struct DockItem<'a> {
    /// Stable id, returned on click and passed to the panel body callback.
    pub id: &'static str,
    /// Human name: popover header (Panel) or tooltip (Action).
    pub label: &'a str,
    pub icon: DockIcon,
    pub kind: DockItemKind,
    /// Highlight the squircle (active tool / non-empty filter…).
    pub active: bool,
    pub visible: bool,
    /// Extra gap before this icon — visual grouping without a strip.
    pub gap_before: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    /// Vertical stack, centered on the canvas's left edge; popovers open right.
    LeftCenter,
    /// Horizontal row, centered on the canvas's bottom edge; popovers open up.
    BottomCenter,
}

#[derive(Clone, Default)]
struct DockState {
    open: Option<&'static str>,
    pinned: bool,
    last_inside_time: f64,
}

fn theme<'a>(palette: &Palette, tokens: &'a DockTokens) -> &'a DockThemeTokens {
    if palette.bg.r() > 128 {
        &tokens.light
    } else {
        &tokens.dark
    }
}

// ---------- squircle + icon painting ----------

fn squircle_points(rect: Rect, exponent: f32, samples: usize) -> Vec<Pos2> {
    let c = rect.center();
    let a = rect.width() * 0.5;
    let b = rect.height() * 0.5;
    let n = exponent.max(2.0);
    (0..samples)
        .map(|i| {
            let t = std::f32::consts::TAU * i as f32 / samples as f32;
            let (st, ct) = t.sin_cos();
            let x = a * ct.signum() * ct.abs().powf(2.0 / n);
            let y = b * st.signum() * st.abs().powf(2.0 / n);
            Pos2::new(c.x + x, c.y + y)
        })
        .collect()
}

fn paint_squircle(painter: &egui::Painter, rect: Rect, fill: Color32, stroke: Stroke, n: f32) {
    painter.add(Shape::convex_polygon(
        squircle_points(rect, n, 32),
        fill,
        stroke,
    ));
}

fn pt(r: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(r.min.x + r.width() * x, r.min.y + r.height() * y)
}

fn paint_builtin_icon(painter: &egui::Painter, rect: Rect, icon: DockIcon, color: Color32) {
    let s = Stroke::new((rect.width() * 0.075).clamp(1.2, 1.8), color);
    match icon {
        DockIcon::Custom(paint) => paint(painter, rect, color),
        DockIcon::Filters => {
            for (y, knob) in [(0.28, 0.68), (0.50, 0.36), (0.72, 0.58)] {
                painter.line_segment([pt(rect, 0.18, y), pt(rect, 0.82, y)], s);
                painter.circle_filled(pt(rect, knob, y), rect.width() * 0.055, color);
            }
        }
        DockIcon::Display => {
            let screen = Rect::from_min_max(pt(rect, 0.18, 0.24), pt(rect, 0.82, 0.68));
            painter.rect_stroke(screen, 2.0, s, egui::StrokeKind::Inside);
            painter.line_segment([pt(rect, 0.42, 0.80), pt(rect, 0.58, 0.80)], s);
            painter.line_segment([pt(rect, 0.50, 0.68), pt(rect, 0.50, 0.80)], s);
        }
        DockIcon::Workflow => {
            for (x, y) in [(0.24, 0.30), (0.72, 0.30), (0.72, 0.72)] {
                painter.circle_stroke(pt(rect, x, y), rect.width() * 0.085, s);
            }
            painter.line_segment([pt(rect, 0.32, 0.30), pt(rect, 0.62, 0.30)], s);
            painter.line_segment([pt(rect, 0.72, 0.39), pt(rect, 0.72, 0.62)], s);
        }
        DockIcon::Ai => {
            painter.circle_stroke(rect.center(), rect.width() * 0.25, s);
            for p in [
                pt(rect, 0.50, 0.14),
                pt(rect, 0.76, 0.50),
                pt(rect, 0.50, 0.86),
                pt(rect, 0.24, 0.50),
            ] {
                painter.line_segment(
                    [rect.center(), p],
                    Stroke::new(s.width * 0.75, color.gamma_multiply(0.7)),
                );
                painter.circle_filled(p, rect.width() * 0.04, color);
            }
        }
        DockIcon::Tags => {
            let points = vec![
                pt(rect, 0.22, 0.28),
                pt(rect, 0.62, 0.20),
                pt(rect, 0.82, 0.42),
                pt(rect, 0.46, 0.78),
                pt(rect, 0.22, 0.58),
            ];
            painter.add(Shape::closed_line(points, s));
            painter.circle_stroke(pt(rect, 0.42, 0.40), rect.width() * 0.055, s);
        }
        DockIcon::Selection => {
            let points = vec![
                pt(rect, 0.22, 0.18),
                pt(rect, 0.78, 0.48),
                pt(rect, 0.55, 0.56),
                pt(rect, 0.68, 0.82),
                pt(rect, 0.56, 0.88),
                pt(rect, 0.43, 0.61),
                pt(rect, 0.25, 0.76),
            ];
            painter.add(Shape::closed_line(points, s));
        }
        DockIcon::View => {
            painter.add(Shape::ellipse_stroke(
                rect.center(),
                Vec2::new(rect.width() * 0.34, rect.height() * 0.20),
                s,
            ));
            painter.circle_filled(rect.center(), rect.width() * 0.075, color);
        }
        DockIcon::Lens => {
            painter.circle_stroke(pt(rect, 0.43, 0.42), rect.width() * 0.22, s);
            painter.line_segment([pt(rect, 0.60, 0.60), pt(rect, 0.82, 0.82)], s);
            painter.line_segment(
                [pt(rect, 0.30, 0.42), pt(rect, 0.56, 0.42)],
                Stroke::new(s.width * 0.75, color.gamma_multiply(0.7)),
            );
        }
    }
}

fn popover_frame(t: &DockTokens, th: &DockThemeTokens) -> egui::Frame {
    egui::Frame::new()
        .fill(th.popover_fill_color())
        .stroke(Stroke::new(1.0_f32, th.border_color()))
        .corner_radius(CornerRadius::same(
            t.popover_corner_radius.clamp(0.0, 255.0) as u8,
        ))
        .shadow(Shadow {
            offset: [
                t.shadow_offset_x.clamp(-127.0, 127.0) as i8,
                t.shadow_offset_y.clamp(-127.0, 127.0) as i8,
            ],
            blur: t.shadow_blur.clamp(0.0, 255.0) as u8,
            spread: t.shadow_spread.clamp(0.0, 255.0) as u8,
            color: Color32::from_black_alpha((t.shadow_opacity.clamp(0.0, 1.0) * 255.0) as u8),
        })
        .inner_margin(egui::Margin::same(t.popover_padding.clamp(0.0, 127.0) as i8))
}

/// Dev/screenshot harness: `ATLAS_DOCK_OPEN=<item id>` pins that panel open.
/// Read once and leaked once — a deliberate one-time allocation.
fn forced_open() -> Option<&'static str> {
    use std::sync::OnceLock;
    static FORCED: OnceLock<Option<&'static str>> = OnceLock::new();
    *FORCED.get_or_init(|| {
        std::env::var("ATLAS_DOCK_OPEN")
            .ok()
            .map(|s| &*Box::leak(s.into_boxed_str()))
    })
}

/// Render a floating dock. Returns the id of a clicked icon, if any
/// (both Panel and Action items report clicks so apps can react).
pub fn floating_dock(
    ctx: &egui::Context,
    id: impl std::hash::Hash,
    canvas: Rect,
    palette: &Palette,
    side: DockSide,
    items: &[DockItem<'_>],
    mut panel_body: impl FnMut(&mut egui::Ui, &'static str),
) -> Option<&'static str> {
    let mut tokens = crate::tokens::current().dock;
    tokens.normalize();
    let th = theme(palette, &tokens);
    let state_id = egui::Id::new(("floating_dock", id));
    let mut state = ctx.data_mut(|d| d.get_temp::<DockState>(state_id).unwrap_or_default());
    let now = ctx.input(|i| i.time);
    let mut clicked: Option<&'static str> = None;

    let visible: Vec<&DockItem<'_>> = items.iter().filter(|item| item.visible).collect();
    if visible.is_empty() {
        return None;
    }

    // Screenshot/dev harness and live tuner preview lock.
    if let Some(forced) = forced_open().or_else(crate::tuning::dock_preview_panel) {
        if let Some(item) = visible.iter().find(|item| item.id == forced) {
            state.open = Some(item.id);
            state.pinned = true;
        }
    }

    // Close a panel whose icon disappeared (e.g. Align without a selection).
    if let Some(open) = state.open {
        if !visible
            .iter()
            .any(|item| item.id == open && item.kind == DockItemKind::Panel)
        {
            state = DockState::default();
        }
    }

    // ---- icon strip: one small anchored area, icons laid out by egui ----
    let bar_area = match side {
        DockSide::LeftCenter => egui::Area::new(state_id.with("bar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::LEFT_CENTER)
            .fixed_pos(Pos2::new(
                canvas.left() + tokens.left_margin,
                canvas.center().y,
            ))
            .constrain(false),
        DockSide::BottomCenter => egui::Area::new(state_id.with("bar"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_BOTTOM)
            .fixed_pos(Pos2::new(
                canvas.center().x,
                canvas.bottom() - tokens.bottom_margin,
            ))
            .constrain(false),
    };

    let mut open_anchor: Option<Rect> = None;
    let bar_response = bar_area.show(ctx, |ui| {
        let mut draw_items = |ui: &mut egui::Ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(tokens.icon_gap);
            for item in &visible {
                if item.gap_before {
                    ui.add_space(tokens.icon_gap * 1.5);
                }
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::splat(tokens.icon_size), Sense::click());
                let hovered = resp.hovered();
                let is_open = state.open == Some(item.id);
                let fill = if item.active || is_open {
                    th.icon_active_color()
                } else if hovered {
                    th.icon_hover_color()
                } else {
                    th.icon_fill_color()
                };
                paint_squircle(
                    ui.painter(),
                    rect.shrink(0.5),
                    fill,
                    Stroke::new(1.0_f32, th.border_color()),
                    tokens.squircle_exponent,
                );
                paint_builtin_icon(ui.painter(), rect.shrink(7.0), item.icon, th.text_color());

                if hovered {
                    state.last_inside_time = now;
                    if item.kind == DockItemKind::Panel && !state.pinned {
                        state.open = Some(item.id);
                    }
                }
                if resp.clicked() {
                    clicked = Some(item.id);
                    state.last_inside_time = now;
                    if item.kind == DockItemKind::Panel {
                        if state.open == Some(item.id) && state.pinned {
                            state = DockState::default();
                        } else {
                            state.open = Some(item.id);
                            state.pinned = true;
                        }
                    }
                }
                if state.open == Some(item.id) {
                    open_anchor = Some(rect);
                }
                if item.kind == DockItemKind::Action {
                    resp.on_hover_text(item.label);
                }
            }
        };
        match side {
            DockSide::LeftCenter => {
                ui.vertical(|ui| draw_items(ui));
            }
            DockSide::BottomCenter => {
                ui.horizontal(|ui| draw_items(ui));
            }
        }
    });
    let bar_rect = bar_response.response.rect;

    // ---- popover panel, anchored to the open icon ----
    let mut popover_rect = Rect::NOTHING;
    if let (Some(open_id), Some(anchor)) = (state.open, open_anchor) {
        let label = visible
            .iter()
            .find(|item| item.id == open_id)
            .map(|item| item.label.to_owned())
            .unwrap_or_default();
        let max_h = tokens
            .popover_max_height
            .min((canvas.height() - 60.0).max(120.0));
        let panel_area = match side {
            DockSide::LeftCenter => egui::Area::new(state_id.with("panel"))
                .order(egui::Order::Foreground)
                .pivot(Align2::LEFT_TOP)
                .fixed_pos(Pos2::new(anchor.right() + tokens.popover_gap, anchor.top()))
                .constrain_to(ctx.screen_rect()),
            DockSide::BottomCenter => egui::Area::new(state_id.with("panel"))
                .order(egui::Order::Foreground)
                .pivot(Align2::CENTER_BOTTOM)
                .fixed_pos(Pos2::new(
                    anchor.center().x,
                    anchor.top() - tokens.popover_gap,
                ))
                .constrain_to(ctx.screen_rect()),
        };
        let response = panel_area.show(ctx, |ui| {
            popover_frame(&tokens, th).show(ui, |ui| {
                ui.set_width((tokens.popover_width - tokens.popover_padding * 2.0).max(1.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().strong().color(th.text_color()));
                    if state.pinned {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("pinned").small().color(th.muted_text_color()));
                        });
                    }
                });
                ui.separator();
                ScrollArea::vertical()
                    .max_height(max_h)
                    .show(ui, |ui| panel_body(ui, open_id));
            });
        });
        popover_rect = response.response.rect;
    }

    // ---- close behavior ----
    let pointer_inside = ctx
        .pointer_latest_pos()
        .is_some_and(|p| bar_rect.expand(4.0).contains(p) || popover_rect.contains(p));
    if pointer_inside {
        state.last_inside_time = now;
    }
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let outside_click = ctx.input(|i| i.pointer.any_click()) && !pointer_inside;
    let hover_expired = !state.pinned && now - state.last_inside_time > tokens.close_delay as f64;
    if state.open.is_some() && (escape || outside_click || hover_expired) {
        state = DockState::default();
    } else if state.open.is_some() {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(state_id, state));

    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squircle_points_stay_inside_rect() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0));
        for point in squircle_points(rect, 4.0, 32) {
            assert!(rect.expand(0.01).contains(point));
        }
    }

    #[test]
    fn squircle_is_wider_than_circle_at_diagonal() {
        // The superellipse (n=4) should bulge past a circle of the same size
        // at 45° — that's what makes it a squircle and not a rounded circle.
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0));
        let c = rect.center();
        let diag = squircle_points(rect, 4.0, 64)
            .iter()
            .map(|p| (*p - c).length())
            .fold(0.0_f32, f32::max);
        assert!(
            diag > 10.0 * 1.05,
            "diagonal reach {diag} not squircle-like"
        );
    }
}
