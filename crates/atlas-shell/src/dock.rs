//! Shared floating canvas docks: spaced squircle icons floating directly on
//! the canvas, with popover panels that can stack when several are open.
//!
//! Apps provide data ([`DockItem`]s) and panel bodies (a per-frame callback);
//! this module owns all chrome: squircle painting, placement, hover-open,
//! click-pin, multi-panel stacking, partition line, hover tracers, and close
//! behavior. See `DOCK.md`.
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
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
///
/// Apps should list **Tool** icons as one contiguous group and **Dashboard**
/// icons as another (neighbors by order only â€” no visible separator).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockItemKind {
    /// Settings dashboards (filters, tags, displayâ€¦). Hover â†’ name chip;
    /// prolonged hover â†’ description; click pins the full body. Hover never
    /// displaces the pinned dashboard stack.
    Dashboard,
    /// Tool flyouts (shapes, curves, navâ€¦). Hover opens the sub-tool panel;
    /// click pins that panel open.
    Tool,
    /// Click fires an action; no popover. Hover shows the shared label chip above the icon.
    Action,
}

impl DockItemKind {
    pub fn opens_body(self) -> bool {
        matches!(self, Self::Dashboard | Self::Tool)
    }
}

pub struct DockItem<'a> {
    /// Stable id, returned on click and passed to the panel body callback.
    pub id: &'static str,
    /// Human name: popover header / name chip / tooltip.
    pub label: &'a str,
    /// Longer blurb shown on prolonged Dashboard hover. Unused for Tool/Action.
    pub description: &'a str,
    pub icon: DockIcon,
    pub kind: DockItemKind,
    /// Highlight the squircle (active tool / non-empty filterâ€¦).
    pub active: bool,
    pub visible: bool,
    /// Extra gap before this icon â€” visual grouping without a strip.
    /// Prefer ordering Tool vs Dashboard neighbors instead of a separator.
    pub gap_before: bool,
}

/// Where the icon strip sits on the canvas. User-selectable in Preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DockSide {
    /// Vertical stack, centered on the canvas's left edge; popovers open right.
    #[default]
    LeftCenter,
    /// Horizontal row, centered on the canvas's bottom edge; popovers open up.
    BottomCenter,
}

impl DockSide {
    pub fn label(self) -> &'static str {
        match self {
            DockSide::LeftCenter => "Left edge",
            DockSide::BottomCenter => "Bottom edge",
        }
    }
}

#[derive(Clone, Default)]
struct DockState {
    /// Click-pinned dashboards and tools, in icon order.
    pinned: Vec<&'static str>,
    /// Transient Tool/Dashboard body while hovering (anchored above the icon; not in the stack).
    body_preview: Option<&'static str>,
    /// Short label chip for Action + Dashboard hovers (always above the spawning icon).
    label_hover: Option<&'static str>,
    label_hover_since: f64,
    /// 0..1 fade for Dashboard description text (smooth, not a hard toggle).
    describe_blend: f32,
    /// Ease-in for pinned stack panels and hover previews (id → 0..1).
    panel_open: HashMap<&'static str, f32>,
    last_inside_time: f64,
    /// Last-frame measured popover sizes for stack centering.
    panel_sizes: HashMap<&'static str, Vec2>,
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn lerp_toward(current: f32, target: f32, dt: f32, duration: f32) -> f32 {
    if duration <= 0.0 {
        return target;
    }
    let step = (dt / duration).clamp(0.0, 1.0);
    current + (target - current) * step
}

/// Popover anchor above (bottom dock) or beside (left dock) the spawning icon.
fn icon_popover_anchor(side: DockSide, icon: Rect, gap: f32) -> (Pos2, Align2) {
    match side {
        DockSide::LeftCenter => (
            Pos2::new(icon.right() + gap, icon.center().y),
            Align2::LEFT_CENTER,
        ),
        DockSide::BottomCenter => (
            Pos2::new(icon.center().x, icon.top() - gap),
            Align2::CENTER_BOTTOM,
        ),
    }
}

fn advance_panel_open(
    open: &mut HashMap<&'static str, f32>,
    active: &[&'static str],
    dt: f32,
    duration: f32,
) {
    open.retain(|id, _| active.contains(id));
    for &id in active {
        let v = open.entry(id).or_insert(0.0);
        *v = lerp_toward(*v, 1.0, dt, duration).min(1.0);
    }
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

/// Axis-aligned wire with rounded corners (File Atlas PCB-trace style).
pub fn rounded_route(painter: &egui::Painter, pts: &[Pos2], radius: f32, stroke: Stroke) {
    if pts.len() < 2 {
        return;
    }
    let mut cursor = pts[0];
    for i in 1..pts.len() {
        let cur = pts[i];
        if i + 1 < pts.len() {
            let next = pts[i + 1];
            let in_v = cur - cursor;
            let out_v = next - cur;
            let in_len = in_v.length();
            let out_len = out_v.length();
            let r = radius.min(in_len * 0.5).min(out_len * 0.5);
            if r < 0.5 || in_len < 0.5 || out_len < 0.5 {
                if in_len >= 0.5 {
                    painter.line_segment([cursor, cur], stroke);
                }
                cursor = cur;
                continue;
            }
            let a = cur - in_v.normalized() * r;
            let b = cur + out_v.normalized() * r;
            painter.line_segment([cursor, a], stroke);
            painter.add(Shape::CubicBezier(
                egui::epaint::CubicBezierShape::from_points_stroke(
                    [a, cur, cur, b],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ),
            ));
            cursor = b;
        } else {
            painter.line_segment([cursor, cur], stroke);
        }
    }
}

fn orthogonal_route(from: Pos2, to: Pos2, side: DockSide) -> [Pos2; 4] {
    match side {
        DockSide::LeftCenter => {
            let mid_x = (from.x + to.x) * 0.5;
            [from, Pos2::new(mid_x, from.y), Pos2::new(mid_x, to.y), to]
        }
        DockSide::BottomCenter => {
            let mid_y = (from.y + to.y) * 0.5;
            [from, Pos2::new(from.x, mid_y), Pos2::new(to.x, mid_y), to]
        }
    }
}

fn border_hovered(rect: Rect, pos: Pos2, hit: f32) -> bool {
    rect.expand(hit * 0.25).contains(pos) && !rect.shrink(hit).contains(pos)
}

/// Partition line: anti-aliased tapered ribbon (see `crate::taper` / `PAINT.md`).
fn paint_partition(
    painter: &egui::Painter,
    bar_rect: Rect,
    canvas: Rect,
    side: DockSide,
    tokens: &DockTokens,
    color: Color32,
) {
    if tokens.partition_max_thickness <= 0.01 || tokens.partition_opacity <= 0.01 {
        return;
    }
    let color = color.gamma_multiply(tokens.partition_opacity);
    let max_half = tokens.partition_max_thickness * 0.5;
    let min_half = tokens.partition_min_thickness * 0.5;
    match side {
        DockSide::LeftCenter => {
            let x = bar_rect.right() + tokens.partition_gap;
            let y0 = (bar_rect.top() - tokens.partition_extend).max(canvas.top() + 4.0);
            let y1 = (bar_rect.bottom() + tokens.partition_extend).min(canvas.bottom() - 4.0);
            crate::taper::paint_tapered_ribbon(
                painter,
                Pos2::new(x, y0),
                Pos2::new(x, y1),
                max_half,
                min_half,
                color,
            );
        }
        DockSide::BottomCenter => {
            let y = bar_rect.top() - tokens.partition_gap;
            let x0 = (bar_rect.left() - tokens.partition_extend).max(canvas.left() + 4.0);
            let x1 = (bar_rect.right() + tokens.partition_extend).min(canvas.right() - 4.0);
            crate::taper::paint_tapered_ribbon(
                painter,
                Pos2::new(x0, y),
                Pos2::new(x1, y),
                max_half,
                min_half,
                color,
            );
        }
    }
}

fn forced_open() -> Option<&'static str> {
    use std::sync::OnceLock;
    static FORCED: OnceLock<Option<&'static str>> = OnceLock::new();
    *FORCED.get_or_init(|| {
        std::env::var("ATLAS_DOCK_OPEN")
            .ok()
            .map(|s| &*Box::leak(s.into_boxed_str()))
    })
}

/// Bodies in the centered stack — **pinned only** (hover previews stay on-icon).
fn stack_ids(state: &DockState) -> Vec<&'static str> {
    state.pinned.clone()
}

fn panel_size_for(state: &DockState, id: &'static str, tokens: &DockTokens) -> Vec2 {
    state.panel_sizes.get(&id).copied().unwrap_or(Vec2::new(
        tokens.popover_width,
        (tokens.popover_max_height * 0.45).clamp(120.0, 280.0),
    ))
}

/// Lay out open panels: pack along the stack axis, then translate so the
/// group stays centered on the canvas edge. Relative order follows icons.
fn layout_panel_origins(
    side: DockSide,
    open: &[(&'static str, Rect, Vec2)],
    tokens: &DockTokens,
    canvas: Rect,
) -> HashMap<&'static str, Pos2> {
    let mut origins = HashMap::new();
    if open.is_empty() {
        return origins;
    }

    match side {
        DockSide::LeftCenter => {
            // Stack vertically; each panel wants y â‰ˆ its icon.top.
            let mut packed: Vec<(usize, f32, Vec2)> = open
                .iter()
                .enumerate()
                .map(|(i, (_, icon, size))| (i, icon.top(), *size))
                .collect();
            packed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let mut cursor_y = packed[0].1;
            let mut placed_y = Vec::with_capacity(packed.len());
            for &(_, prefer_y, size) in &packed {
                let y = prefer_y.max(cursor_y);
                placed_y.push(y);
                cursor_y = y + size.y + tokens.stack_gap;
            }
            let group_top = placed_y[0];
            let group_bottom =
                placed_y.last().unwrap() + packed.last().map(|p| p.2.y).unwrap_or(0.0);
            let group_mid = (group_top + group_bottom) * 0.5;
            let shift = canvas.center().y - group_mid;
            let x = open
                .iter()
                .map(|(_, icon, _)| icon.right())
                .fold(f32::NEG_INFINITY, f32::max)
                + tokens.popover_gap;

            for (slot, &(i, _, _)) in packed.iter().enumerate() {
                let id = open[i].0;
                origins.insert(id, Pos2::new(x, placed_y[slot] + shift));
            }
        }
        DockSide::BottomCenter => {
            // Stack horizontally; each panel wants x â‰ˆ its icon center.
            let mut packed: Vec<(usize, f32, Vec2)> = open
                .iter()
                .enumerate()
                .map(|(i, (_, icon, size))| (i, icon.center().x - size.x * 0.5, *size))
                .collect();
            packed.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            let mut cursor_x = packed[0].1;
            let mut placed_x = Vec::with_capacity(packed.len());
            for &(_, prefer_x, size) in &packed {
                let x = prefer_x.max(cursor_x);
                placed_x.push(x);
                cursor_x = x + size.x + tokens.stack_gap;
            }
            let group_left = placed_x[0];
            let group_right =
                placed_x.last().unwrap() + packed.last().map(|p| p.2.x).unwrap_or(0.0);
            let group_mid = (group_left + group_right) * 0.5;
            let shift = canvas.center().x - group_mid;
            let y = open
                .iter()
                .map(|(_, icon, _)| icon.top())
                .fold(f32::INFINITY, f32::min)
                - tokens.popover_gap;

            for (slot, &(i, _, _)) in packed.iter().enumerate() {
                let id = open[i].0;
                let size = open[i].2;
                // Pivot is CENTER_BOTTOM â€” pass the bottom-center anchor.
                origins.insert(id, Pos2::new(placed_x[slot] + size.x * 0.5 + shift, y));
            }
        }
    }
    origins
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

    let dt = ctx.input(|i| i.stable_dt).clamp(1.0 / 240.0, 1.0 / 20.0);

    // Drop pinned/hover entries whose icons disappeared.
    state.pinned.retain(|pid| {
        visible
            .iter()
            .any(|item| item.id == *pid && item.kind.opens_body())
    });
    if let Some(hover) = state.body_preview {
        if !visible
            .iter()
            .any(|item| item.id == hover && item.kind.opens_body())
        {
            state.body_preview = None;
        }
    }
    if let Some(hover) = state.label_hover {
        if !visible.iter().any(|item| {
            item.id == hover && matches!(item.kind, DockItemKind::Action | DockItemKind::Dashboard)
        }) {
            state.label_hover = None;
            state.label_hover_since = 0.0;
            state.describe_blend = 0.0;
        }
    }

    // Screenshot/dev harness and live tuner preview lock.
    if let Some(forced) = forced_open().or_else(crate::tuning::dock_preview_panel) {
        if visible
            .iter()
            .any(|item| item.id == forced && item.kind.opens_body())
            && !state.pinned.contains(&forced)
        {
            state.pinned.push(forced);
        }
    }

    // Reorder pinned to match icon strip order.
    let order: Vec<&'static str> = visible
        .iter()
        .filter(|item| item.kind.opens_body())
        .map(|item| item.id)
        .collect();
    state
        .pinned
        .sort_by_key(|pid| order.iter().position(|id| id == pid).unwrap_or(usize::MAX));

    // ---- icon strip ----
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

    let mut icon_rects: HashMap<&'static str, Rect> = HashMap::new();
    let mut body_preview_candidate: Option<&'static str> = None;
    let mut label_hover_candidate: Option<&'static str> = None;
    let bar_response = bar_area.show(ctx, |ui| {
        let mut draw_items = |ui: &mut egui::Ui| {
            ui.spacing_mut().item_spacing = Vec2::splat(tokens.icon_gap);
            for item in &visible {
                if item.gap_before {
                    ui.add_space(tokens.icon_gap * 1.5);
                }
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::splat(tokens.icon_size), Sense::click());
                icon_rects.insert(item.id, rect);
                let hovered = resp.hovered();
                let is_open = state.pinned.contains(&item.id)
                    || state.body_preview == Some(item.id)
                    || state.label_hover == Some(item.id);
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
                    if !state.pinned.contains(&item.id) {
                        match item.kind {
                            DockItemKind::Tool | DockItemKind::Dashboard => {
                                body_preview_candidate = Some(item.id);
                            }
                            DockItemKind::Action => {}
                        }
                    }
                    match item.kind {
                        DockItemKind::Action | DockItemKind::Dashboard => {
                            label_hover_candidate = Some(item.id);
                        }
                        DockItemKind::Tool => {}
                    }
                }
                if resp.clicked() {
                    clicked = Some(item.id);
                    state.last_inside_time = now;
                    if item.kind.opens_body() {
                        if let Some(idx) = state.pinned.iter().position(|p| *p == item.id) {
                            state.pinned.remove(idx);
                            state.panel_open.remove(&item.id);
                        } else {
                            state.pinned.push(item.id);
                            state.panel_open.insert(item.id, 0.0);
                        }
                        if state.body_preview == Some(item.id) {
                            state.body_preview = None;
                        }
                        if state.label_hover == Some(item.id) {
                            state.label_hover = None;
                            state.label_hover_since = 0.0;
                            state.describe_blend = 0.0;
                        }
                    }
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

    if let Some(id) = body_preview_candidate {
        if !state.pinned.contains(&id) {
            state.body_preview = Some(id);
        }
    } else {
        state.body_preview = None;
    }
    match label_hover_candidate {
        Some(id) => {
            if state.label_hover != Some(id) {
                state.label_hover = Some(id);
                state.label_hover_since = now;
                state.describe_blend = 0.0;
            }
        }
        None => {}
    }

    if let Some(label_id) = state.label_hover {
        if let Some(item) = visible.iter().find(|item| item.id == label_id) {
            let target = if item.kind == DockItemKind::Dashboard
                && !item.description.is_empty()
                && now - state.label_hover_since >= tokens.dashboard_describe_delay as f64
            {
                1.0
            } else {
                0.0
            };
            state.describe_blend = lerp_toward(
                state.describe_blend,
                target,
                dt,
                tokens.describe_fade_duration,
            );
        }
    } else {
        state.describe_blend = 0.0;
    }

    let mut anim_ids: Vec<&'static str> = state.pinned.clone();
    if let Some(p) = state.body_preview {
        if !state.pinned.contains(&p) {
            anim_ids.push(p);
        }
    }
    if let Some(l) = state.label_hover {
        if !anim_ids.contains(&l) {
            anim_ids.push(l);
        }
    }
    advance_panel_open(
        &mut state.panel_open,
        &anim_ids,
        dt,
        tokens.panel_open_duration,
    );

    // Partition between icons and canvas.
    paint_partition(
        &ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            state_id.with("partition"),
        )),
        bar_rect,
        canvas,
        side,
        &tokens,
        th.muted_text_color(),
    );

    // ---- Shared label chip (Action + Dashboard; above the icon) ----
    let mut label_chip_rect: Option<Rect> = None;
    if let Some(label_id) = state.label_hover {
        if let Some(&icon) = icon_rects.get(&label_id) {
            if let Some(item) = visible.iter().find(|item| item.id == label_id) {
                let (chip_pos, chip_pivot) = icon_popover_anchor(side, icon, tokens.hover_chip_gap);
                let chip_alpha =
                    ease_out_cubic(state.panel_open.get(&label_id).copied().unwrap_or(1.0));
                let chip = egui::Area::new(state_id.with("label_chip"))
                    .order(egui::Order::Foreground)
                    .pivot(chip_pivot)
                    .fixed_pos(chip_pos)
                    .constrain(false)
                    .show(ctx, |ui| {
                        ui.set_opacity(chip_alpha);
                        egui::Frame::new()
                            .fill(th.popover_fill_color())
                            .stroke(Stroke::new(1.0_f32, th.border_color()))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.set_max_width(220.0);
                                ui.label(RichText::new(item.label).strong().color(th.text_color()));
                                if item.kind == DockItemKind::Dashboard
                                    && !item.description.is_empty()
                                    && state.describe_blend > 0.001
                                {
                                    ui.add_space(2.0);
                                    ui.label(RichText::new(item.description).small().color(
                                        th.muted_text_color().gamma_multiply(state.describe_blend),
                                    ));
                                }
                            });
                    });
                label_chip_rect = Some(chip.response.rect);
                if pointer_in_rect(ctx, chip.response.rect) {
                    state.last_inside_time = now;
                }
            }
        }
    }

    let mut union_panels = Rect::NOTHING;
    let mut new_sizes: HashMap<&'static str, Vec2> = HashMap::new();
    let pointer = ctx.pointer_latest_pos();

    // ---- Hover preview panel (on-icon; does not join the centered stack) ----
    if let Some(preview_id) = state.body_preview {
        if !state.pinned.contains(&preview_id) {
            if let Some(&icon) = icon_rects.get(&preview_id) {
                let label = visible
                    .iter()
                    .find(|item| item.id == preview_id)
                    .map(|item| item.label.to_owned())
                    .unwrap_or_default();
                let max_h = tokens
                    .popover_max_height
                    .min((canvas.height() - 60.0).max(120.0));
                let gap = tokens.popover_gap + tokens.hover_chip_gap;
                let (origin, pivot) = icon_popover_anchor(side, icon, gap);
                let open =
                    ease_out_cubic(state.panel_open.get(&preview_id).copied().unwrap_or(0.0));
                let panel_area = egui::Area::new(state_id.with(("preview", preview_id)))
                    .order(egui::Order::Foreground)
                    .pivot(pivot)
                    .fixed_pos(origin)
                    .constrain_to(ctx.screen_rect());
                let response = panel_area.show(ctx, |ui| {
                    ui.set_opacity(open);
                    popover_frame(&tokens, th).show(ui, |ui| {
                        ui.set_width(
                            (tokens.popover_width - tokens.popover_padding * 2.0).max(1.0),
                        );
                        ui.label(RichText::new(label).small().strong().color(th.text_color()));
                        ui.separator();
                        ScrollArea::vertical()
                            .max_height(max_h)
                            .show(ui, |ui| panel_body(ui, preview_id));
                    });
                });
                let panel_rect = response.response.rect;
                new_sizes.insert(preview_id, panel_rect.size());
                union_panels = panel_rect;
                if pointer.is_some_and(|p| panel_rect.contains(p)) {
                    state.last_inside_time = now;
                }
            }
        }
    }

    // ---- Centered stack (pinned panels only) ----
    let open = stack_ids(&state);
    let open_meta: Vec<(&'static str, Rect, Vec2)> = open
        .iter()
        .filter_map(|oid| {
            let icon = *icon_rects.get(oid)?;
            let size = panel_size_for(&state, oid, &tokens);
            Some((*oid, icon, size))
        })
        .collect();
    let origins = layout_panel_origins(side, &open_meta, &tokens, canvas);

    let mut tracer_for: Option<(&'static str, Rect, Rect)> = None;

    for oid in &open {
        let Some(&origin) = origins.get(oid) else {
            continue;
        };
        let label = visible
            .iter()
            .find(|item| item.id == *oid)
            .map(|item| item.label.to_owned())
            .unwrap_or_default();
        let pinned = state.pinned.contains(oid);
        let max_h = tokens
            .popover_max_height
            .min((canvas.height() - 60.0).max(120.0));
        let panel_area = match side {
            DockSide::LeftCenter => egui::Area::new(state_id.with(("panel", *oid)))
                .order(egui::Order::Foreground)
                .pivot(Align2::LEFT_TOP)
                .fixed_pos(origin)
                .constrain_to(ctx.screen_rect()),
            DockSide::BottomCenter => egui::Area::new(state_id.with(("panel", *oid)))
                .order(egui::Order::Foreground)
                .pivot(Align2::CENTER_BOTTOM)
                .fixed_pos(origin)
                .constrain_to(ctx.screen_rect()),
        };
        let open_anim = ease_out_cubic(state.panel_open.get(oid).copied().unwrap_or(1.0));
        let response = panel_area.show(ctx, |ui| {
            ui.set_opacity(open_anim);
            popover_frame(&tokens, th).show(ui, |ui| {
                ui.set_width((tokens.popover_width - tokens.popover_padding * 2.0).max(1.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().strong().color(th.text_color()));
                    if pinned {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("pinned").small().color(th.muted_text_color()));
                        });
                    }
                });
                ui.separator();
                ScrollArea::vertical()
                    .max_height(max_h)
                    .show(ui, |ui| panel_body(ui, *oid));
            });
        });
        let panel_rect = response.response.rect;
        new_sizes.insert(*oid, panel_rect.size());
        if union_panels == Rect::NOTHING {
            union_panels = panel_rect;
        } else {
            union_panels = union_panels.union(panel_rect);
        }

        if let Some(p) = pointer {
            if border_hovered(panel_rect, p, tokens.tracer_border_hit) {
                if let Some(&icon_rect) = icon_rects.get(oid) {
                    tracer_for = Some((*oid, icon_rect, panel_rect));
                }
            }
        }
        if pointer.is_some_and(|p| panel_rect.contains(p)) {
            state.last_inside_time = now;
        }
    }
    state.panel_sizes = new_sizes;

    // Hover tracer: faint orthogonal wire from panel border back to icon.
    if let Some((_id, icon_rect, panel_rect)) = tracer_for {
        let (from, to) = match side {
            DockSide::LeftCenter => (icon_rect.right_center(), panel_rect.left_center()),
            DockSide::BottomCenter => (
                Pos2::new(icon_rect.center().x, icon_rect.top()),
                Pos2::new(panel_rect.center().x, panel_rect.bottom()),
            ),
        };
        let pts = orthogonal_route(from, to, side);
        let stroke = Stroke::new(
            tokens.tracer_width,
            th.muted_text_color().gamma_multiply(tokens.tracer_opacity),
        );
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            state_id.with("tracer"),
        ));
        rounded_route(&painter, &pts, tokens.tracer_corner_radius, stroke);
        ctx.request_repaint();
    }

    // ---- close behavior ----
    let pointer_inside = pointer.is_some_and(|p| {
        bar_rect.expand(4.0).contains(p)
            || (union_panels != Rect::NOTHING && union_panels.expand(2.0).contains(p))
            || label_chip_rect.is_some_and(|r| r.expand(2.0).contains(p))
    });
    if pointer_inside {
        state.last_inside_time = now;
    } else if body_preview_candidate.is_none() && label_hover_candidate.is_none() {
        let hover_expired = now - state.last_inside_time > tokens.close_delay as f64;
        if hover_expired {
            state.body_preview = None;
            state.label_hover = None;
            state.label_hover_since = 0.0;
            state.describe_blend = 0.0;
        }
    }

    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let outside_click = ctx.input(|i| i.pointer.any_click()) && !pointer_inside;
    if escape {
        state = DockState::default();
    } else if outside_click {
        if state.body_preview.is_some() || state.label_hover.is_some() {
            state.body_preview = None;
            state.label_hover = None;
            state.label_hover_since = 0.0;
            state.describe_blend = 0.0;
        } else {
            state.pinned.clear();
            state.panel_open.clear();
        }
    }

    if !state.pinned.is_empty()
        || state.body_preview.is_some()
        || state.label_hover.is_some()
        || state.describe_blend > 0.001
    {
        ctx.request_repaint();
    }
    ctx.data_mut(|d| d.insert_temp(state_id, state));

    clicked
}

fn pointer_in_rect(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.pointer_latest_pos().is_some_and(|p| rect.contains(p))
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

    #[test]
    fn dock_side_labels() {
        assert_eq!(DockSide::LeftCenter.label(), "Left edge");
        assert_eq!(DockSide::BottomCenter.label(), "Bottom edge");
    }
}
