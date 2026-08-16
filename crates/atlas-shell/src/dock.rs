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
    Mode,
    Workflow,
    Ai,
    Tags,
    Selection,
    View,
    Lens,
    /// 3×3 lattice — show-grid / document grid.
    Grid,
    /// Grid with a snap tick — snap-to-grid.
    SnapGrid,
    /// Magnet — object-snap master.
    Osnap,
    SnapEnd,
    SnapMid,
    SnapCenter,
    SnapNear,
    SnapInt,
    SnapQuad,
    SnapPerp,
    SnapTan,
    Swap,
    Reset,
    Fit,
    Dark,
    Ghost,
    Hide,
    Duplicates,
    Custom(DockIconPainter),
}

/// How an icon responds to interaction.
///
/// Apps should list **Tool** icons as one contiguous group and **Dashboard**
/// icons as another (neighbors by order only â€” no visible separator).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockItemKind {
    /// Settings dashboards (filters, tags, display…). Hover → title chip;
    /// click → volatile body; double-click → pin. See `TOOLBARS.md`.
    Dashboard,
    /// Tool flyouts (shapes, curves, nav…). Same hover / click / pin model.
    Tool,
    /// Click fires an action; no body. Hover shows the title chip above the icon.
    Action,
}

impl DockItemKind {
    pub fn opens_body(self) -> bool {
        matches!(self, Self::Dashboard | Self::Tool)
    }
}

/// How an open dock body lays out its members. Every panel that opens a
/// body (tool or dashboard) can switch between the stacked list and a
/// free-space icon strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DockBodyLayout {
    #[default]
    List,
    Icons,
}

/// One row / squircle in a tool flyout ([`flyout_items`]).
pub struct FlyoutItem<'a> {
    pub id: &'static str,
    pub label: &'a str,
    pub description: &'a str,
    pub hotkey: Option<&'a str>,
    pub icon: DockIcon,
    pub active: bool,
}

const BODY_LAYOUT_ID: &str = "atlas_dock_body_layout";
const DOCK_SIDE_ID: &str = "atlas_dock_side";
const STRIP_BUDGET_ID: &str = "atlas_dock_strip_budget";
/// List-toggle + gap + minimize, reserved on the primary row of a free strip.
const STRIP_CONTROLS_W: f32 = 44.0;

/// Layout the current panel body should use. Set while `floating_dock`
/// paints a tool flyout; defaults to the stacked list.
pub fn current_body_layout(ctx: &egui::Context) -> DockBodyLayout {
    ctx.data(|d| d.get_temp(egui::Id::new(BODY_LAYOUT_ID)))
        .unwrap_or(DockBodyLayout::List)
}

fn current_dock_side(ctx: &egui::Context) -> DockSide {
    ctx.data(|d| d.get_temp(egui::Id::new(DOCK_SIDE_ID)))
        .unwrap_or(DockSide::BottomCenter)
}

/// Title chips only while the pointer is on the icon itself. An open
/// flyout (pinned above the strip, or volatile in front of it) owns hover.
fn chip_from_icon_hover(icon_hovered: bool, panel_owns_pointer: bool) -> bool {
    icon_hovered && !panel_owns_pointer
}

fn panel_owns_pointer(last_union: Option<Rect>, pointer: Option<Pos2>) -> bool {
    last_union.is_some_and(|r| pointer.is_some_and(|p| r.expand(2.0).contains(p)))
}

pub struct DockItem<'a> {
    /// Stable id, returned on click and passed to the panel body callback.
    pub id: &'static str,
    /// Human name: popover header / name chip / tooltip.
    pub label: &'a str,
    /// Longer blurb shown after a short linger on the title chip (any kind).
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
    /// Click-pinned dashboards and tools, in icon order. Pins are persistent:
    /// outside clicks never clear them (only re-clicking the icon unpins),
    /// and apps may persist them across sessions via [`seed_pinned`] /
    /// [`pinned_ids`] + `ChromePrefs`.
    pinned: Vec<&'static str>,
    /// Dock-wide body layout: every pinned (and volatile) palette uses the
    /// free-space icon strip when true. Toggling any list control, or the
    /// single strip-mode glyph, flips this for the whole dock.
    icon_strip: bool,
    /// Volatile body from a single click (anchored on the icon; not in the
    /// stack). Retires on minimize, Escape, outside click, or close-delay.
    body_preview: Option<&'static str>,
    /// Title chip while hovering any icon (above the spawning icon).
    label_hover: Option<&'static str>,
    label_hover_since: f64,
    /// After a click / double-click pin, suppress the title chip until the
    /// pointer leaves the icon strip. Without this the chip snaps back under
    /// a still-hovering cursor the same frame the pin lands.
    label_suppress: bool,
    /// 0..1 fade for Dashboard description text (smooth, not a hard toggle).
    describe_blend: f32,
    /// Ease-in for pinned stack panels and hover previews (id → 0..1).
    panel_open: HashMap<&'static str, f32>,
    last_inside_time: f64,
    /// Last-frame measured popover sizes for stack centering.
    panel_sizes: HashMap<&'static str, Vec2>,
    /// Adaptive body width per panel: grown from the token width while the
    /// body's content would need to scroll (see [`adapt_panel_width`]).
    panel_widths: HashMap<&'static str, f32>,
    /// Last-frame body content height per panel. Drives the grow-vs-scroll
    /// decision: an egui `Area` locks its `max_rect` to the previous size, so
    /// a always-on `ScrollArea` traps expanded fold sections forever. We only
    /// scroll once content has actually overflowed the canvas budget.
    panel_content_h: HashMap<&'static str, f32>,
    /// Whether persisted pins were already restored this session.
    seeded: bool,
    /// Previous frame's union of open panel rects. A bottom-anchored panel
    /// shrinks upward when a fold collapses, so the pointer can sit outside
    /// the new rect on the same click that collapsed it — without this,
    /// that click is read as an outside dismiss.
    last_union_panels: Option<Rect>,
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// How hard hover / active icon fills lean toward their token colors.
/// Keep these low — selected palette icons should barely shift.
const ICON_HOVER_MIX: f32 = 0.14;
const ICON_ACTIVE_MIX: f32 = 0.18;
/// Title-chip translucency (on top of the open animation).
const HOVER_CHIP_OPACITY: f32 = 0.78;

fn mix_icon_fill(base: Color32, accent: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (base.r() as f32 + (accent.r() as f32 - base.r() as f32) * t).round() as u8,
        (base.g() as f32 + (accent.g() as f32 - base.g() as f32) * t).round() as u8,
        (base.b() as f32 + (accent.b() as f32 - base.b() as f32) * t).round() as u8,
        (base.a() as f32 + (accent.a() as f32 - base.a() as f32) * t).round() as u8,
    )
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

pub(crate) fn squircle_points(rect: Rect, exponent: f32, samples: usize) -> Vec<Pos2> {
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

pub(crate) fn paint_squircle(
    painter: &egui::Painter,
    rect: Rect,
    fill: Color32,
    stroke: Stroke,
    n: f32,
) {
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
        DockIcon::Mode => {
            let view = Rect::from_min_max(pt(rect, 0.18, 0.24), pt(rect, 0.50, 0.58));
            let edit = Rect::from_min_max(pt(rect, 0.50, 0.42), pt(rect, 0.82, 0.76));
            painter.rect_stroke(view, 2.0, s, egui::StrokeKind::Inside);
            painter.rect_stroke(edit, 2.0, s, egui::StrokeKind::Inside);
            painter.line_segment([pt(rect, 0.57, 0.49), pt(rect, 0.75, 0.67)], s);
            painter.line_segment([pt(rect, 0.73, 0.50), pt(rect, 0.56, 0.67)], s);
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
        DockIcon::Grid => {
            for f in [0.38, 0.62] {
                painter.line_segment([pt(rect, 0.16, f), pt(rect, 0.84, f)], s);
                painter.line_segment([pt(rect, f, 0.16), pt(rect, f, 0.84)], s);
            }
            painter.rect_stroke(
                Rect::from_min_max(pt(rect, 0.16, 0.16), pt(rect, 0.84, 0.84)),
                1.0,
                s,
                egui::StrokeKind::Inside,
            );
        }
        DockIcon::SnapGrid => {
            for f in [0.38, 0.62] {
                painter.line_segment([pt(rect, 0.16, f), pt(rect, 0.84, f)], s);
                painter.line_segment([pt(rect, f, 0.16), pt(rect, f, 0.84)], s);
            }
            painter.rect_stroke(
                Rect::from_min_max(pt(rect, 0.16, 0.16), pt(rect, 0.84, 0.84)),
                1.0,
                s,
                egui::StrokeKind::Inside,
            );
            painter.circle_filled(pt(rect, 0.38, 0.38), rect.width() * 0.055, color);
        }
        DockIcon::Osnap => {
            painter.add(Shape::line(
                vec![
                    pt(rect, 0.30, 0.20),
                    pt(rect, 0.30, 0.52),
                    pt(rect, 0.38, 0.68),
                    pt(rect, 0.54, 0.72),
                    pt(rect, 0.68, 0.62),
                    pt(rect, 0.72, 0.44),
                    pt(rect, 0.72, 0.20),
                ],
                s,
            ));
            painter.line_segment([pt(rect, 0.24, 0.28), pt(rect, 0.38, 0.28)], s);
            painter.line_segment([pt(rect, 0.64, 0.28), pt(rect, 0.80, 0.28)], s);
            painter.circle_filled(pt(rect, 0.51, 0.88), rect.width() * 0.05, color);
        }
        DockIcon::SnapEnd => {
            painter.line_segment([pt(rect, 0.18, 0.74), pt(rect, 0.70, 0.24)], s);
            let end = pt(rect, 0.70, 0.24);
            painter.rect_filled(
                Rect::from_center_size(end, Vec2::splat(rect.width() * 0.14)),
                1.0,
                color,
            );
        }
        DockIcon::SnapMid => {
            painter.line_segment([pt(rect, 0.16, 0.50), pt(rect, 0.84, 0.50)], s);
            painter.circle_filled(pt(rect, 0.50, 0.50), rect.width() * 0.08, color);
        }
        DockIcon::SnapCenter => {
            painter.circle_stroke(rect.center(), rect.width() * 0.28, s);
            let thin = Stroke::new(s.width * 0.75, color);
            painter.line_segment([pt(rect, 0.50, 0.28), pt(rect, 0.50, 0.72)], thin);
            painter.line_segment([pt(rect, 0.28, 0.50), pt(rect, 0.72, 0.50)], thin);
        }
        DockIcon::SnapNear => {
            painter.add(Shape::line(
                vec![
                    pt(rect, 0.16, 0.72),
                    pt(rect, 0.34, 0.38),
                    pt(rect, 0.58, 0.28),
                    pt(rect, 0.84, 0.42),
                ],
                s,
            ));
            painter.circle_filled(pt(rect, 0.58, 0.28), rect.width() * 0.07, color);
        }
        DockIcon::SnapInt => {
            painter.line_segment([pt(rect, 0.20, 0.22), pt(rect, 0.80, 0.78)], s);
            painter.line_segment([pt(rect, 0.80, 0.22), pt(rect, 0.20, 0.78)], s);
            painter.circle_filled(rect.center(), rect.width() * 0.07, color);
        }
        DockIcon::SnapQuad => {
            painter.circle_stroke(rect.center(), rect.width() * 0.28, s);
            for (x, y) in [(0.50, 0.18), (0.82, 0.50), (0.50, 0.82), (0.18, 0.50)] {
                painter.circle_filled(pt(rect, x, y), rect.width() * 0.045, color);
            }
        }
        DockIcon::SnapPerp => {
            painter.line_segment([pt(rect, 0.20, 0.78), pt(rect, 0.82, 0.78)], s);
            painter.line_segment([pt(rect, 0.36, 0.78), pt(rect, 0.36, 0.20)], s);
            painter.rect_stroke(
                Rect::from_min_max(pt(rect, 0.36, 0.62), pt(rect, 0.52, 0.78)),
                0.5,
                s,
                egui::StrokeKind::Inside,
            );
        }
        DockIcon::SnapTan => {
            painter.circle_stroke(pt(rect, 0.38, 0.52), rect.width() * 0.22, s);
            painter.line_segment([pt(rect, 0.58, 0.18), pt(rect, 0.58, 0.84)], s);
        }
        DockIcon::Swap => {
            painter.add(Shape::line(
                vec![
                    pt(rect, 0.22, 0.38),
                    pt(rect, 0.70, 0.38),
                    pt(rect, 0.58, 0.26),
                ],
                s,
            ));
            painter.add(Shape::line(
                vec![
                    pt(rect, 0.78, 0.62),
                    pt(rect, 0.30, 0.62),
                    pt(rect, 0.42, 0.74),
                ],
                s,
            ));
        }
        DockIcon::Reset => {
            painter.add(Shape::line(
                vec![
                    pt(rect, 0.70, 0.28),
                    pt(rect, 0.72, 0.50),
                    pt(rect, 0.50, 0.72),
                    pt(rect, 0.28, 0.58),
                    pt(rect, 0.30, 0.36),
                ],
                s,
            ));
            painter.line_segment([pt(rect, 0.70, 0.28), pt(rect, 0.56, 0.22)], s);
            painter.line_segment([pt(rect, 0.70, 0.28), pt(rect, 0.78, 0.40)], s);
        }
        DockIcon::Fit => {
            let box_r = Rect::from_min_max(pt(rect, 0.28, 0.28), pt(rect, 0.72, 0.72));
            painter.rect_stroke(box_r, 1.0, s, egui::StrokeKind::Inside);
            painter.line_segment([pt(rect, 0.16, 0.16), pt(rect, 0.30, 0.30)], s);
            painter.line_segment([pt(rect, 0.84, 0.16), pt(rect, 0.70, 0.30)], s);
            painter.line_segment([pt(rect, 0.16, 0.84), pt(rect, 0.30, 0.70)], s);
            painter.line_segment([pt(rect, 0.84, 0.84), pt(rect, 0.70, 0.70)], s);
        }
        DockIcon::Dark => {
            painter.circle_stroke(pt(rect, 0.42, 0.50), rect.width() * 0.26, s);
            painter.add(Shape::line(
                vec![
                    pt(rect, 0.50, 0.26),
                    pt(rect, 0.62, 0.36),
                    pt(rect, 0.66, 0.50),
                    pt(rect, 0.62, 0.64),
                    pt(rect, 0.50, 0.74),
                ],
                s,
            ));
        }
        DockIcon::Ghost => {
            let r = Rect::from_min_max(pt(rect, 0.20, 0.22), pt(rect, 0.80, 0.78));
            dashed_rect(painter, r, s);
        }
        DockIcon::Hide => {
            painter.add(Shape::ellipse_stroke(
                rect.center(),
                Vec2::new(rect.width() * 0.34, rect.height() * 0.20),
                s,
            ));
            painter.circle_filled(rect.center(), rect.width() * 0.06, color);
            painter.line_segment([pt(rect, 0.22, 0.78), pt(rect, 0.78, 0.22)], s);
        }
        DockIcon::Duplicates => {
            let a = Rect::from_min_max(pt(rect, 0.18, 0.22), pt(rect, 0.62, 0.66));
            let b = Rect::from_min_max(pt(rect, 0.38, 0.34), pt(rect, 0.82, 0.78));
            painter.rect_stroke(a, 1.0, s, egui::StrokeKind::Inside);
            painter.rect_stroke(b, 1.0, s, egui::StrokeKind::Inside);
        }
    }
}

fn dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let pts = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    for pair in pts.windows(2) {
        dashed_segment(painter, pair[0], pair[1], stroke);
    }
}

fn dashed_segment(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    let delta = b - a;
    let len = delta.length();
    if len < 0.5 {
        return;
    }
    let dir = delta / len;
    let dash = 3.5_f32;
    let gap = 2.5_f32;
    let mut t = 0.0;
    while t < len {
        let t1 = (t + dash).min(len);
        painter.line_segment([a + dir * t, a + dir * t1], stroke);
        t += dash + gap;
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

/// Currently pinned panel ids, in icon order — for persisting to prefs.
/// `None` until the dock has rendered at least once this session (callers
/// must not treat "no state yet" as "no pins" or they will wipe saved pins).
pub fn pinned_ids(ctx: &egui::Context, id: impl std::hash::Hash) -> Option<Vec<String>> {
    let state_id = egui::Id::new(("floating_dock", &id));
    ctx.data_mut(|d| d.get_temp::<DockState>(state_id))
        .filter(|s| s.seeded)
        .map(|s| s.pinned.iter().map(|p| (*p).to_owned()).collect())
}

/// Dock-wide icon-strip mode — for persisting to prefs.
/// `None` until the dock has rendered at least once this session.
/// A non-empty vec means strip mode (`["*"]`); empty means stacked lists.
pub fn icon_strip_ids(ctx: &egui::Context, id: impl std::hash::Hash) -> Option<Vec<String>> {
    let state_id = egui::Id::new(("floating_dock", &id));
    ctx.data_mut(|d| d.get_temp::<DockState>(state_id))
        .filter(|s| s.seeded)
        .map(|s| {
            if s.icon_strip {
                vec!["*".to_owned()]
            } else {
                Vec::new()
            }
        })
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

/// Gap kept between a panel and the canvas edge.
const PANEL_EDGE_MARGIN: f32 = 12.0;
/// Height of the caption row (label + minimize glyph) and the rule below it.
const PANEL_CAPTION_H: f32 = 22.0;
const PANEL_SEPARATOR_H: f32 = 10.0;
/// A panel body never gets less than this, even on a tiny canvas.
const PANEL_MIN_BODY_H: f32 = 120.0;
/// Fraction of the canvas a panel may grow across. Docks are palettes, not
/// workspaces.
const PANEL_MAX_WIDTH_FRAC: f32 = 0.42;
/// One widen/narrow increment of the width feedback loop.
const PANEL_WIDTH_STEP: f32 = 48.0;
/// Narrow only once the content uses less than this share of the height
/// budget. The gap between this and "overflows the budget" is the dead zone
/// that stops widen and narrow from fighting each frame.
const PANEL_SHRINK_FRACTION: f32 = 0.7;

fn panel_size_for(state: &DockState, id: &'static str, tokens: &DockTokens) -> Vec2 {
    if let Some(sz) = state.panel_sizes.get(&id) {
        return *sz;
    }
    if state.icon_strip {
        return Vec2::new(tokens.icon_size * 6.0, flyout_icon_size(tokens) + 12.0);
    }
    let width = state
        .panel_widths
        .get(&id)
        .copied()
        .unwrap_or(tokens.popover_width);
    Vec2::new(width, PANEL_MIN_BODY_H * 2.0)
}

/// Widest a panel may grow to.
fn panel_max_width(canvas: Rect, tokens: &DockTokens) -> f32 {
    (canvas.width() * PANEL_MAX_WIDTH_FRAC).max(tokens.popover_width)
}

/// This panel's current body width, clamped to what the canvas allows.
fn panel_width(state: &DockState, id: &'static str, tokens: &DockTokens, canvas: Rect) -> f32 {
    state
        .panel_widths
        .get(&id)
        .copied()
        .unwrap_or(tokens.popover_width)
        .clamp(tokens.popover_width, panel_max_width(canvas, tokens))
}

/// Vertical space a panel body may use before it has to scroll: the canvas
/// minus the edge margins and the panel's own chrome.
fn panel_body_max_height(canvas: Rect, tokens: &DockTokens) -> f32 {
    let chrome = tokens.popover_padding * 2.0 + PANEL_CAPTION_H + PANEL_SEPARATOR_H;
    (canvas.height() - PANEL_EDGE_MARGIN * 2.0 - chrome).max(PANEL_MIN_BODY_H)
}

/// One step of the panel width feedback loop. A pinned panel with several fold
/// sections expanded overflows its height budget; widening reflows wrapped
/// rows (chip flows, label + control rows) so the complement of open sections
/// fits without a scrollbar.
fn adapt_panel_width(current: f32, content_h: f32, body_max_h: f32, min_w: f32, max_w: f32) -> f32 {
    let max_w = max_w.max(min_w);
    let current = current.clamp(min_w, max_w);
    if content_h > body_max_h + 1.0 {
        (current + PANEL_WIDTH_STEP).min(max_w)
    } else if content_h < body_max_h * PANEL_SHRINK_FRACTION {
        (current - PANEL_WIDTH_STEP).max(min_w)
    } else {
        current
    }
}

/// Lay out open panels in **primary-icon order** (`open` is already sorted
/// that way). Pack along the stack axis, then translate so the group stays
/// centered on the canvas edge. Never re-sort by preferred alignment — a
/// wide strip's desired left can sit left of an earlier icon and shuffle
/// the band relative to the dock.
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
            let mut cursor_y = 0.0;
            let mut placed: Vec<(&'static str, f32, Vec2)> = Vec::with_capacity(open.len());
            for &(id, _, size) in open {
                placed.push((id, cursor_y, size));
                cursor_y += size.y + tokens.stack_gap;
            }
            let group_h = (cursor_y - tokens.stack_gap).max(0.0);
            let shift = canvas.center().y - group_h * 0.5;
            let x = open
                .iter()
                .map(|(_, icon, _)| icon.right())
                .fold(f32::NEG_INFINITY, f32::max)
                + tokens.popover_gap;

            for (id, y, size) in placed {
                let top = canvas.top() + PANEL_EDGE_MARGIN;
                let bottom = (canvas.bottom() - PANEL_EDGE_MARGIN - size.y).max(top);
                origins.insert(id, Pos2::new(x, (y + shift).clamp(top, bottom)));
            }
        }
        DockSide::BottomCenter => {
            let mut cursor_x = 0.0;
            let mut placed: Vec<(&'static str, f32, Vec2)> = Vec::with_capacity(open.len());
            for &(id, _, size) in open {
                placed.push((id, cursor_x, size));
                cursor_x += size.x + tokens.stack_gap;
            }
            let group_w = (cursor_x - tokens.stack_gap).max(0.0);
            let shift = canvas.center().x - group_w * 0.5;
            let y = open
                .iter()
                .map(|(_, icon, _)| icon.top())
                .fold(f32::INFINITY, f32::min)
                - tokens.popover_gap;

            for (id, x, size) in placed {
                let half = size.x * 0.5;
                let left = canvas.left() + PANEL_EDGE_MARGIN + half;
                let right = (canvas.right() - PANEL_EDGE_MARGIN - half).max(left);
                let cx = (x + half + shift).clamp(left, right);
                origins.insert(id, Pos2::new(cx, y));
            }
        }
    }
    origins
}

/// On-screen size of a flyout icon-strip squircle (65% of the primary dock).
pub fn flyout_icon_size(tokens: &DockTokens) -> f32 {
    tokens.icon_size * tokens.flyout_icon_scale
}

fn set_strip_budget(ctx: &egui::Context, budget: f32) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STRIP_BUDGET_ID), budget));
}

fn strip_budget(ctx: &egui::Context) -> Option<f32> {
    ctx.data(|d| d.get_temp(egui::Id::new(STRIP_BUDGET_ID)))
}

fn strip_icon_budget(side: DockSide, canvas: Rect, tokens: &DockTokens) -> f32 {
    let span = match side {
        DockSide::BottomCenter => canvas.width(),
        DockSide::LeftCenter => canvas.height(),
    };
    let min = flyout_icon_size(tokens);
    (span - PANEL_EDGE_MARGIN * 2.0 - STRIP_CONTROLS_W).max(min)
}

/// Hex-pack `count` icons. The primary line (bottom row / left column) fills
/// first; overflow lines stagger by half a pitch. Offsets are top-left of
/// each icon relative to the cluster origin (top-left of the bounding box).
fn hex_pack_offsets(
    count: usize,
    size: f32,
    gap: f32,
    budget: f32,
    side: DockSide,
) -> (Vec<Vec2>, Vec2) {
    if count == 0 {
        return (Vec::new(), Vec2::ZERO);
    }
    let pitch = size + gap;
    let cross = pitch * 3.0_f32.sqrt() / 2.0;
    let even_cap = (((budget - size) / pitch).floor() as i32 + 1).max(1) as usize;
    let odd_cap = (((budget - size - pitch * 0.5) / pitch).floor() as i32 + 1).max(1) as usize;

    let mut line_of = Vec::with_capacity(count);
    let mut slot_of = Vec::with_capacity(count);
    let mut i = 0;
    let mut line = 0usize;
    while i < count {
        let cap = if line % 2 == 0 { even_cap } else { odd_cap };
        let take = (count - i).min(cap);
        for k in 0..take {
            line_of.push(line);
            slot_of.push(k);
        }
        i += take;
        line += 1;
    }
    let n_lines = line;

    let mut max_along = size;
    let mut raw: Vec<Vec2> = Vec::with_capacity(count);
    for idx in 0..count {
        let li = line_of[idx];
        let k = slot_of[idx];
        let stagger = if li % 2 == 1 { pitch * 0.5 } else { 0.0 };
        let along = stagger + k as f32 * pitch;
        max_along = max_along.max(along + size);
        raw.push(Vec2::new(along, li as f32 * cross));
    }

    match side {
        DockSide::BottomCenter => {
            let width = max_along;
            let height = size + (n_lines.saturating_sub(1) as f32) * cross;
            let offsets = raw
                .into_iter()
                .map(|p| Vec2::new(p.x, height - size - p.y))
                .collect();
            (offsets, Vec2::new(width, height))
        }
        DockSide::LeftCenter => {
            let width = size + (n_lines.saturating_sub(1) as f32) * cross;
            let height = max_along;
            let offsets = raw
                .into_iter()
                .map(|p| Vec2::new(p.y, height - size - p.x))
                .collect();
            (offsets, Vec2::new(width, height))
        }
    }
}

/// Paint a flyout as a stacked list or a free-space hex-packed strip,
/// following [`current_body_layout`]. Hovering a strip icon shows the same
/// name + linger-description chip as the primary dock.
pub fn flyout_items(ui: &mut egui::Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    match current_body_layout(ui.ctx()) {
        DockBodyLayout::List => flyout_list(ui, items),
        DockBodyLayout::Icons => flyout_icon_strip(ui, items),
    }
}

fn flyout_list(ui: &mut egui::Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    let mut clicked = None;
    let ink = ui.visuals().text_color();
    let sub = ui.visuals().weak_text_color();
    for item in items {
        let row = ui.horizontal(|ui| {
            let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
            paint_builtin_icon(ui.painter(), icon_rect.shrink(1.0), item.icon, ink);
            let resp = ui.selectable_label(item.active, item.label);
            if let Some(key) = item.hotkey {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(key).small().color(sub));
                });
            }
            resp
        });
        if row.inner.clicked() {
            clicked = Some(item.id);
        }
    }
    clicked
}

fn flyout_icon_strip(ui: &mut egui::Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    let mut tokens = crate::tokens::current().dock;
    tokens.normalize();
    let th = if ui.visuals().dark_mode {
        &tokens.dark
    } else {
        &tokens.light
    };
    let size = flyout_icon_size(&tokens);
    let gap = (tokens.icon_gap * tokens.flyout_icon_scale).max(4.0);
    let side = current_dock_side(ui.ctx());
    let budget = strip_budget(ui.ctx()).unwrap_or_else(|| ui.available_width().max(size));
    let (offsets, cluster) = hex_pack_offsets(items.len(), size, gap, budget, side);
    let now = ui.ctx().input(|i| i.time);
    let dt = ui
        .ctx()
        .input(|i| i.stable_dt)
        .clamp(1.0 / 240.0, 1.0 / 20.0);
    let hover_id = ui.id().with("flyout_strip_hover");
    let mut hover: Option<(&'static str, f64, f32)> = ui.ctx().data_mut(|d| d.get_temp(hover_id));
    let mut clicked = None;
    let mut hovered_id: Option<&'static str> = None;
    let mut hovered_rect = Rect::NOTHING;

    let (origin, _) = ui.allocate_exact_size(cluster, Sense::hover());
    for (item, off) in items.iter().zip(offsets.iter()) {
        let rect = Rect::from_min_size(origin.min + *off, Vec2::splat(size));
        let resp = ui.interact(rect, ui.id().with(item.id), Sense::click());
        let hovered = resp.hovered();
        let base = th.icon_fill_color();
        let fill = if item.active {
            mix_icon_fill(base, th.icon_active_color(), ICON_ACTIVE_MIX)
        } else if hovered {
            mix_icon_fill(base, th.icon_hover_color(), ICON_HOVER_MIX)
        } else {
            base
        };
        paint_squircle(
            ui.painter(),
            rect.shrink(0.5),
            fill,
            Stroke::new(1.0_f32, th.border_color()),
            tokens.squircle_exponent,
        );
        paint_builtin_icon(
            ui.painter(),
            rect.shrink((size * 0.20).max(3.0)),
            item.icon,
            th.text_color(),
        );
        if hovered {
            hovered_id = Some(item.id);
            hovered_rect = rect;
        }
        if resp.clicked() {
            clicked = Some(item.id);
        }
    }

    if let Some(id) = hovered_id {
        let (since, mut blend) = match hover {
            Some((hid, since, blend)) if hid == id => (since, blend),
            _ => (now, 0.0),
        };
        let item = items.iter().find(|item| item.id == id);
        let target = if item.is_some_and(|it| !it.description.is_empty())
            && now - since >= tokens.dashboard_describe_delay as f64
        {
            1.0
        } else {
            0.0
        };
        blend = lerp_toward(blend, target, dt, tokens.describe_fade_duration);
        hover = Some((id, since, blend));
        if let Some(item) = item {
            let side = current_dock_side(ui.ctx());
            let (pos, pivot) = icon_popover_anchor(side, hovered_rect, tokens.hover_chip_gap);
            let desc = if !item.description.is_empty() && blend > 0.001 {
                Some((item.description, blend))
            } else {
                None
            };
            show_hover_chip(
                ui.ctx(),
                ui.id().with(("flyout_chip", id)),
                pos,
                pivot,
                item.label,
                item.hotkey,
                desc,
                th,
            );
            if target > 0.0 || blend > 0.001 {
                ui.ctx().request_repaint();
            }
        }
    } else {
        hover = None;
    }
    ui.ctx().data_mut(|d| {
        if let Some(h) = hover {
            d.insert_temp(hover_id, h);
        } else {
            d.remove_temp::<(&'static str, f64, f32)>(hover_id);
        }
    });
    clicked
}

fn show_hover_chip(
    ctx: &egui::Context,
    id: egui::Id,
    pos: Pos2,
    pivot: Align2,
    label: &str,
    hotkey: Option<&str>,
    description: Option<(&str, f32)>,
    th: &DockThemeTokens,
) {
    egui::Area::new(id)
        .order(egui::Order::Tooltip)
        .pivot(pivot)
        .fixed_pos(pos)
        .constrain(false)
        .show(ctx, |ui| {
            ui.set_opacity(HOVER_CHIP_OPACITY);
            egui::Frame::new()
                .fill(th.popover_fill_color().gamma_multiply(0.82))
                .stroke(Stroke::new(1.0_f32, th.border_color().gamma_multiply(0.7)))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_max_width(240.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(label).strong().color(th.text_color()));
                        if let Some(key) = hotkey {
                            ui.label(RichText::new(key).small().color(th.muted_text_color()));
                        }
                    });
                    if let Some((text, blend)) = description {
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(text)
                                .small()
                                .color(th.muted_text_color().gamma_multiply(blend)),
                        );
                    }
                });
        });
}

/// Render a floating dock. Returns the id of a clicked icon, if any
/// (both Panel and Action items report clicks so apps can react).
///
/// `restore_pins`: panel ids to restore as pinned on the dock's first
/// rendered frame (persisted palettes, e.g. from `ChromePrefs`). Ids are
/// matched against `items`, so stale entries are ignored harmlessly. Read
/// the live set back with [`pinned_ids`] to persist changes.
/// `restore_icon_strips`: bodies that should open in free-space icon-strip mode.
#[allow(clippy::too_many_arguments)]
pub fn floating_dock(
    ctx: &egui::Context,
    id: impl std::hash::Hash,
    canvas: Rect,
    palette: &Palette,
    side: DockSide,
    items: &[DockItem<'_>],
    restore_pins: &[String],
    restore_icon_strips: &[String],
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

    // Restore persisted pins once per session. Matched against the full item
    // set so pins on currently-hidden icons survive until their icon returns.
    if !state.seeded {
        state.seeded = true;
        for sid in restore_pins {
            if let Some(item) = items
                .iter()
                .find(|item| item.id == sid && item.kind.opens_body())
            {
                if !state.pinned.contains(&item.id) {
                    state.pinned.push(item.id);
                    state.panel_open.insert(item.id, 0.0);
                }
            }
        }
        // Non-empty restore (legacy per-id lists or the `"*"` sentinel)
        // turns on dock-wide strip mode.
        state.icon_strip = restore_icon_strips.iter().any(|sid| {
            sid == "*"
                || items
                    .iter()
                    .any(|item| item.id == sid && item.kind.opens_body())
        });
    }

    let dt = ctx.input(|i| i.stable_dt).clamp(1.0 / 240.0, 1.0 / 20.0);

    // Pins survive icon invisibility (e.g. board tools while in another view):
    // they simply stop rendering until the icon returns. Only hover state is
    // validated against the current item set.
    if let Some(hover) = state.body_preview {
        if !visible
            .iter()
            .any(|item| item.id == hover && item.kind.opens_body())
        {
            state.body_preview = None;
        }
    }
    if let Some(hover) = state.label_hover {
        // Title chips are for unpinned icons only — a pinned panel already
        // owns the label in its caption, and a chip would sit on the icon.
        let still_ok = visible.iter().any(|item| {
            item.id == hover
                && !state.pinned.contains(&item.id)
                && state.body_preview != Some(item.id)
        });
        if !still_ok {
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

    let pointer = ctx.pointer_latest_pos();
    let panel_owns = panel_owns_pointer(state.last_union_panels, pointer);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(DOCK_SIDE_ID), side));

    let mut icon_rects: HashMap<&'static str, Rect> = HashMap::new();
    let mut label_hover_candidate: Option<&'static str> = None;
    // Pointer is over a pinned / volatile-open icon — previous chips must die
    // immediately (the bar still counts as "inside", so close-delay alone
    // would leave the prior name stuck).
    let mut label_blocked_by_open = false;
    let mut hovered_icon: Option<&'static str> = None;
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
                let hovered = chip_from_icon_hover(resp.hovered(), panel_owns);
                let is_pinned = state.pinned.contains(&item.id);
                let is_preview = state.body_preview == Some(item.id);
                // Selected / pinned / hover fills are a bare whisper over the
                // default — never a full-opacity swap that screams "active".
                let base = th.icon_fill_color();
                let fill = if item.active || is_pinned || is_preview {
                    mix_icon_fill(base, th.icon_active_color(), ICON_ACTIVE_MIX)
                } else if hovered {
                    mix_icon_fill(base, th.icon_hover_color(), ICON_HOVER_MIX)
                } else {
                    base
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
                    hovered_icon = Some(item.id);
                    // Title chip only when the panel isn't already open —
                    // pinned / volatile bodies carry their own caption.
                    if is_pinned || is_preview {
                        label_blocked_by_open = true;
                    } else {
                        label_hover_candidate = Some(item.id);
                    }
                }
                if item.kind.opens_body() && resp.double_clicked() {
                    clicked = Some(item.id);
                    state.last_inside_time = now;
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
                    // Pin/unpin under a still-hovering cursor must not flash
                    // the name chip back on top of the icon.
                    state.label_hover = None;
                    state.label_hover_since = 0.0;
                    state.describe_blend = 0.0;
                    state.label_suppress = true;
                } else if resp.clicked() {
                    clicked = Some(item.id);
                    state.last_inside_time = now;
                    if item.kind.opens_body() {
                        if state.pinned.contains(&item.id) {
                            // Already pinned — single click is a no-op (use ─).
                        } else if state.body_preview == Some(item.id) {
                            state.body_preview = None;
                            state.panel_open.remove(&item.id);
                        } else {
                            // Volatile menu.
                            state.body_preview = Some(item.id);
                            state.panel_open.insert(item.id, 0.0);
                        }
                        state.label_hover = None;
                        state.label_hover_since = 0.0;
                        state.describe_blend = 0.0;
                        state.label_suppress = true;
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

    // Hover only drives the title chip. Volatile bodies come from single-click.
    let _ = hovered_icon;
    if label_hover_candidate.is_none() && !label_blocked_by_open {
        // Pointer left every eligible icon — allow chips again next hover.
        // (Blocked-by-open keeps suppress so we don't flash a chip if the
        // user later drifts onto a non-pinned neighbor without leaving.)
        state.label_suppress = false;
    }
    if state.label_suppress || label_blocked_by_open || panel_owns {
        state.label_hover = None;
        state.label_hover_since = 0.0;
        state.describe_blend = 0.0;
    } else if let Some(id) = label_hover_candidate {
        if state.label_hover != Some(id) {
            state.label_hover = Some(id);
            state.label_hover_since = now;
            state.describe_blend = 0.0;
        }
    } else {
        // Left the icon — the chip dies now. Close-delay is only for
        // volatile *bodies*; a pinned flyout still counts as "inside"
        // and would otherwise leave the last name stuck on screen.
        state.label_hover = None;
        state.label_hover_since = 0.0;
        state.describe_blend = 0.0;
    }

    if let Some(label_id) = state.label_hover {
        if let Some(item) = visible.iter().find(|item| item.id == label_id) {
            // Linger: any icon with a description expands the chip after delay.
            let target = if !item.description.is_empty()
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
            if target > 0.0 || state.describe_blend > 0.001 {
                ctx.request_repaint();
            }
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

    // Primary hover chips paint *after* panels (same Tooltip order, later
    // Area wins) so a name tag never slips behind a pinned toolbar. See
    // the chip block below the stack.

    let mut union_panels = Rect::NOTHING;
    let mut new_sizes: HashMap<&'static str, Vec2> = HashMap::new();
    let mut new_widths: HashMap<&'static str, f32> = HashMap::new();
    let mut new_content_h: HashMap<&'static str, f32> = HashMap::new();
    let mut strip_hits: Vec<(&'static str, Rect, Rect)> = Vec::new();
    let mut tracer_for: Option<(&'static str, Rect, Rect)> = None;

    // ---- Hover preview panel (on-icon; does not join the centered stack) ----
    if let Some(preview_id) = state.body_preview {
        if !state.pinned.contains(&preview_id) {
            if let Some(&icon) = icon_rects.get(&preview_id) {
                let label = visible
                    .iter()
                    .find(|item| item.id == preview_id)
                    .map(|item| item.label.to_owned())
                    .unwrap_or_default();
                let body_max_h = panel_body_max_height(canvas, &tokens);
                let (origin, pivot) = icon_popover_anchor(side, icon, tokens.popover_gap);
                let open =
                    ease_out_cubic(state.panel_open.get(&preview_id).copied().unwrap_or(0.0));
                let width = panel_width(&state, preview_id, &tokens, canvas);
                let last_h = state
                    .panel_content_h
                    .get(&preview_id)
                    .copied()
                    .unwrap_or(0.0);
                let kind = visible
                    .iter()
                    .find(|item| item.id == preview_id)
                    .map(|item| item.kind)
                    .unwrap_or(DockItemKind::Dashboard);
                let layout = body_layout_for(&state, preview_id, kind);
                let last_size = state.panel_sizes.get(&preview_id).copied();
                let panel_area = dock_body_area(
                    state_id.with((
                        if layout == DockBodyLayout::Icons {
                            "preview_strip"
                        } else {
                            "preview"
                        },
                        preview_id,
                    )),
                    side,
                    origin,
                    pivot,
                    canvas,
                    layout,
                    width,
                    body_max_h,
                    last_size,
                );
                let render = show_dock_body(
                    ctx,
                    panel_area,
                    &tokens,
                    th,
                    &label,
                    false,
                    kind,
                    layout,
                    width,
                    body_max_h,
                    last_h,
                    open,
                    canvas,
                    side,
                    state.pinned.is_empty(),
                    |ui| {
                        set_body_layout(ui.ctx(), layout);
                        panel_body(ui, preview_id);
                    },
                );
                apply_layout_toggle(&mut state, preview_id, render.toggle_layout);
                if render.minimize {
                    state.body_preview = None;
                    state.panel_open.remove(&preview_id);
                }
                if layout != DockBodyLayout::Icons {
                    new_widths.insert(
                        preview_id,
                        adapt_panel_width(
                            width,
                            render.content_h,
                            body_max_h,
                            tokens.popover_width,
                            panel_max_width(canvas, &tokens),
                        ),
                    );
                }
                new_content_h.insert(preview_id, render.content_h);
                new_sizes.insert(preview_id, render.rect.size());
                union_panels = render.rect;
                if layout == DockBodyLayout::Icons {
                    if let Some(&icon) = icon_rects.get(&preview_id) {
                        strip_hits.push((preview_id, icon, render.rect));
                    }
                } else if let Some(p) = pointer {
                    if border_hovered(render.rect, p, tokens.tracer_border_hit) {
                        if let Some(&icon_rect) = icon_rects.get(&preview_id) {
                            tracer_for = Some((preview_id, icon_rect, render.rect));
                        }
                    }
                }
                if pointer.is_some_and(|p| render.rect.contains(p)) {
                    state.last_inside_time = now;
                    state.label_hover = None;
                    state.label_hover_since = 0.0;
                    state.describe_blend = 0.0;
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

    let mut pinned_strip_rects: Vec<Rect> = Vec::new();

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
        let body_max_h = panel_body_max_height(canvas, &tokens);
        let width = panel_width(&state, oid, &tokens, canvas);
        let last_h = state.panel_content_h.get(oid).copied().unwrap_or(0.0);
        let open_anim = ease_out_cubic(state.panel_open.get(oid).copied().unwrap_or(1.0));
        let kind = visible
            .iter()
            .find(|item| item.id == *oid)
            .map(|item| item.kind)
            .unwrap_or(DockItemKind::Dashboard);
        let layout = body_layout_for(&state, oid, kind);
        let last_size = state.panel_sizes.get(oid).copied();
        let pivot = match side {
            DockSide::LeftCenter => Align2::LEFT_TOP,
            DockSide::BottomCenter => Align2::CENTER_BOTTOM,
        };
        let panel_area = dock_body_area(
            state_id.with((
                if layout == DockBodyLayout::Icons {
                    "strip"
                } else {
                    "panel"
                },
                *oid,
            )),
            side,
            origin,
            pivot,
            canvas,
            layout,
            width,
            body_max_h,
            last_size,
        );
        let render = show_dock_body(
            ctx,
            panel_area,
            &tokens,
            th,
            &label,
            pinned,
            kind,
            layout,
            width,
            body_max_h,
            last_h,
            open_anim,
            canvas,
            side,
            false,
            |ui| {
                set_body_layout(ui.ctx(), layout);
                panel_body(ui, oid);
            },
        );
        apply_layout_toggle(&mut state, oid, render.toggle_layout);
        if render.minimize {
            if let Some(idx) = state.pinned.iter().position(|p| p == oid) {
                state.pinned.remove(idx);
            }
            state.panel_open.remove(oid);
        }
        if layout != DockBodyLayout::Icons {
            new_widths.insert(
                *oid,
                adapt_panel_width(
                    width,
                    render.content_h,
                    body_max_h,
                    tokens.popover_width,
                    panel_max_width(canvas, &tokens),
                ),
            );
        }
        new_content_h.insert(*oid, render.content_h);
        let panel_rect = render.rect;
        new_sizes.insert(*oid, panel_rect.size());
        if union_panels == Rect::NOTHING {
            union_panels = panel_rect;
        } else {
            union_panels = union_panels.union(panel_rect);
        }

        if layout == DockBodyLayout::Icons {
            pinned_strip_rects.push(panel_rect);
            if let Some(&icon_rect) = icon_rects.get(oid) {
                strip_hits.push((*oid, icon_rect, panel_rect));
            }
        } else if let Some(p) = pointer {
            if border_hovered(panel_rect, p, tokens.tracer_border_hit) {
                if let Some(&icon_rect) = icon_rects.get(oid) {
                    tracer_for = Some((*oid, icon_rect, panel_rect));
                }
            }
        }
        if pointer.is_some_and(|p| panel_rect.contains(p)) {
            state.last_inside_time = now;
            state.label_hover = None;
            state.label_hover_since = 0.0;
            state.describe_blend = 0.0;
        }
    }

    if state.icon_strip && !pinned_strip_rects.is_empty() {
        paint_strip_dividers(
            &ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                state_id.with("strip_dividers"),
            )),
            side,
            &pinned_strip_rects,
            th,
        );
        let band = pinned_strip_rects
            .iter()
            .copied()
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::NOTHING);
        if band != Rect::NOTHING {
            let size = flyout_icon_size(&tokens);
            let (toggle_pos, toggle_pivot) = match side {
                DockSide::BottomCenter => (
                    Pos2::new(band.right() + 14.0, strip_primary_center(band, size)),
                    Align2::LEFT_CENTER,
                ),
                DockSide::LeftCenter => (
                    Pos2::new(band.right() + 14.0, strip_primary_center(band, size)),
                    Align2::LEFT_CENTER,
                ),
            };
            let mut band_toggle = false;
            let toggle = egui::Area::new(state_id.with("strip_band_toggle"))
                .order(egui::Order::Tooltip)
                .pivot(toggle_pivot)
                .fixed_pos(toggle_pos)
                .constrain(false)
                .show(ctx, |ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(16.0, 14.0), Sense::hover());
                    band_toggle = paint_list_toggle(ui, rect, th);
                });
            apply_layout_toggle(&mut state, "", band_toggle);
            union_panels = if union_panels == Rect::NOTHING {
                toggle.response.rect
            } else {
                union_panels.union(toggle.response.rect)
            };
            if pointer_in_rect(ctx, toggle.response.rect) {
                state.last_inside_time = now;
            }
        }
    }

    if let Some(p) = pointer {
        if let Some(hit) = nearest_strip_at(p, &strip_hits, tokens.stack_gap.max(8.0)) {
            tracer_for = Some(hit);
        }
    }

    // Paint stack (back → front): bar Foreground, panels/strips Tooltip,
    // then this chip at Tooltip so a primary-icon name sits in front of
    // any pinned toolbar it would otherwise hide behind.
    let mut label_chip_rect: Option<Rect> = None;
    if let Some(label_id) = state.label_hover {
        if let Some(&icon) = icon_rects.get(&label_id) {
            if let Some(item) = visible.iter().find(|item| item.id == label_id) {
                let (chip_pos, chip_pivot) = icon_popover_anchor(side, icon, tokens.hover_chip_gap);
                let chip_alpha =
                    ease_out_cubic(state.panel_open.get(&label_id).copied().unwrap_or(1.0))
                        * HOVER_CHIP_OPACITY;
                let desc = if !item.description.is_empty() && state.describe_blend > 0.001 {
                    Some((item.description, state.describe_blend))
                } else {
                    None
                };
                let chip = egui::Area::new(state_id.with("label_chip"))
                    .order(egui::Order::Tooltip)
                    .pivot(chip_pivot)
                    .fixed_pos(chip_pos)
                    .constrain(false)
                    .show(ctx, |ui| {
                        ui.set_opacity(chip_alpha);
                        egui::Frame::new()
                            .fill(th.popover_fill_color().gamma_multiply(0.82))
                            .stroke(Stroke::new(1.0_f32, th.border_color().gamma_multiply(0.7)))
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.set_max_width(240.0);
                                ui.label(RichText::new(item.label).strong().color(th.text_color()));
                                if let Some((text, blend)) = desc {
                                    ui.add_space(2.0);
                                    ui.label(
                                        RichText::new(text)
                                            .small()
                                            .color(th.muted_text_color().gamma_multiply(blend)),
                                    );
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

    // Width / height / scroll-mode changes take effect next frame.
    let settling = new_widths.iter().any(|(id, w)| {
        state
            .panel_widths
            .get(id)
            .is_none_or(|prev| (prev - w).abs() > 0.5)
    }) || new_content_h.iter().any(|(id, h)| {
        state
            .panel_content_h
            .get(id)
            .is_none_or(|prev| (prev - h).abs() > 1.0)
    });
    state.panel_sizes = new_sizes;
    state.panel_widths = new_widths;
    state.panel_content_h = new_content_h;
    if settling {
        ctx.request_repaint();
    }

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
    //
    // Pinned panels are persistent tool palettes: canvas clicks and Escape
    // never dismiss them — only re-clicking the icon unpins. Transient hover
    // state (preview + label chip) survives the pointer's travel between icon
    // and panel and retires after a close-delay grace once abandoned.
    let hit_panels = match (union_panels == Rect::NOTHING, state.last_union_panels) {
        (true, None) => Rect::NOTHING,
        (true, Some(last)) => last,
        (false, None) => union_panels,
        (false, Some(last)) => union_panels.union(last),
    };
    let pointer_inside = pointer.is_some_and(|p| {
        bar_rect.expand(4.0).contains(p)
            || (hit_panels != Rect::NOTHING && hit_panels.expand(2.0).contains(p))
            || label_chip_rect.is_some_and(|r| r.expand(2.0).contains(p))
    });
    if pointer_inside {
        state.last_inside_time = now;
    } else if label_hover_candidate.is_none() {
        let hover_expired = now - state.last_inside_time > tokens.close_delay as f64;
        if hover_expired {
            // Volatile bodies and title chips retire; pins stay.
            state.body_preview = None;
            state.label_hover = None;
            state.label_hover_since = 0.0;
            state.describe_blend = 0.0;
        }
    }

    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let outside_click = ctx.input(|i| i.pointer.any_click()) && !pointer_inside;
    if escape || outside_click {
        state.body_preview = None;
        state.label_hover = None;
        state.label_hover_since = 0.0;
        state.describe_blend = 0.0;
    }

    if !state.pinned.is_empty()
        || state.body_preview.is_some()
        || state.label_hover.is_some()
        || state.describe_blend > 0.001
    {
        ctx.request_repaint();
    }
    state.last_union_panels = (union_panels != Rect::NOTHING).then_some(union_panels);
    ctx.data_mut(|d| d.insert_temp(state_id, state));

    clicked
}

fn pointer_in_rect(ctx: &egui::Context, rect: Rect) -> bool {
    ctx.pointer_latest_pos().is_some_and(|p| rect.contains(p))
}

/// Panel body in a thin floating scroll area. Returns the body's content
/// height, which drives [`adapt_panel_width`] on the next frame.
fn compact_panel_scroll(
    ui: &mut egui::Ui,
    max_h: f32,
    add_body: impl FnOnce(&mut egui::Ui),
) -> f32 {
    ui.scope(|ui| {
        let scroll = &mut ui.style_mut().spacing.scroll;
        scroll.floating = true;
        scroll.bar_width = 2.5;
        scroll.floating_width = 1.0;
        scroll.floating_allocated_width = 3.0;
        scroll.bar_inner_margin = 2.0;
        scroll.bar_outer_margin = 1.0;
        ScrollArea::vertical()
            .max_height(max_h)
            .drag_to_scroll(false)
            .auto_shrink([true, true])
            .show(ui, add_body)
            .content_size
            .y
    })
    .inner
}

/// Lay the body out without a ScrollArea so the parent [`egui::Area`] can grow
/// with expanded fold sections. Returns the measured content height.
fn panel_body_unsized(ui: &mut egui::Ui, add_body: impl FnOnce(&mut egui::Ui)) -> f32 {
    let top = ui.cursor().top();
    add_body(ui);
    (ui.cursor().top() - top).max(0.0)
}

/// Outcome of one rendered panel: what the user clicked, where it landed, and
/// how tall its content wanted to be.
struct PanelRender {
    minimize: bool,
    toggle_layout: bool,
    rect: Rect,
    content_h: f32,
}

fn body_layout_for(state: &DockState, _id: &str, kind: DockItemKind) -> DockBodyLayout {
    if kind.opens_body() && state.icon_strip {
        DockBodyLayout::Icons
    } else {
        DockBodyLayout::List
    }
}

fn apply_layout_toggle(state: &mut DockState, _id: &'static str, toggled: bool) {
    if toggled {
        state.icon_strip = !state.icon_strip;
    }
}

fn set_body_layout(ctx: &egui::Context, layout: DockBodyLayout) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(BODY_LAYOUT_ID), layout));
}

fn dock_body_area(
    id: egui::Id,
    _side: DockSide,
    origin: Pos2,
    pivot: Align2,
    canvas: Rect,
    layout: DockBodyLayout,
    width: f32,
    body_max_h: f32,
    last_size: Option<Vec2>,
) -> egui::Area {
    let default_size = if layout == DockBodyLayout::Icons {
        last_size.unwrap_or(Vec2::new(width.min(320.0), 48.0))
    } else {
        Vec2::new(width, body_max_h + PANEL_CAPTION_H + PANEL_SEPARATOR_H)
    };
    egui::Area::new(id)
        .order(egui::Order::Tooltip)
        .pivot(pivot)
        .fixed_pos(origin)
        .default_size(default_size)
        .constrain_to(canvas)
}

#[allow(clippy::too_many_arguments)]
fn show_dock_body(
    ctx: &egui::Context,
    area: egui::Area,
    tokens: &DockTokens,
    th: &DockThemeTokens,
    label: &str,
    pinned: bool,
    kind: DockItemKind,
    layout: DockBodyLayout,
    width: f32,
    body_max_h: f32,
    last_content_h: f32,
    open_anim: f32,
    canvas: Rect,
    side: DockSide,
    show_strip_toggle: bool,
    add_body: impl FnOnce(&mut egui::Ui),
) -> PanelRender {
    if layout == DockBodyLayout::Icons {
        set_strip_budget(ctx, strip_icon_budget(side, canvas, tokens));
        show_free_strip(
            ctx,
            area,
            tokens,
            th,
            pinned,
            show_strip_toggle,
            open_anim,
            add_body,
        )
    } else {
        show_panel(
            ctx,
            area,
            tokens,
            th,
            label,
            pinned,
            kind.opens_body(),
            layout,
            width,
            body_max_h,
            last_content_h,
            open_anim,
            add_body,
        )
    }
}

/// Borderless icon strip: squircles in free space, hex-packed. Minimize
/// sits on the right of this cluster. The stacked-list toggle is painted
/// once for the whole band unless `show_toggle` (volatile-only).
fn show_free_strip(
    ctx: &egui::Context,
    area: egui::Area,
    tokens: &DockTokens,
    th: &DockThemeTokens,
    pinned: bool,
    show_toggle: bool,
    open_anim: f32,
    add_body: impl FnOnce(&mut egui::Ui),
) -> PanelRender {
    let mut minimize = false;
    let mut toggle_layout = false;
    let size = flyout_icon_size(tokens);
    let mut content_h = 0.0;
    let response = area.show(ctx, |ui| {
        ui.set_opacity(open_anim);
        let body = ui.scope(add_body);
        let cluster = body.response.rect;
        content_h = cluster.height();
        let clicks = put_strip_controls(ui, cluster, size, th, pinned, show_toggle);
        minimize |= clicks.minimize;
        toggle_layout |= clicks.toggle_layout;
    });
    PanelRender {
        minimize,
        toggle_layout,
        rect: response.response.rect,
        content_h,
    }
}

fn strip_primary_center(cluster: Rect, icon_size: f32) -> f32 {
    cluster.bottom() - icon_size * 0.5
}

fn put_strip_controls(
    ui: &mut egui::Ui,
    cluster: Rect,
    icon_size: f32,
    th: &DockThemeTokens,
    pinned: bool,
    show_toggle: bool,
) -> CaptionClicks {
    let cy = strip_primary_center(cluster, icon_size);
    let mut x = cluster.right() + 12.0;
    let mut toggle_layout = false;
    if show_toggle {
        let toggle_rect = Rect::from_center_size(Pos2::new(x + 8.0, cy), Vec2::new(16.0, 14.0));
        toggle_layout = paint_list_toggle(ui, toggle_rect, th);
        x = toggle_rect.right() + 6.0;
    }
    let min_rect = Rect::from_center_size(Pos2::new(x + 7.0, cy), Vec2::splat(14.0));
    CaptionClicks {
        minimize: paint_strip_minimize(ui, min_rect, th, pinned),
        toggle_layout,
    }
}

fn paint_list_toggle(ui: &mut egui::Ui, rect: Rect, th: &DockThemeTokens) -> bool {
    let resp = ui
        .allocate_rect(rect, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Stacked list");
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, th.border_color().gamma_multiply(0.22));
    }
    paint_toggle_lines(
        ui.painter(),
        rect,
        th.muted_text_color().gamma_multiply(0.7),
    );
    resp.clicked()
}

fn paint_strip_minimize(ui: &mut egui::Ui, rect: Rect, th: &DockThemeTokens, pinned: bool) -> bool {
    let resp = ui
        .allocate_rect(rect, Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if pinned {
            "Unpin (return to icon)"
        } else {
            "Close"
        });
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, th.border_color().gamma_multiply(0.22));
    }
    let c = rect.center();
    ui.painter().line_segment(
        [Pos2::new(c.x - 4.0, c.y), Pos2::new(c.x + 4.0, c.y)],
        Stroke::new(1.15_f32, th.muted_text_color().gamma_multiply(0.85)),
    );
    resp.clicked()
}

fn paint_strip_dividers(
    painter: &egui::Painter,
    side: DockSide,
    rects: &[Rect],
    th: &DockThemeTokens,
) {
    if rects.len() < 2 {
        return;
    }
    let stroke = Stroke::new(1.0_f32, th.muted_text_color().gamma_multiply(0.28));
    for pair in rects.windows(2) {
        let a = pair[0];
        let b = pair[1];
        match side {
            DockSide::BottomCenter => {
                let x = (a.right() + b.left()) * 0.5;
                let y0 = a.top().min(b.top()) + 3.0;
                let y1 = a.bottom().max(b.bottom()) - 3.0;
                if y1 > y0 {
                    painter.line_segment([Pos2::new(x, y0), Pos2::new(x, y1)], stroke);
                }
            }
            DockSide::LeftCenter => {
                let y = (a.bottom() + b.top()) * 0.5;
                let x0 = a.left().min(b.left()) + 3.0;
                let x1 = a.right().max(b.right()) - 3.0;
                if x1 > x0 {
                    painter.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], stroke);
                }
            }
        }
    }
}

fn nearest_strip_at(
    pos: Pos2,
    strips: &[(&'static str, Rect, Rect)],
    pad: f32,
) -> Option<(&'static str, Rect, Rect)> {
    let mut best: Option<(&'static str, Rect, Rect, f32)> = None;
    for &(id, icon, panel) in strips {
        if !panel.expand(pad).contains(pos) {
            continue;
        }
        let d = panel.center().distance(pos);
        if best.is_none_or(|b| d < b.3) {
            best = Some((id, icon, panel, d));
        }
    }
    best.map(|(id, icon, panel, _)| (id, icon, panel))
}

/// Frame + caption + body, shared by hover-preview (volatile) and pinned
/// panels so both size and read identically.
///
/// Height policy: while `last_content_h` fits in `body_max_h`, the body is
/// laid out directly and the Area grows/shrinks with open fold sections. Only
/// after content has overflowed the budget do we wrap in a ScrollArea — an
/// always-on ScrollArea would lock the Area to its previous size forever.
#[allow(clippy::too_many_arguments)]
fn show_panel(
    ctx: &egui::Context,
    area: egui::Area,
    tokens: &DockTokens,
    th: &DockThemeTokens,
    label: &str,
    pinned: bool,
    show_layout_toggle: bool,
    layout: DockBodyLayout,
    width: f32,
    body_max_h: f32,
    last_content_h: f32,
    open_anim: f32,
    add_body: impl FnOnce(&mut egui::Ui),
) -> PanelRender {
    let mut minimize = false;
    let mut toggle_layout = false;
    let mut content_h = 0.0;
    let needs_scroll = last_content_h > body_max_h + 1.0;
    let response = area.show(ctx, |ui| {
        ui.set_opacity(open_anim);
        popover_frame(tokens, th).show(ui, |ui| {
            ui.set_width((width - tokens.popover_padding * 2.0).max(1.0));
            let cap = panel_caption(ui, label, pinned, show_layout_toggle, layout, th);
            minimize |= cap.minimize;
            toggle_layout |= cap.toggle_layout;
            ui.separator();
            if needs_scroll {
                // Previous frame already grew the Area past the budget, so
                // available height is large enough for a max-height scroll.
                content_h = compact_panel_scroll(ui, body_max_h, add_body);
            } else {
                content_h = panel_body_unsized(ui, add_body);
            }
        });
    });
    PanelRender {
        minimize,
        toggle_layout,
        rect: response.response.rect,
        content_h,
    }
}

struct CaptionClicks {
    minimize: bool,
    toggle_layout: bool,
}

/// Panel title row: label, optional "pinned", layout toggle, minimize (─).
fn panel_caption(
    ui: &mut egui::Ui,
    label: &str,
    pinned: bool,
    show_layout_toggle: bool,
    layout: DockBodyLayout,
    th: &DockThemeTokens,
) -> CaptionClicks {
    let mut minimize = false;
    let mut toggle_layout = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(RichText::new(label).small().strong().color(th.text_color()))
                .sense(Sense::hover()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let size = Vec2::splat(14.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
            let resp = resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(if pinned {
                    "Unpin (return to icon)"
                } else {
                    "Close"
                });
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, 2.0, th.border_color().gamma_multiply(0.35));
            }
            let c = rect.center();
            ui.painter().line_segment(
                [Pos2::new(c.x - 4.0, c.y), Pos2::new(c.x + 4.0, c.y)],
                Stroke::new(1.15_f32, th.text_color()),
            );
            if resp.clicked() {
                minimize = true;
            }
            if show_layout_toggle {
                toggle_layout = layout_toggle_button(ui, layout, th);
            }
            if pinned {
                ui.add(
                    egui::Label::new(RichText::new("pinned").small().color(th.muted_text_color()))
                        .sense(Sense::hover()),
                );
            }
        });
    });
    CaptionClicks {
        minimize,
        toggle_layout,
    }
}

/// List mode shows the destination (three squircles). Icon-strip chrome
/// uses a muted stacked-list glyph via [`put_strip_controls`] instead.
fn layout_toggle_button(ui: &mut egui::Ui, layout: DockBodyLayout, th: &DockThemeTokens) -> bool {
    let size = Vec2::new(16.0, 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let (tip, paint_icons) = match layout {
        DockBodyLayout::List => ("Icon strip", true),
        DockBodyLayout::Icons => ("Stacked list", false),
    };
    let resp = resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tip);
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, th.border_color().gamma_multiply(0.35));
    }
    if paint_icons {
        paint_toggle_squircles(ui.painter(), rect, th.text_color());
    } else {
        paint_toggle_lines(ui.painter(), rect, th.text_color());
    }
    resp.clicked()
}

fn paint_toggle_lines(painter: &egui::Painter, rect: Rect, color: Color32) {
    let s = Stroke::new(1.15_f32, color);
    for y in [0.28, 0.50, 0.72] {
        painter.line_segment(
            [
                Pos2::new(
                    rect.left() + rect.width() * 0.18,
                    rect.top() + rect.height() * y,
                ),
                Pos2::new(
                    rect.right() - rect.width() * 0.18,
                    rect.top() + rect.height() * y,
                ),
            ],
            s,
        );
    }
}

fn paint_toggle_squircles(painter: &egui::Painter, rect: Rect, color: Color32) {
    let d = rect.height() * 0.38;
    let gap = (rect.width() - d * 3.0).max(1.0) / 4.0;
    let y = rect.center().y;
    for i in 0..3 {
        let x = rect.left() + gap + d * 0.5 + i as f32 * (d + gap);
        paint_squircle(
            painter,
            Rect::from_center_size(Pos2::new(x, y), Vec2::splat(d)),
            Color32::TRANSPARENT,
            Stroke::new(1.05_f32, color),
            4.0,
        );
    }
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

    #[test]
    fn body_height_budget_spans_the_canvas() {
        let mut tokens = DockTokens::default();
        tokens.normalize();
        let canvas = Rect::from_min_size(Pos2::new(0.0, 40.0), Vec2::new(1600.0, 900.0));
        let budget = panel_body_max_height(canvas, &tokens);
        // Nearly the whole canvas, minus margins and caption chrome — the old
        // behavior capped bodies at the much smaller popover_max_height.
        assert!(
            budget > canvas.height() - 100.0 && budget < canvas.height(),
            "budget {budget} should track the canvas height"
        );
    }

    #[test]
    fn panel_widens_while_content_overflows_then_stops_at_max() {
        let (min_w, max_w) = (260.0, 400.0);
        let mut w = min_w;
        for _ in 0..20 {
            // Content always overflows: width must climb and then hold.
            w = adapt_panel_width(w, 2000.0, 600.0, min_w, max_w);
        }
        assert_eq!(w, max_w);
    }

    #[test]
    fn panel_narrows_again_once_content_fits_easily() {
        let (min_w, max_w) = (260.0, 500.0);
        let mut w = 452.0;
        for _ in 0..20 {
            w = adapt_panel_width(w, 100.0, 600.0, min_w, max_w);
        }
        assert_eq!(w, min_w);
    }

    #[test]
    fn width_holds_inside_the_dead_zone() {
        let w = 340.0;
        // Content fills most, but not all, of the budget: no oscillation.
        assert_eq!(adapt_panel_width(w, 500.0, 600.0, 260.0, 500.0), w);
    }

    #[test]
    fn width_never_exceeds_a_narrow_canvas() {
        let mut tokens = DockTokens::default();
        tokens.normalize();
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(500.0, 600.0));
        let mut state = DockState::default();
        state.panel_widths.insert("filters", 9000.0);
        let w = panel_width(&state, "filters", &tokens, canvas);
        assert!(
            w >= tokens.popover_width && w <= panel_max_width(canvas, &tokens),
            "width {w} escaped the canvas clamp"
        );
    }

    #[test]
    fn title_chip_only_while_the_pointer_is_on_the_icon() {
        assert!(chip_from_icon_hover(true, false));
        assert!(
            !chip_from_icon_hover(true, true),
            "an open flyout owns hover over the strip behind/beneath it"
        );
        assert!(!chip_from_icon_hover(false, false));
        assert!(!chip_from_icon_hover(false, true));
    }

    #[test]
    fn flyout_squircles_are_sixty_five_percent_of_the_dock() {
        let mut tokens = DockTokens::default();
        tokens.icon_size = 34.0;
        tokens.flyout_icon_scale = 0.65;
        tokens.normalize();
        assert!((flyout_icon_size(&tokens) - 22.1).abs() < 0.01);
    }

    #[test]
    fn hex_pack_fills_the_bottom_row_first() {
        // size 20, gap 4, pitch 24. Budget 100 → 4 on the primary row.
        let (offs, size) = hex_pack_offsets(5, 20.0, 4.0, 100.0, DockSide::BottomCenter);
        assert_eq!(offs.len(), 5);
        let bottom_y = offs.iter().map(|o| o.y).fold(f32::NEG_INFINITY, f32::max);
        let bottom = offs
            .iter()
            .filter(|o| (o.y - bottom_y).abs() < 0.05)
            .count();
        assert_eq!(
            bottom, 4,
            "overflow should sit on a new row, not replace the first"
        );
        let top = offs.iter().find(|o| (o.y - bottom_y).abs() > 0.05).unwrap();
        assert!(
            (top.x - 12.0).abs() < 0.05,
            "odd row should stagger by half a pitch, got x={}",
            top.x
        );
        assert!(size.y > 20.0, "two rows must grow the cluster height");
    }

    #[test]
    fn hex_pack_left_dock_fills_the_near_column_first() {
        let (offs, _) = hex_pack_offsets(5, 20.0, 4.0, 100.0, DockSide::LeftCenter);
        let left_x = offs.iter().map(|o| o.x).fold(f32::INFINITY, f32::min);
        let primary = offs.iter().filter(|o| (o.x - left_x).abs() < 0.05).count();
        assert_eq!(primary, 4);
        let overflow = offs.iter().find(|o| (o.x - left_x).abs() > 0.05).unwrap();
        assert!(overflow.x > left_x, "extra column goes to the right");
    }

    #[test]
    fn dashboards_can_use_the_icon_strip() {
        let mut state = DockState::default();
        state.icon_strip = true;
        assert_eq!(
            body_layout_for(&state, "document.settings", DockItemKind::Dashboard),
            DockBodyLayout::Icons
        );
        assert_eq!(
            body_layout_for(&state, "tool.shapes", DockItemKind::Tool),
            DockBodyLayout::Icons
        );
        state.icon_strip = false;
        assert_eq!(
            body_layout_for(&state, "tool.shapes", DockItemKind::Tool),
            DockBodyLayout::List
        );
    }

    #[test]
    fn layout_toggle_is_dock_wide() {
        let mut state = DockState::default();
        apply_layout_toggle(&mut state, "tool.shapes", true);
        assert!(state.icon_strip);
        assert_eq!(
            body_layout_for(&state, "tool.frame", DockItemKind::Tool),
            DockBodyLayout::Icons
        );
        apply_layout_toggle(&mut state, "document.settings", true);
        assert!(!state.icon_strip);
    }

    #[test]
    fn pinned_strips_keep_primary_icon_order() {
        let mut tokens = DockTokens::default();
        tokens.normalize();
        let canvas = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 900.0));
        // A wide later palette would sort *left* of an earlier one if we
        // packed by preferred left (icon.center - size/2). Icon order wins.
        let frame_icon = Rect::from_center_size(Pos2::new(200.0, 860.0), Vec2::splat(34.0));
        let shapes_icon = Rect::from_center_size(Pos2::new(400.0, 860.0), Vec2::splat(34.0));
        let open = [
            ("tool.frame", frame_icon, Vec2::new(80.0, 30.0)),
            ("tool.shapes", shapes_icon, Vec2::new(500.0, 30.0)),
        ];
        let origins = layout_panel_origins(DockSide::BottomCenter, &open, &tokens, canvas);
        let frame_x = origins.get("tool.frame").expect("frame").x;
        let shapes_x = origins.get("tool.shapes").expect("shapes").x;
        assert!(
            frame_x < shapes_x,
            "frame ({frame_x}) should stay left of shapes ({shapes_x})"
        );
    }

    #[test]
    fn tall_panel_is_anchored_inside_the_canvas() {
        let mut tokens = DockTokens::default();
        tokens.normalize();
        let canvas = Rect::from_min_size(Pos2::new(0.0, 40.0), Vec2::new(1600.0, 900.0));
        let icon = Rect::from_min_size(Pos2::new(10.0, 460.0), Vec2::splat(34.0));
        // A panel taller than the canvas would otherwise be centered off-screen.
        let open = [("filters", icon, Vec2::new(260.0, 1200.0))];
        let origins = layout_panel_origins(DockSide::LeftCenter, &open, &tokens, canvas);
        let y = origins.get("filters").expect("origin").y;
        assert!(
            y >= canvas.top(),
            "panel top {y} floated above canvas top {}",
            canvas.top()
        );
    }
}
