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
    self, Align, Align2, Color32, CursorIcon, FontId, Layout, Pos2, Rect, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2,
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
    /// Optional right-click action label for replacing the tab's content.
    pub content_action_label: Option<&'static str>,
    /// Empty tabs render without a label and invite content selection:
    /// clicking the active empty tab yields [`TabAction::ActivateEmpty`]
    /// instead of a switch.
    pub is_empty: bool,
}

pub enum TabAction {
    Switch(usize),
    Close(usize),
    New,
    /// Right-click tab menu: app-specific content replacement. File Atlas uses
    /// this for "Change directory…"; Slate may ignore it.
    ChangeContent(usize),
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
    paint_vertical_gradient_top_fillet(painter, rect, top, bottom, 0.0);
}

/// Top-to-bottom bar gradient whose scanlines follow a top-only fillet so
/// a portal identity tab cannot oversail the host frame's rounded corners.
fn paint_vertical_gradient_top_fillet(
    painter: &egui::Painter,
    rect: Rect,
    top: Color32,
    bottom: Color32,
    radius: f32,
) {
    let r = radius.min(rect.width() * 0.5).min(rect.height()).max(0.0);
    let steps = rect.height().ceil().max(1.0) as usize;
    for step in 0..steps {
        let t = step as f32 / steps as f32;
        let y = rect.top() + rect.height() * t;
        let (left, right) = if r < 0.5 || y >= rect.top() + r {
            (rect.left(), rect.right())
        } else {
            let dy = y - (rect.top() + r);
            let dx = (r * r - dy * dy).max(0.0).sqrt();
            (rect.left() + r - dx, rect.right() - r + dx)
        };
        if right - left < 0.5 {
            continue;
        }
        painter.line_segment(
            [Pos2::new(left, y), Pos2::new(right, y)],
            Stroke::new(1.25_f32, lerp_color(top, bottom, t)),
        );
    }
}

fn active_tab_x_bounds(rect: Rect, y: f32, metrics: &TopBarTokens) -> (f32, f32) {
    let shoulder = metrics.tab_shoulder_radius.max(0.0);
    let body_left = rect.left() + shoulder;
    let body_right = rect.right() - shoulder;
    let radius = metrics.tab_top_radius.max(0.0);

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
            rect.left() + metrics.tab_shoulder_radius + metrics.tab_top_inset * 0.5,
            rect.top() + metrics.tab_top_inset * 0.25,
        ),
        Pos2::new(
            rect.right() - metrics.tab_shoulder_radius - metrics.tab_top_inset * 0.5,
            rect.top() + metrics.tab_top_inset * 0.42,
        ),
    );
    paint_vertical_gradient(
        painter,
        inner,
        Color32::from_white_alpha((metrics.inner_highlight_opacity.clamp(0.0, 1.0) * 255.0) as u8),
        Color32::TRANSPARENT,
    );
}

fn paint_inactive_dividers(painter: &egui::Painter, slots: &[TabSlot], colors: TabChromeColors) {
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
            if let Some(label) = spec.content_action_label {
                resp.clone().context_menu(|ui| {
                    let dark = ui.visuals().dark_mode;
                    crate::menu::prepare(ui, dark);
                    if crate::menu::item(ui, crate::menu::MenuIcon::Folder, label, dark).clicked() {
                        action = Some(TabAction::ChangeContent(i));
                        ui.close_menu();
                    }
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

// ---------------------------------------------------------------------------
// Portal identity tab — same Chrome-style tab language, one tab, no '+'
// ---------------------------------------------------------------------------

/// What a portal wants shown on its identity tab.
pub struct PortalTabModel<'a> {
    pub title: &'a str,
    pub tooltip: &'a str,
    pub live: bool,
    pub maximized: bool,
}

/// Clicks on a portal identity tab or its chrome buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortalTabAction {
    ToggleMaximize,
    Collapse,
    Context,
}

/// Windows / Slate caption maximize glyph: a square made of four corner
/// chevrons. `restored` paints the overlapping-squares restore mark.
/// Do not use a Unicode box — those fight the tab typeface.
pub fn paint_maximize_glyph(painter: &egui::Painter, hit: Rect, color: Color32, restored: bool) {
    // Slim portal bars are short; the window-control slot is wider than it
    // is tall. Size the glyph from the larger axis so it stays readable.
    let scale = (hit.width().max(hit.height()) / 30.0).max(0.01);
    let c = hit.center();
    let s = Stroke::new(1.2 * scale, color);
    if restored {
        let r = Rect::from_center_size(c + Vec2::new(-1.0, 1.0) * scale, Vec2::splat(8.0 * scale));
        let back = r.translate(Vec2::new(2.5 * scale, -2.5 * scale));
        painter.line_segment([back.left_top(), back.right_top()], s);
        painter.line_segment([back.right_top(), back.right_bottom()], s);
        painter.rect_stroke(r, 1.0 * scale, s, StrokeKind::Middle);
        return;
    }
    let half = 4.5 * scale;
    let arm = 3.2 * scale;
    let r = Rect::from_center_size(c, Vec2::splat(half * 2.0));
    painter.line_segment([r.left_top(), Pos2::new(r.left() + arm, r.top())], s);
    painter.line_segment([r.left_top(), Pos2::new(r.left(), r.top() + arm)], s);
    painter.line_segment([r.right_top(), Pos2::new(r.right() - arm, r.top())], s);
    painter.line_segment([r.right_top(), Pos2::new(r.right(), r.top() + arm)], s);
    painter.line_segment(
        [r.right_bottom(), Pos2::new(r.right() - arm, r.bottom())],
        s,
    );
    painter.line_segment(
        [r.right_bottom(), Pos2::new(r.right(), r.bottom() - arm)],
        s,
    );
    painter.line_segment([r.left_bottom(), Pos2::new(r.left() + arm, r.bottom())], s);
    painter.line_segment([r.left_bottom(), Pos2::new(r.left(), r.bottom() - arm)], s);
}

/// Floating maximize hit on a portal that has no identity tab (or whose tab
/// is folded). Sits on the node, upper-right. Returns true on click.
pub fn portal_maximize_button(
    ui: &Ui,
    palette: &Palette,
    rect: Rect,
    id_salt: u64,
    maximized: bool,
) -> bool {
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return false;
    }
    let resp = ui.interact(rect, ui.id().with(("portal_max", id_salt)), Sense::click());
    paint_maximize_glyph(
        ui.painter(),
        rect,
        if resp.hovered() {
            palette.ink
        } else {
            palette.sub
        },
        maximized,
    );
    resp.clicked()
}

/// Paint one workbook-style tab on a portal frame, plus the maximize hit.
/// `maximize` is the window-control slot on the right of the bar — the
/// same place the Slate / File Atlas caption maximize sits. `frame_radius`
/// is the host fillet in screen pixels so the bar follows the frame
/// corners instead of oversailing them. Geometry reuses [`TopBarTokens`];
/// apps must not paint their own tab shapes (Art. X).
pub fn portal_tab_bar(
    ui: &Ui,
    palette: &Palette,
    bar: Rect,
    maximize: Rect,
    frame_radius: f32,
    id_salt: u64,
    model: &PortalTabModel<'_>,
) -> Option<PortalTabAction> {
    let tokens = crate::tokens::current();
    let colors = TabChromeColors::from_palette(palette, &tokens.topbar);
    let painter = ui.painter();
    let h = bar.height();
    if h < 2.0 {
        return None;
    }
    // The portal bar is slimmer than the dashboard top bar. Scale type and
    // padding to the strip we actually have, not to the 0.4 height ratio —
    // that would produce 5 px type on a 12 px bar.
    let scale = (h / tokens.topbar.height.max(1.0)).max(0.0);
    let mut metrics = tokens.topbar.scaled(scale);
    metrics.tab_top_inset = (h * 0.08).min(2.0);
    metrics.tab_text_size = (h * 0.62).max(1.0);
    metrics.tab_horizontal_padding = (h * 0.45).max(4.0);
    let inner_r = frame_radius.min(h * 0.45);
    metrics.tab_top_radius = inner_r;
    metrics.tab_shoulder_radius = inner_r;

    paint_vertical_gradient_top_fillet(painter, bar, colors.bar_top, colors.bar, frame_radius);

    let pointer = ui.ctx().pointer_latest_pos();
    let over_maximize = pointer.is_some_and(|p| maximize.contains(p));

    let title = crate::widgets::trunc(model.title, metrics.tab_title_chars);
    let font = FontId::proportional(metrics.tab_text_size);
    let text_w =
        crate::canvas_text::layout_no_wrap(&painter, title.clone(), font.clone(), Color32::WHITE)
            .size()
            .x;
    let pad = metrics.tab_horizontal_padding;
    let tab_left = bar.left() + frame_radius.max(4.0 * scale.max(0.4));
    let tab_w = (text_w + pad * 2.0)
        .clamp(metrics.tab_min_width, metrics.tab_max_width)
        .min((maximize.left() - tab_left - 4.0).max(metrics.tab_min_width * 0.5));
    let tab_slot = Rect::from_min_max(
        Pos2::new(tab_left, bar.top()),
        Pos2::new(tab_left + tab_w, bar.bottom()),
    );
    let paint = tab_paint_rect(tab_slot, &metrics);
    paint_active_tab(
        painter,
        paint,
        colors.active_top,
        colors.active,
        colors,
        &metrics,
    );
    crate::canvas_text::text(
        &painter,
        Pos2::new(paint.left() + pad, paint.center().y),
        Align2::LEFT_CENTER,
        title,
        font,
        palette.ink,
    );
    if model.live {
        painter.circle_filled(
            Pos2::new(paint.right() - 10.0 * scale, paint.center().y),
            3.0 * scale,
            Color32::from_rgb(120, 220, 150),
        );
    }

    paint_maximize_glyph(
        painter,
        maximize,
        if over_maximize {
            palette.ink
        } else {
            palette.sub
        },
        model.maximized,
    );

    let tab_id = ui.id().with(("portal_tab", id_salt));
    let tab_resp = ui.interact(tab_slot, tab_id, Sense::click());
    tab_resp.clone().on_hover_text(model.tooltip);
    let max_resp = ui.interact(
        maximize,
        ui.id().with(("portal_max", id_salt)),
        Sense::click(),
    );
    if max_resp.clicked() {
        return Some(PortalTabAction::ToggleMaximize);
    }
    if tab_resp.secondary_clicked() || max_resp.secondary_clicked() {
        return Some(PortalTabAction::Context);
    }
    None
}

/// Hover affordance at the top interior of a portal whose identity tab is folded.
/// Returns true when the user clicks to expand.
pub fn portal_reveal_hint(ui: &Ui, palette: &Palette, strip: Rect, id_salt: u64) -> bool {
    let pointer = ui.ctx().pointer_latest_pos();
    let hovered = pointer.is_some_and(|p| strip.contains(p));
    let painter = ui.painter();
    if hovered {
        let s = strip.height().max(0.01);
        painter.rect_filled(strip, s * 0.2, palette.card.gamma_multiply(0.72));
        crate::canvas_text::text(
            painter,
            strip.center(),
            Align2::CENTER_CENTER,
            "⌄",
            FontId::proportional(s * 0.85),
            palette.ink,
        );
    }
    ui.interact(
        strip,
        ui.id().with(("portal_reveal", id_salt)),
        Sense::click(),
    )
    .clicked()
}
