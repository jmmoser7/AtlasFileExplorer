//! Browser-style tab strip — shared painting for the unified top bar.
//!
//! Data-driven so every app in the ecosystem paints identical tabs: the app
//! supplies [`TabSpec`]s and reacts to the returned [`TabAction`]. All
//! geometry and colors live here; apps must not paint their own tab chrome.
//!
//! Tabs render inline inside [`crate::menubar::unified_top_bar`] — they no
//! longer occupy a separate panel row.

use crate::theme::Palette;
use crate::tokens::TopBarTokens;
use crate::widgets::trunc;
use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontId, Layout, Pos2, Rect, Sense,
    Shape, Stroke, Ui, Vec2,
};

#[derive(Clone, Copy)]
pub struct TabChromeColors {
    pub bar: Color32,
    pub bar_top: Color32,
    pub inactive: Color32,
    pub inactive_hover: Color32,
    pub active: Color32,
    pub active_top: Color32,
    pub divider: Color32,
    pub accent_stroke: Color32,
}

impl TabChromeColors {
    pub fn from_palette(p: &Palette, metrics: &TopBarTokens) -> Self {
        let theme = if p.bg.r() > 128 {
            &metrics.light
        } else {
            &metrics.dark
        };
        Self {
            bar: theme.bar_color(),
            bar_top: theme.bar_top_color(),
            inactive: theme.inactive_color(),
            inactive_hover: theme.inactive_hover_color(),
            active: p.bg,
            active_top: lerp_color(p.bg, Color32::WHITE, theme.active_top_mix),
            divider: p.border.gamma_multiply(theme.divider_strength),
            accent_stroke: lerp_color(p.accent, Color32::WHITE, theme.accent_white_mix),
        }
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}

/// What the app wants shown for one tab.
pub struct TabSpec {
    pub title: String,
    pub tooltip: String,
    pub closable: bool,
    /// Empty tabs render without a label and invite content selection:
    /// clicking the active empty tab yields [`TabAction::ActivateEmpty`]
    /// instead of a switch.
    pub is_empty: bool,
}

pub enum TabAction {
    Switch(usize),
    Close(usize),
    New,
    /// Active empty tab clicked — the app opens its "choose content" flow
    /// (folder picker in File Atlas, workbook picker in Slate).
    ActivateEmpty,
}

struct TabSlot {
    paint: Rect,
    active: bool,
    hovered: bool,
    closable: bool,
    title: String,
}

fn tab_paint_rect(rect: Rect, metrics: &TopBarTokens) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x, rect.min.y + metrics.tab_top_inset),
        rect.max,
    )
}

pub(crate) fn paint_vertical_gradient(
    painter: &egui::Painter,
    rect: Rect,
    top: Color32,
    bottom: Color32,
) {
    let steps = rect.height().ceil().max(1.0) as usize;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let y = rect.top() + rect.height() * t;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.25_f32, lerp_color(top, bottom, t)),
        );
    }
}

fn active_tab_x_bounds(rect: Rect, y: f32, metrics: &TopBarTokens) -> (f32, f32) {
    let shoulder = metrics.tab_shoulder_radius.max(0.5);
    let body_left = rect.left() + shoulder;
    let body_right = rect.right() - shoulder;
    let radius = metrics.tab_top_radius.max(0.5);

    if y < rect.top() + radius {
        let dy = y - (rect.top() + radius);
        let dx = (radius * radius - dy * dy).max(0.0).sqrt();
        (body_left + radius - dx, body_right - radius + dx)
    } else if y > rect.bottom() - shoulder {
        let t = ((y - (rect.bottom() - shoulder)) / shoulder).clamp(0.0, 1.0);
        let flare = shoulder * (1.0 - (1.0 - t * t).sqrt());
        (body_left - flare, body_right + flare)
    } else {
        (body_left, body_right)
    }
}

fn active_tab_outline(rect: Rect, metrics: &TopBarTokens) -> Vec<Pos2> {
    let mut points = Vec::new();
    let samples = (rect.height() * 1.5).ceil() as usize;
    for i in (0..=samples).rev() {
        let y = rect.top() + rect.height() * i as f32 / samples as f32;
        points.push(Pos2::new(active_tab_x_bounds(rect, y, metrics).0, y));
    }
    for i in 0..=samples {
        let y = rect.top() + rect.height() * i as f32 / samples as f32;
        points.push(Pos2::new(active_tab_x_bounds(rect, y, metrics).1, y));
    }
    points
}

fn paint_active_tab(
    painter: &egui::Painter,
    rect: Rect,
    fill_top: Color32,
    fill_bottom: Color32,
    colors: TabChromeColors,
    metrics: &TopBarTokens,
) {
    let steps = rect.height().ceil().max(1.0) as usize;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let y = rect.top() + rect.height() * t;
        let (left, right) = active_tab_x_bounds(rect, y, metrics);
        painter.line_segment(
            [Pos2::new(left, y), Pos2::new(right, y)],
            Stroke::new(1.35_f32, lerp_color(fill_top, fill_bottom, t)),
        );
    }

    // Three nested strokes reproduce the reference's soft cyan falloff.
    let outline = active_tab_outline(rect, metrics);
    painter.add(Shape::line(
        outline.clone(),
        Stroke::new(
            metrics.glow_outer_width,
            colors
                .accent_stroke
                .gamma_multiply(metrics.glow_outer_opacity),
        ),
    ));
    painter.add(Shape::line(
        outline.clone(),
        Stroke::new(
            metrics.glow_middle_width,
            colors
                .accent_stroke
                .gamma_multiply(metrics.glow_middle_opacity),
        ),
    ));
    painter.add(Shape::line(
        outline,
        Stroke::new(
            metrics.glow_core_width,
            colors
                .accent_stroke
                .gamma_multiply(metrics.glow_core_opacity),
        ),
    ));

    // A faint inner highlight gives the raised/embossed top edge.
    let inner = Rect::from_min_max(
        Pos2::new(
            rect.left() + metrics.tab_shoulder_radius + 3.0,
            rect.top() + 1.5,
        ),
        Pos2::new(
            rect.right() - metrics.tab_shoulder_radius - 3.0,
            rect.top() + 2.5,
        ),
    );
    paint_vertical_gradient(
        painter,
        inner,
        Color32::from_white_alpha((metrics.inner_highlight_opacity.clamp(0.0, 1.0) * 255.0) as u8),
        Color32::TRANSPARENT,
    );
}

fn paint_inactive_dividers(
    painter: &egui::Painter,
    slots: &[TabSlot],
    colors: TabChromeColors,
) {
    for pair in slots.windows(2) {
        if pair[0].active || pair[1].active {
            continue;
        }
        let x = pair[0].paint.right();
        let y0 = pair[0].paint.top() + 5.0;
        let y1 = pair[0].paint.bottom() - 3.0;
        painter.line_segment(
            [Pos2::new(x, y0), Pos2::new(x, y1)],
            Stroke::new(1.0_f32, colors.divider),
        );
    }
}

/// Paint the hoverable close "×" for one tab slot.
fn paint_close_x(
    ui: &Ui,
    painter: &egui::Painter,
    slot: &TabSlot,
    palette: &Palette,
    metrics: &TopBarTokens,
) {
    let cx = egui::Rect::from_center_size(
        Pos2::new(
            slot.paint.right() - metrics.tab_horizontal_padding,
            slot.paint.center().y,
        ),
        Vec2::splat(14.0),
    );
    let over_x = ui
        .ctx()
        .pointer_latest_pos()
        .map(|p| cx.contains(p))
        .unwrap_or(false);
    painter.text(
        cx.center(),
        Align2::CENTER_CENTER,
        "×",
        FontId::proportional(metrics.plus_text_size),
        if over_x { palette.ink } else { palette.sub },
    );
}

/// Renders the tab strip inline inside an existing [`Ui`]; returns the user's
/// action, if any. Used by the unified top bar — not a standalone panel.
pub fn tab_strip(
    ui: &mut Ui,
    palette: &Palette,
    metrics: &TopBarTokens,
    tabs: &[TabSpec],
    active_tab: usize,
    busy: bool,
) -> Option<TabAction> {
    let colors = TabChromeColors::from_palette(palette, metrics);
    let mut action: Option<TabAction> = None;
    let mut slots: Vec<TabSlot> = Vec::new();

    ui.set_min_height(metrics.height);
    ui.with_layout(Layout::left_to_right(Align::BOTTOM), |ui| {
        for (i, spec) in tabs.iter().enumerate() {
            let active = i == active_tab;
            let title = trunc(&spec.title, metrics.tab_title_chars);

            let font = FontId::proportional(metrics.tab_text_size);
            let text_w = ui
                .painter()
                .layout_no_wrap(title.clone(), font.clone(), Color32::WHITE)
                .size()
                .x;
            let base_w = text_w
                + metrics.tab_horizontal_padding * 2.0
                + if spec.closable {
                    metrics.tab_close_width
                } else {
                    0.0
                };
            let w = base_w.clamp(metrics.tab_min_width, metrics.tab_max_width);
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, metrics.height), Sense::click());
            let hovered = resp.hovered() && !active;
            let paint = tab_paint_rect(rect, metrics);

            slots.push(TabSlot {
                paint,
                active,
                hovered,
                closable: spec.closable,
                title,
            });

            if spec.closable {
                let cx = egui::Rect::from_center_size(
                    Pos2::new(
                        rect.right_center().x - metrics.tab_horizontal_padding - 2.0,
                        paint.center().y,
                    ),
                    Vec2::splat(14.0),
                );
                let over_x = ui
                    .ctx()
                    .pointer_latest_pos()
                    .map(|p| cx.contains(p))
                    .unwrap_or(false);
                if resp.clicked() && over_x {
                    action = Some(TabAction::Close(i));
                } else if resp.clicked() {
                    action = Some(if active && spec.is_empty {
                        TabAction::ActivateEmpty
                    } else {
                        TabAction::Switch(i)
                    });
                }
            } else if resp.clicked() {
                action = Some(if active && spec.is_empty {
                    TabAction::ActivateEmpty
                } else {
                    TabAction::Switch(i)
                });
            }
            resp.on_hover_text(spec.tooltip.clone());
        }

        ui.add_space(2.0);
        let (prect, presp) = ui.allocate_exact_size(
            Vec2::new(metrics.plus_hit_width, metrics.height),
            Sense::click(),
        );
        let presp = presp.on_hover_cursor(CursorIcon::PointingHand);
        let plus_center = prect.center();
        let plus_hover = presp.hovered();
        if plus_hover {
            ui.painter()
                .circle_filled(plus_center, metrics.plus_radius, colors.inactive_hover);
        }
        ui.painter().text(
            plus_center,
            Align2::CENTER_CENTER,
            "+",
            FontId::proportional(metrics.plus_text_size),
            if plus_hover { palette.ink } else { palette.sub },
        );
        if presp.on_hover_text("New tab").clicked() {
            action = Some(TabAction::New);
        }

        if busy {
            ui.add_space(6.0);
            ui.spinner();
        }
    });

    let painter = ui.painter().clone();
    let active = slots.iter().find(|s| s.active);

    paint_inactive_dividers(&painter, &slots, colors);
    if let Some(slot) = active {
        paint_active_tab(
            &painter,
            slot.paint,
            colors.active_top,
            colors.active,
            colors,
            metrics,
        );
    }

    for slot in &slots {
        let text_color = if slot.active {
            palette.ink
        } else if slot.hovered {
            palette.ink.gamma_multiply(0.92)
        } else {
            palette.sub
        };
        painter.text(
            Pos2::new(
                slot.paint.left() + metrics.tab_horizontal_padding,
                slot.paint.center().y,
            ),
            Align2::LEFT_CENTER,
            slot.title.clone(),
            FontId::proportional(metrics.tab_text_size),
            text_color,
        );

        if slot.closable && (slot.hovered || slot.active) {
            paint_close_x(ui, &painter, slot, palette, metrics);
        }
    }

    action
}
