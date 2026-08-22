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

use crate::sidebar::{
    paint_sidebar_icon_pill, sidebar_icon_row, sidebar_tool_row, SidebarTheme, TOGGLE_SLIDE_SECS,
};
use crate::theme::Palette;
use crate::tokens::{DockPaletteTokens, DockThemeTokens, DockTokens};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, ScrollArea, Sense, Shadow,
    Shape, Stroke, Vec2,
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
/// free-space icon strip. Advanced is a framed overlay on top of a strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DockBodyLayout {
    #[default]
    List,
    Icons,
    /// Tinted catalog canvas of every tool in the palette. Never the
    /// dock-wide default — only while a strip's Advanced overlay is open.
    Advanced,
}

/// Secondary circular icon vs tertiary sliding toggle in the icon strip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlyoutRole {
    #[default]
    Icon,
    Toggle,
}

/// One row / icon / toggle in a tool flyout ([`flyout_items`]).
pub struct FlyoutItem<'a> {
    pub id: &'static str,
    pub label: &'a str,
    pub description: &'a str,
    pub hotkey: Option<&'a str>,
    pub icon: DockIcon,
    pub active: bool,
    /// Fieldset label on the icon strip. Consecutive items with the same
    /// group share one bordered frame. `None` is an unlabeled frame.
    pub group: Option<&'a str>,
    pub role: FlyoutRole,
}

impl<'a> FlyoutItem<'a> {
    pub fn new(
        id: &'static str,
        label: &'a str,
        description: &'a str,
        hotkey: Option<&'a str>,
        icon: DockIcon,
        active: bool,
    ) -> Self {
        Self {
            id,
            label,
            description,
            hotkey,
            icon,
            active,
            group: None,
            role: FlyoutRole::Icon,
        }
    }

    pub fn grouped(mut self, group: &'a str) -> Self {
        self.group = Some(group);
        self
    }

    pub fn toggle(mut self) -> Self {
        self.role = FlyoutRole::Toggle;
        self
    }
}

const BODY_LAYOUT_ID: &str = "atlas_dock_body_layout";
const DOCK_SIDE_ID: &str = "atlas_dock_side";
const STRIP_BUDGET_ID: &str = "atlas_dock_strip_budget";
const STRIP_PALETTE_ID: &str = "atlas_dock_strip_palette";
const STRIP_PALETTE_LABEL_ID: &str = "atlas_dock_strip_palette_label";
const STRIP_HIDDEN_ID: &str = "atlas_dock_strip_hidden";
const STRIP_VISIBLE_ID: &str = "atlas_dock_strip_visible";
const STRIP_NATURAL_ID: &str = "atlas_dock_strip_natural";
const STRIP_ASSOCIATE_ID: &str = "atlas_dock_strip_associate";

/// Click / drop outcome from one [`floating_dock`] frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct DockOutcome {
    pub clicked: Option<&'static str>,
    /// Palette id whose Drop dot asked to embed a copy on the canvas.
    pub drop_to_canvas: Option<&'static str>,
}

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
    /// free-space icon strip when true. Toggling any list control, or a
    /// strip's stacked-view dot, flips this for the whole dock.
    icon_strip: bool,
    /// Palette whose Advanced catalog canvas is open.
    advanced: Option<&'static str>,
    /// Tools hidden from each palette's icon strip. Empty vec = show all.
    hidden: HashMap<String, Vec<String>>,
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
    /// Primary icon bar tucked into a readout blister. Pinned palettes stay.
    bar_collapsed: bool,
    last_icon_rects: HashMap<&'static str, Rect>,
    last_panel_rects: HashMap<&'static str, Rect>,
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

/// Dark mode lightens gray; light mode darkens it.
fn associate_shade(color: Color32, dark: bool, tint: f32) -> Color32 {
    let toward = if dark {
        Color32::from_rgba_unmultiplied(255, 255, 255, color.a())
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, color.a())
    };
    mix_icon_fill(color, toward, tint)
}

/// Primary-icon outline: pinned is always denser than idle; hover can go further.
fn icon_outline(associated: bool, pinned: bool, p: &DockPaletteTokens) -> (f32, f32) {
    let (width, tint) = if pinned {
        (p.pinned_stroke.max(1.0), p.pinned_tint)
    } else {
        (1.0, 0.0)
    };
    if associated {
        (width.max(p.associate_stroke), tint.max(p.associate_tint))
    } else {
        (width, tint)
    }
}

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

pub fn paint_squircle(painter: &egui::Painter, rect: Rect, fill: Color32, stroke: Stroke, n: f32) {
    painter.add(Shape::convex_polygon(
        squircle_points(rect, n, 32),
        fill,
        stroke,
    ));
}

fn pt(r: Rect, x: f32, y: f32) -> Pos2 {
    Pos2::new(r.min.x + r.width() * x, r.min.y + r.height() * y)
}

/// Paint a dock icon into `rect`. Canvas embeds reuse this so a dropped
/// toolbar matches the baseline strip.
pub fn paint_dock_icon(painter: &egui::Painter, rect: Rect, icon: DockIcon, color: Color32) {
    paint_builtin_icon(painter, rect, icon, color);
}

fn paint_builtin_icon(painter: &egui::Painter, rect: Rect, icon: DockIcon, color: Color32) {
    let s = Stroke::new((rect.width() * 0.075).max(0.4), color);
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

/// Whether the primary icon bar is collapsed into the readout blister.
/// `None` until the dock has rendered at least once this session.
pub fn bar_collapsed(ctx: &egui::Context, id: impl std::hash::Hash) -> Option<bool> {
    let state_id = egui::Id::new(("floating_dock", &id));
    ctx.data_mut(|d| d.get_temp::<DockState>(state_id))
        .filter(|s| s.seeded)
        .map(|s| s.bar_collapsed)
}

pub fn set_bar_collapsed(ctx: &egui::Context, id: impl std::hash::Hash, collapsed: bool) {
    let state_id = egui::Id::new(("floating_dock", &id));
    ctx.data_mut(|d| {
        if let Some(mut s) = d.get_temp::<DockState>(state_id) {
            s.bar_collapsed = collapsed;
            d.insert_temp(state_id, s);
        }
    });
}

/// Tools hidden from each palette's icon strip — for persisting to prefs.
/// `None` until the dock has rendered at least once this session.
pub fn strip_hidden(
    ctx: &egui::Context,
    id: impl std::hash::Hash,
) -> Option<Vec<(String, Vec<String>)>> {
    let state_id = egui::Id::new(("floating_dock", &id));
    ctx.data_mut(|d| d.get_temp::<DockState>(state_id))
        .filter(|s| s.seeded)
        .map(|s| {
            let mut rows: Vec<(String, Vec<String>)> = s
                .hidden
                .iter()
                .filter(|(_, tools)| !tools.is_empty())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            rows
        })
}

/// Flyout ids the Advanced catalog asked to duplicate as new tool types.
pub fn take_catalog_duplicate(ctx: &egui::Context) -> Option<Vec<String>> {
    crate::dock_advanced::take_duplicate(ctx)
}

/// Tool ids last shown on a palette's icon strip (after on-strip tags).
/// Used when dropping a copy onto the canvas.
pub fn last_strip_tools(ctx: &egui::Context, palette: &str) -> Vec<String> {
    ctx.data(|d| {
        d.get_temp::<HashMap<String, Vec<String>>>(egui::Id::new(STRIP_VISIBLE_ID))
            .and_then(|m| m.get(palette).cloned())
            .unwrap_or_default()
    })
}

fn set_current_palette(ctx: &egui::Context, id: &'static str, label: &str) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::new(STRIP_PALETTE_ID), id);
        d.insert_temp(egui::Id::new(STRIP_PALETTE_LABEL_ID), label.to_owned());
    });
}

fn current_palette_label(ctx: &egui::Context) -> Option<String> {
    ctx.data(|d| d.get_temp(egui::Id::new(STRIP_PALETTE_LABEL_ID)))
}

pub(crate) fn current_palette(ctx: &egui::Context) -> Option<&'static str> {
    ctx.data(|d| d.get_temp(egui::Id::new(STRIP_PALETTE_ID)))
}

fn sync_hidden_to_ctx(ctx: &egui::Context, hidden: &HashMap<String, Vec<String>>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STRIP_HIDDEN_ID), hidden.clone()));
}

pub(crate) fn hidden_from_ctx(ctx: &egui::Context) -> HashMap<String, Vec<String>> {
    ctx.data(|d| {
        d.get_temp::<HashMap<String, Vec<String>>>(egui::Id::new(STRIP_HIDDEN_ID))
            .unwrap_or_default()
    })
}

pub(crate) fn tool_hidden_in(
    hidden: &HashMap<String, Vec<String>>,
    palette: &str,
    tool: &str,
) -> bool {
    hidden
        .get(palette)
        .is_some_and(|tools| tools.iter().any(|t| t == tool))
}

pub(crate) fn set_tool_on_strip(ctx: &egui::Context, palette: &str, tool: &str, on: bool) {
    ctx.data_mut(|d| {
        let mut hidden: HashMap<String, Vec<String>> = d
            .get_temp(egui::Id::new(STRIP_HIDDEN_ID))
            .unwrap_or_default();
        let entry = hidden.entry(palette.to_owned()).or_default();
        if on {
            entry.retain(|t| t != tool);
        } else if !entry.iter().any(|t| t == tool) {
            entry.push(tool.to_owned());
        }
        if entry.is_empty() {
            hidden.remove(palette);
        }
        d.insert_temp(egui::Id::new(STRIP_HIDDEN_ID), hidden);
    });
}

fn remember_strip_tools(ctx: &egui::Context, palette: &str, ids: Vec<String>) {
    ctx.data_mut(|d| {
        let mut map: HashMap<String, Vec<String>> = d
            .get_temp(egui::Id::new(STRIP_VISIBLE_ID))
            .unwrap_or_default();
        map.insert(palette.to_owned(), ids);
        d.insert_temp(egui::Id::new(STRIP_VISIBLE_ID), map);
    });
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
    let chrome = tokens.popover_padding * 2.0 + tokens.palette.caption_height + PANEL_SEPARATOR_H;
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
    bar_collapsed: bool,
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
            let x = if bar_collapsed {
                canvas.left() + tokens.left_margin
            } else {
                open.iter()
                    .map(|(_, icon, _)| icon.right())
                    .fold(f32::NEG_INFINITY, f32::max)
                    + tokens.popover_gap
            };

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
            let y = if bar_collapsed {
                canvas.bottom() - tokens.bottom_margin
            } else {
                open.iter()
                    .map(|(_, icon, _)| icon.top())
                    .fold(f32::INFINITY, f32::min)
                    - tokens.popover_gap
            };

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

/// Design-pixel layout of a fieldset icon strip. The docked flyout and a
/// canvas `DockStrip` both consume this so they cannot diverge.
#[derive(Clone, Debug)]
pub struct IconStripLayout {
    pub size: Vec2,
    groups: Vec<IconStripGroup>,
}

#[derive(Clone, Debug)]
struct IconStripGroup {
    label: Option<String>,
    origin: Vec2,
    frame: Vec2,
    slots: Vec<IconStripSlot>,
}

#[derive(Clone, Debug)]
struct IconStripSlot {
    id: &'static str,
    off: Vec2,
    size: Vec2,
}

/// Uniform scale that fits `src` inside `dest` (contain, never stretch).
pub fn contain_scale(dest: Vec2, src: Vec2) -> f32 {
    if src.x < 1.0 || src.y < 1.0 {
        return 1.0;
    }
    (dest.x / src.x).min(dest.y / src.y).max(0.0)
}

/// Measure the strip at docked-flyout design size. `budget` wraps groups
/// the same way the window dock does; pass a large value for a one-row
/// poster (canvas embeds).
pub fn measure_icon_strip(
    ctx: &egui::Context,
    items: &[FlyoutItem<'_>],
    tokens: &DockTokens,
    side: DockSide,
    budget: f32,
) -> IconStripLayout {
    let shown: Vec<&FlyoutItem<'_>> = items.iter().collect();
    measure_icon_strip_refs(ctx, &shown, tokens, side, budget)
}

fn measure_icon_strip_refs(
    ctx: &egui::Context,
    shown: &[&FlyoutItem<'_>],
    tokens: &DockTokens,
    side: DockSide,
    budget: f32,
) -> IconStripLayout {
    let groups = group_flyout_runs(shown);
    let size = flyout_icon_size(tokens);
    let gap = (tokens.icon_gap * tokens.flyout_icon_scale).max(4.0);
    let laid = layout_fieldset_strip(
        ctx,
        &groups,
        size,
        gap,
        budget.max(size),
        side,
        &tokens.palette,
    );
    IconStripLayout {
        size: laid.cluster,
        groups: laid
            .groups
            .into_iter()
            .map(|g| IconStripGroup {
                label: g.label.map(str::to_owned),
                origin: g.origin,
                frame: g.frame,
                slots: g
                    .slots
                    .into_iter()
                    .map(|s| IconStripSlot {
                        id: s.item.id,
                        off: s.off,
                        size: s.size,
                    })
                    .collect(),
            })
            .collect(),
    }
}

impl IconStripLayout {
    pub fn first_slot_id(&self) -> Option<&'static str> {
        self.groups
            .iter()
            .find_map(|g| g.slots.first().map(|s| s.id))
    }
}

/// Painted bounds of a canvas copy — the strip itself, not a second card.
pub fn icon_strip_card_size(strip: &IconStripLayout, _tokens: &DockTokens) -> Vec2 {
    strip.size
}

/// Painted card inside `dest` after contain-scale (centered, never stretched).
pub fn icon_strip_card_rect(dest: Rect, layout: &IconStripLayout, tokens: &DockTokens) -> Rect {
    let card = icon_strip_card_size(layout, tokens);
    let scale = contain_scale(dest.size(), card);
    let used = Vec2::new(card.x * scale, card.y * scale);
    Rect::from_center_size(dest.center(), used)
}

/// Fillet of a contained strip — selection chrome must match the fieldset.
pub fn icon_strip_card_radius(dest: Rect, layout: &IconStripLayout, tokens: &DockTokens) -> f32 {
    tokens.palette.group_radius * contain_scale(dest.size(), icon_strip_card_size(layout, tokens))
}

/// Screen rect of one tool slot after the same contain transform paint uses.
pub fn icon_strip_slot_rect(
    dest: Rect,
    layout: &IconStripLayout,
    tokens: &DockTokens,
    id: &str,
) -> Option<Rect> {
    let (strip_origin, scale) = icon_strip_placed(dest, layout, tokens);
    if scale <= 0.0 {
        return None;
    }
    for g in &layout.groups {
        let frame = Rect::from_min_size(strip_origin + g.origin * scale, g.frame * scale);
        for s in &g.slots {
            if s.id == id {
                return Some(Rect::from_min_size(frame.min + s.off * scale, s.size * scale));
            }
        }
    }
    None
}

/// Which tool sits under `screen`, using the same contain transform paint uses.
pub fn icon_strip_hit(
    dest: Rect,
    layout: &IconStripLayout,
    tokens: &DockTokens,
    screen: Pos2,
) -> Option<&'static str> {
    let (strip_origin, scale) = icon_strip_placed(dest, layout, tokens);
    if scale <= 0.0 {
        return None;
    }
    for g in &layout.groups {
        let frame = Rect::from_min_size(strip_origin + g.origin * scale, g.frame * scale);
        for s in &g.slots {
            let r = Rect::from_min_size(frame.min + s.off * scale, s.size * scale);
            if r.contains(screen) {
                return Some(s.id);
            }
        }
    }
    None
}

fn icon_strip_placed(
    dest: Rect,
    layout: &IconStripLayout,
    tokens: &DockTokens,
) -> (Pos2, f32) {
    let card = icon_strip_card_size(layout, tokens);
    let scale = contain_scale(dest.size(), card);
    let used = Vec2::new(card.x * scale, card.y * scale);
    let origin = dest.center() - used * 0.5;
    (origin, scale)
}

/// Paint a canvas copy of a docked icon-strip palette — the same fieldset
/// strip as the flyout, no second card around it. Returns a hovered tool
/// id. Clicks are the caller's (board vs dock).
pub fn paint_icon_strip_card(
    ui: &egui::Ui,
    dest: Rect,
    title: &str,
    items: &[FlyoutItem<'_>],
    layout: &IconStripLayout,
    tokens: &DockTokens,
    hovered_id: Option<&str>,
) -> f32 {
    let th = if ui.visuals().dark_mode {
        &tokens.dark
    } else {
        &tokens.light
    };
    let card = icon_strip_card_size(layout, tokens);
    let scale = contain_scale(dest.size(), card);
    if scale <= 0.0 {
        return 0.0;
    }
    let (strip_origin, _) = icon_strip_placed(dest, layout, tokens);
    paint_icon_strip_visuals(
        ui,
        strip_origin,
        scale,
        layout,
        items,
        tokens,
        th,
        hovered_id,
        Some(title),
        false,
    );
    if let Some(hid) = hovered_id {
        if let Some(item) = items.iter().find(|it| it.id == hid) {
            if let Some(rect) = icon_strip_slot_rect(dest, layout, tokens, hid) {
                show_hover_chip(
                    ui.ctx(),
                    ui.id().with(("canvas_strip", hid)),
                    Pos2::new(rect.center().x, rect.top() - 6.0 * scale.max(0.5)),
                    Align2::CENTER_BOTTOM,
                    item.label,
                    item.hotkey,
                    None,
                    th,
                );
            }
        }
    }
    scale
}

fn set_strip_budget(ctx: &egui::Context, budget: f32) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STRIP_BUDGET_ID), budget));
}

fn set_strip_associate(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(STRIP_ASSOCIATE_ID), on));
}

fn strip_associate(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp(egui::Id::new(STRIP_ASSOCIATE_ID)))
        .unwrap_or(false)
}

fn body_rect_at(origin: Pos2, pivot: Align2, size: Vec2) -> Rect {
    let min = if pivot == Align2::CENTER_BOTTOM {
        Pos2::new(origin.x - size.x * 0.5, origin.y - size.y)
    } else {
        origin
    };
    Rect::from_min_size(min, size)
}

fn collapse_zone_rects(side: DockSide, bar: Rect, canvas: Rect, zone: f32) -> [Rect; 3] {
    match side {
        DockSide::BottomCenter => [
            Rect::from_min_max(
                Pos2::new(bar.left() - zone, bar.bottom()),
                Pos2::new(bar.right() + zone, canvas.bottom()),
            ),
            Rect::from_min_max(
                Pos2::new((bar.left() - zone).max(canvas.left()), bar.top()),
                Pos2::new(bar.left(), bar.bottom()),
            ),
            Rect::from_min_max(
                Pos2::new(bar.right(), bar.top()),
                Pos2::new((bar.right() + zone).min(canvas.right()), bar.bottom()),
            ),
        ],
        DockSide::LeftCenter => [
            Rect::from_min_max(
                Pos2::new(canvas.left(), bar.top() - zone),
                Pos2::new(bar.left(), bar.bottom() + zone),
            ),
            Rect::from_min_max(
                Pos2::new(bar.left(), (bar.top() - zone).max(canvas.top())),
                Pos2::new(bar.right(), bar.top()),
            ),
            Rect::from_min_max(
                Pos2::new(bar.left(), bar.bottom()),
                Pos2::new(bar.right(), (bar.bottom() + zone).min(canvas.bottom())),
            ),
        ],
    }
}

fn paint_tiny_chevron(painter: &egui::Painter, center: Pos2, size: f32, color: Color32, down: bool) {
    let h = size * 0.45;
    let w = size * 0.55;
    let (a, b, c) = if down {
        (
            Pos2::new(center.x - w, center.y - h * 0.4),
            Pos2::new(center.x, center.y + h * 0.6),
            Pos2::new(center.x + w, center.y - h * 0.4),
        )
    } else {
        (
            Pos2::new(center.x - w, center.y + h * 0.4),
            Pos2::new(center.x, center.y - h * 0.6),
            Pos2::new(center.x + w, center.y + h * 0.4),
        )
    };
    painter.line_segment([a, b], Stroke::new(1.2_f32, color));
    painter.line_segment([b, c], Stroke::new(1.2_f32, color));
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
    (span - PANEL_EDGE_MARGIN * 2.0 - tokens.palette.controls_width).max(min)
}

/// Icons that fit on one primary line inside `budget`.
fn icon_line_cap(size: f32, gap: f32, budget: f32) -> usize {
    if budget + 0.5 < size {
        return 1;
    }
    let pitch = size + gap;
    (((budget - size) / pitch).floor() as i32 + 1).max(1) as usize
}

fn icon_line_along(cols: usize, size: f32, gap: f32) -> f32 {
    let c = cols.max(1);
    c as f32 * size + c.saturating_sub(1) as f32 * gap
}

/// Peel one column at a time from the widest group so icons stay in a
/// sideways row and only the overflow steps onto the next line.
fn accordion_peel_inners(
    icon_counts: &[usize],
    start_inners: &[f32],
    icon: f32,
    gap: f32,
    group_pad: f32,
    group_gap: f32,
    budget: f32,
) -> Vec<f32> {
    let n = start_inners.len();
    let mut inners = start_inners.to_vec();
    if n == 0 {
        return inners;
    }
    let frame_along = |inners: &[f32]| -> f32 {
        inners.iter().map(|w| w + group_pad * 2.0).sum::<f32>()
            + group_gap * n.saturating_sub(1) as f32
    };
    let cols_of = |inner: f32, count: usize| -> usize {
        let cap = icon_line_cap(icon, gap, inner);
        if count > 0 {
            cap.min(count)
        } else {
            cap
        }
    };
    let mut guard = 0;
    while frame_along(&inners) > budget + 0.5 && guard < 256 {
        guard += 1;
        let mut best: Option<usize> = None;
        let mut best_cols = 1usize;
        for i in 0..n {
            let cols = cols_of(inners[i], icon_counts[i]);
            if cols > 1 && cols >= best_cols {
                best_cols = cols;
                best = Some(i);
            }
        }
        let Some(i) = best else {
            break;
        };
        inners[i] = icon_line_along(best_cols - 1, icon, gap);
    }
    inners
}

/// When the pinned band overflows, every category peels one column in the
/// same round — no strip stays a single row while its neighbor is already
/// stacked and overlapping it.
fn accordion_band_budgets(
    ids: &[&str],
    naturals: &HashMap<String, f32>,
    usable: f32,
    stack_gap: f32,
    min_along: f32,
    pitch: f32,
) -> Option<HashMap<String, f32>> {
    if ids.is_empty() {
        return None;
    }
    let mut widths = Vec::with_capacity(ids.len());
    for id in ids {
        let w = naturals.get(*id).copied()?;
        if w < 1.0 {
            return None;
        }
        widths.push(w);
    }
    let gaps = stack_gap * ids.len().saturating_sub(1) as f32;
    let total = |w: &[f32]| w.iter().sum::<f32>() + gaps;
    if total(&widths) <= usable + 1.0 {
        return None;
    }
    let mut guard = 0;
    while total(&widths) > usable + 1.0 && guard < 256 {
        guard += 1;
        let mut peeled = false;
        for w in &mut widths {
            if *w > min_along + 0.5 {
                *w = (*w - pitch).max(min_along);
                peeled = true;
            }
        }
        if !peeled {
            break;
        }
    }
    Some(
        ids.iter()
            .zip(widths)
            .map(|(id, w)| ((*id).to_owned(), w))
            .collect(),
    )
}

/// Wrap `count` icons into left-aligned rows. The primary line (bottom row /
/// left column) fills first; overflow stacks the next line above (or to the
/// right on a left dock) — an accordion, not a hex stagger.
fn row_pack_offsets(
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
    let cap = icon_line_cap(size, gap, budget);

    let mut line_of = Vec::with_capacity(count);
    let mut slot_of = Vec::with_capacity(count);
    let mut i = 0;
    let mut line = 0usize;
    while i < count {
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
        let k = slot_of[idx];
        let along = k as f32 * pitch;
        max_along = max_along.max(along + size);
        raw.push(Vec2::new(along, line_of[idx] as f32 * pitch));
    }

    match side {
        DockSide::BottomCenter => {
            let width = max_along;
            let height = size + (n_lines.saturating_sub(1) as f32) * pitch;
            let offsets = raw
                .into_iter()
                .map(|p| Vec2::new(p.x, height - size - p.y))
                .collect();
            (offsets, Vec2::new(width, height))
        }
        DockSide::LeftCenter => {
            let width = size + (n_lines.saturating_sub(1) as f32) * pitch;
            let height = max_along;
            let offsets = raw
                .into_iter()
                .map(|p| Vec2::new(p.y, height - size - p.x))
                .collect();
            (offsets, Vec2::new(width, height))
        }
    }
}

/// Paint a flyout as a stacked list, a free-space icon strip, or the
/// Advanced tag list, following [`current_body_layout`]. Hovering a strip
/// icon shows the same name + linger-description chip as the primary dock.
pub fn flyout_items(ui: &mut egui::Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    match current_body_layout(ui.ctx()) {
        DockBodyLayout::List => flyout_list(ui, items),
        DockBodyLayout::Icons => flyout_icon_strip(ui, items),
        DockBodyLayout::Advanced => flyout_advanced(ui, items),
    }
}

fn flyout_list_theme(ui: &egui::Ui) -> SidebarTheme {
    SidebarTheme {
        card: ui.visuals().panel_fill,
        border: ui.visuals().widgets.noninteractive.bg_stroke.color,
        ink: ui.visuals().text_color(),
        sub: ui.visuals().weak_text_color(),
    }
}

fn flyout_list(ui: &mut egui::Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    // A drop to canvas is always an icon strip, whichever presentation the
    // palette is wearing — so record the strip's contents here too, or Drop
    // from stacked view would place an empty node.
    if let Some(palette) = current_palette(ui.ctx()) {
        let hidden = hidden_from_ctx(ui.ctx());
        remember_strip_tools(
            ui.ctx(),
            palette,
            items
                .iter()
                .filter(|item| !tool_hidden_in(&hidden, palette, item.id))
                .map(|item| item.id.to_owned())
                .collect(),
        );
    }
    let theme = flyout_list_theme(ui);
    let mut clicked = None;
    for item in items {
        let paint = |p: &egui::Painter, r: Rect, c: Color32| {
            paint_builtin_icon(p, r, item.icon, c);
        };
        let mut resp = if item.role == FlyoutRole::Toggle {
            sidebar_icon_row(ui, item.label, item.hotkey, item.active, theme, paint)
        } else {
            sidebar_tool_row(ui, item.label, item.hotkey, item.active, theme, paint)
        };
        if !item.description.is_empty() {
            resp = resp.on_hover_text(item.description);
        }
        if resp.clicked() {
            clicked = Some(item.id);
        }
    }
    clicked
}

fn flyout_advanced(ui: &mut egui::Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    crate::dock_advanced::show(ui, items).or_else(|| crate::dock_advanced::take_use(ui.ctx()))
}

fn tertiary_slot_h(icon: f32, p: &DockPaletteTokens) -> f32 {
    ((icon - p.tertiary_stack_gap) * 0.5).max(8.0)
}

fn flyout_icon_strip(ui: &mut egui::Ui, items: &[FlyoutItem<'_>]) -> Option<&'static str> {
    let palette = current_palette(ui.ctx());
    let hidden = hidden_from_ctx(ui.ctx());
    let shown: Vec<&FlyoutItem<'_>> = items
        .iter()
        .filter(|item| !palette.is_some_and(|p| tool_hidden_in(&hidden, p, item.id)))
        .collect();
    if let Some(p) = palette {
        remember_strip_tools(
            ui.ctx(),
            p,
            shown.iter().map(|item| item.id.to_owned()).collect(),
        );
    }
    let mut tokens = crate::tokens::current().dock;
    tokens.normalize();
    let th = if ui.visuals().dark_mode {
        &tokens.dark
    } else {
        &tokens.light
    };
    let size = flyout_icon_size(&tokens);
    let side = current_dock_side(ui.ctx());
    let budget = strip_budget(ui.ctx()).unwrap_or_else(|| ui.available_width().max(size));
    let layout = measure_icon_strip_refs(ui.ctx(), &shown, &tokens, side, budget);
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

    let (origin, _) = ui.allocate_exact_size(layout.size, Sense::hover());
    for group in &layout.groups {
        let frame = Rect::from_min_size(origin.min + group.origin, group.frame);
        for slot in &group.slots {
            let rect = Rect::from_min_size(frame.min + slot.off, slot.size);
            let resp = ui
                .interact(rect, ui.id().with(slot.id), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.hovered() {
                hovered_id = Some(slot.id);
                hovered_rect = rect;
            }
            if resp.clicked() {
                clicked = Some(slot.id);
            }
        }
    }
    paint_icon_strip_visuals(
        ui,
        origin.min,
        1.0,
        &layout,
        items,
        &tokens,
        th,
        hovered_id,
        current_palette_label(ui.ctx()).as_deref(),
        strip_associate(ui.ctx()),
    );

    if let Some(id) = hovered_id {
        let (since, mut blend) = match hover {
            Some((hid, since, blend)) if hid == id => (since, blend),
            _ => (now, 0.0),
        };
        let item = shown.iter().find(|item| item.id == id);
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

fn group_flyout_runs<'a>(
    items: &[&'a FlyoutItem<'a>],
) -> Vec<(Option<&'a str>, Vec<&'a FlyoutItem<'a>>)> {
    let mut out: Vec<(Option<&'a str>, Vec<&'a FlyoutItem<'a>>)> = Vec::new();
    for item in items {
        match out.last_mut() {
            Some((g, list)) if *g == item.group => list.push(*item),
            _ => out.push((item.group, vec![*item])),
        }
    }
    out
}

struct StripSlot<'a> {
    item: &'a FlyoutItem<'a>,
    off: Vec2,
    size: Vec2,
}

struct StripGroup<'a> {
    label: Option<&'a str>,
    origin: Vec2,
    frame: Vec2,
    slots: Vec<StripSlot<'a>>,
}

struct StripLayout<'a> {
    cluster: Vec2,
    groups: Vec<StripGroup<'a>>,
}

fn tertiary_col_w(
    ctx: &egui::Context,
    pair: &[&FlyoutItem<'_>],
    icon: f32,
    p: &DockPaletteTokens,
) -> f32 {
    let th = tertiary_slot_h(icon, p);
    let track_w = (th * 1.9).max(16.0);
    let font = FontId::proportional(p.labeled_text_size);
    let mut label_w = 0.0_f32;
    for item in pair {
        let g = ctx.fonts(|f| f.layout_no_wrap(item.label.to_owned(), font.clone(), Color32::WHITE));
        label_w = label_w.max(g.size().x);
    }
    track_w + 4.0 + label_w + 4.0
}

fn layout_group_slots<'a>(
    ctx: &egui::Context,
    items: &[&'a FlyoutItem<'a>],
    icon: f32,
    gap: f32,
    side: DockSide,
    p: &DockPaletteTokens,
    inner: f32,
) -> (Vec<StripSlot<'a>>, Vec2) {
    let all_icons = !items.is_empty() && items.iter().all(|it| it.role == FlyoutRole::Icon);
    if all_icons {
        let n = items.len();
        let linear = n as f32 * icon + n.saturating_sub(1) as f32 * gap;
        if linear > inner + 0.5 {
            let (offs, content) = row_pack_offsets(n, icon, gap, inner, side);
            let slots = items
                .iter()
                .zip(offs)
                .map(|(item, off)| StripSlot {
                    item: *item,
                    off,
                    size: Vec2::splat(icon),
                })
                .collect();
            return (slots, content);
        }
    }
    let th = tertiary_slot_h(icon, p);
    let mut slots = Vec::new();
    let mut i = 0;
    match side {
        DockSide::BottomCenter => {
            let mut x = 0.0;
            let mut y = 0.0;
            let mut row_h = icon;
            while i < items.len() {
                if items[i].role == FlyoutRole::Toggle {
                    let mut pair = vec![items[i]];
                    if i + 1 < items.len() && items[i + 1].role == FlyoutRole::Toggle {
                        pair.push(items[i + 1]);
                        i += 2;
                    } else {
                        i += 1;
                    }
                    let w = tertiary_col_w(ctx, &pair, icon, p);
                    if x > 0.0 && x + w > inner {
                        x = 0.0;
                        y += row_h + gap;
                        row_h = icon;
                    }
                    for (k, item) in pair.into_iter().enumerate() {
                        slots.push(StripSlot {
                            item,
                            off: Vec2::new(x, y + k as f32 * (th + p.tertiary_stack_gap)),
                            size: Vec2::new(w, th),
                        });
                    }
                    row_h = row_h.max(icon);
                    x += w + gap;
                } else {
                    if x > 0.0 && x + icon > inner {
                        x = 0.0;
                        y += row_h + gap;
                        row_h = icon;
                    }
                    slots.push(StripSlot {
                        item: items[i],
                        off: Vec2::new(x, y),
                        size: Vec2::splat(icon),
                    });
                    x += icon + gap;
                    i += 1;
                }
            }
        }
        DockSide::LeftCenter => {
            let mut y = 0.0;
            let mut max_w = icon;
            while i < items.len() {
                if items[i].role == FlyoutRole::Toggle {
                    let mut pair = vec![items[i]];
                    if i + 1 < items.len() && items[i + 1].role == FlyoutRole::Toggle {
                        pair.push(items[i + 1]);
                        i += 2;
                    } else {
                        i += 1;
                    }
                    let w = tertiary_col_w(ctx, &pair, icon, p);
                    max_w = max_w.max(w);
                    for (k, item) in pair.into_iter().enumerate() {
                        slots.push(StripSlot {
                            item,
                            off: Vec2::new(0.0, y + k as f32 * (th + p.tertiary_stack_gap)),
                            size: Vec2::new(w, th),
                        });
                    }
                    y += icon + gap;
                } else {
                    slots.push(StripSlot {
                        item: items[i],
                        off: Vec2::new((max_w - icon) * 0.5, y),
                        size: Vec2::splat(icon),
                    });
                    y += icon + gap;
                    i += 1;
                }
            }
            for slot in &mut slots {
                if slot.item.role == FlyoutRole::Icon {
                    slot.off.x = (max_w - icon) * 0.5;
                }
            }
        }
    }
    let w = slots
        .iter()
        .map(|s| s.off.x + s.size.x)
        .fold(0.0_f32, f32::max);
    let h = slots
        .iter()
        .map(|s| s.off.y + s.size.y)
        .fold(0.0_f32, f32::max);
    (slots, Vec2::new(w, h.max(icon)))
}

fn layout_fieldset_strip<'a>(
    ctx: &egui::Context,
    groups: &[(Option<&'a str>, Vec<&'a FlyoutItem<'a>>)],
    icon: f32,
    gap: f32,
    budget: f32,
    side: DockSide,
    p: &DockPaletteTokens,
) -> StripLayout<'a> {
    let mut laid = Vec::new();
    let mut cursor = 0.0;
    let cross = 0.0;
    let mut row_span = 0.0_f32;
    let mut max_along = 0.0_f32;
    let open_inner = 16_384.0;
    let mut parts: Vec<(Option<&'a str>, Vec<StripSlot<'a>>, Vec2)> = Vec::new();
    for (label, items) in groups {
        let (slots, content) = layout_group_slots(ctx, items, icon, gap, side, p, open_inner);
        parts.push((*label, slots, content));
    }
    let along_of = |content: Vec2| match side {
        DockSide::BottomCenter => content.x + p.group_pad * 2.0,
        DockSide::LeftCenter => content.y + p.group_pad * 2.0,
    };
    let natural: f32 = parts
        .iter()
        .map(|(_, _, c)| along_of(*c))
        .sum::<f32>()
        + p.group_gap * parts.len().saturating_sub(1) as f32;
    if let Some(id) = current_palette(ctx) {
        ctx.data_mut(|d| {
            let mut map = d
                .get_temp::<HashMap<String, f32>>(egui::Id::new(STRIP_NATURAL_ID))
                .unwrap_or_default();
            map.insert(id.to_owned(), natural);
            d.insert_temp(egui::Id::new(STRIP_NATURAL_ID), map);
        });
    }
    if natural > budget + 0.5 && !parts.is_empty() {
        let counts: Vec<usize> = groups
            .iter()
            .map(|(_, items)| {
                if !items.is_empty() && items.iter().all(|it| it.role == FlyoutRole::Icon) {
                    items.len()
                } else {
                    0
                }
            })
            .collect();
        let start: Vec<f32> = parts.iter().map(|(_, _, c)| along_of(*c) - p.group_pad * 2.0).collect();
        let inners = accordion_peel_inners(
            &counts,
            &start,
            icon,
            gap,
            p.group_pad,
            p.group_gap,
            budget,
        );
        parts.clear();
        for ((label, items), inner) in groups.iter().zip(inners) {
            let (slots, content) = layout_group_slots(ctx, items, icon, gap, side, p, inner.max(icon));
            parts.push((*label, slots, content));
        }
    }
    for (label, slots, content) in parts {
        let frame = Vec2::new(content.x + p.group_pad * 2.0, content.y + p.group_pad * 2.0);
        let along = match side {
            DockSide::BottomCenter => frame.x,
            DockSide::LeftCenter => frame.y,
        };
        let across = match side {
            DockSide::BottomCenter => frame.y,
            DockSide::LeftCenter => frame.x,
        };
        let origin = match side {
            DockSide::BottomCenter => Vec2::new(cursor, cross),
            DockSide::LeftCenter => Vec2::new(cross, cursor),
        };
        let slots = slots
            .into_iter()
            .map(|mut s| {
                s.off += Vec2::splat(p.group_pad);
                s
            })
            .collect();
        laid.push(StripGroup {
            label,
            origin,
            frame,
            slots,
        });
        cursor += along + p.group_gap;
        row_span = row_span.max(across);
        max_along = max_along.max(cursor - p.group_gap);
    }
    let title_overhang = p.group_label_size * 0.5 + p.pallet_label_lift.max(0.0) + 1.0;
    let band = category_rule_band(p);
    let cluster = match side {
        DockSide::BottomCenter => {
            Vec2::new(max_along, cross + row_span + title_overhang + band)
        }
        DockSide::LeftCenter => {
            Vec2::new(cross + row_span, max_along + title_overhang + band)
        }
    };
    for group in &mut laid {
        match side {
            DockSide::BottomCenter => {
                // Sit every pallet on the category rule (the basedatum).
                group.origin.y = title_overhang + (row_span - group.frame.y).max(0.0);
            }
            DockSide::LeftCenter => {
                group.origin.y += title_overhang;
            }
        }
    }
    StripLayout {
        cluster,
        groups: laid,
    }
}

fn paint_fieldset_frame(
    ui: &egui::Ui,
    frame: Rect,
    pallet_name: Option<&str>,
    th: &DockThemeTokens,
    p: &DockPaletteTokens,
    scale: f32,
    associate: bool,
) {
    let radius = p.group_radius * scale;
    let fill_k = if associate { p.associate_fill } else { 0.42 };
    let mut fill = th.popover_fill_color().gamma_multiply(fill_k);
    if associate {
        fill = associate_shade(fill, ui.visuals().dark_mode, p.associate_tint);
    }
    ui.painter().rect_filled(frame, radius, fill);
    let title = layout_border_title(
        ui,
        frame,
        pallet_name,
        radius,
        p.group_label_inset * scale,
        p.group_label_size * scale,
        p.pallet_label_lift * scale,
        p.rule_text_gap * scale,
        th.title_color(),
    );
    let stroke_w = p.group_stroke
        * scale
        * if associate { p.associate_stroke } else { 1.0 };
    if stroke_w > 0.0 {
        let mut stroke_c = th.border_color()
            .gamma_multiply(if associate { 1.0 } else { 0.9 });
        if associate {
            stroke_c = associate_shade(stroke_c, ui.visuals().dark_mode, p.associate_tint);
        }
        let stroke = Stroke::new(stroke_w.max(0.5), stroke_c);
        match title.as_ref() {
            Some(t) if t.gap_r > t.gap_l => {
                stroke_round_rect_top_gap(ui.painter(), frame, radius, stroke, t.gap_l, t.gap_r);
            }
            _ => {
                ui.painter()
                    .rect_stroke(frame, radius, stroke, egui::StrokeKind::Inside);
            }
        }
    }
    if let Some(t) = title {
        ui.painter().galley(t.pos, t.galley, th.title_color());
    }
}

struct BorderTitle {
    galley: std::sync::Arc<egui::Galley>,
    pos: Pos2,
    gap_l: f32,
    gap_r: f32,
}

/// Pallet name sitting in a gap in the top stroke of that box.
fn layout_border_title(
    ui: &egui::Ui,
    frame: Rect,
    label: Option<&str>,
    radius: f32,
    inset: f32,
    font_px: f32,
    lift: f32,
    gap: f32,
    color: Color32,
) -> Option<BorderTitle> {
    let label = label.filter(|s| !s.is_empty())?;
    if font_px < 6.0 {
        return None;
    }
    let galley = ui.fonts(|f| {
        f.layout_no_wrap(label.to_owned(), FontId::proportional(font_px), color)
    });
    let pos = Pos2::new(
        frame.left() + radius + inset,
        frame.top() - galley.size().y * 0.5 - lift,
    );
    let gap_l = pos.x - gap;
    let gap_r = pos.x + galley.size().x + gap;
    Some(BorderTitle {
        galley,
        pos,
        gap_l,
        gap_r,
    })
}

/// Inside-stroke rounded rect with a break in the top edge for the pallet name.
fn stroke_round_rect_top_gap(
    painter: &egui::Painter,
    frame: Rect,
    radius: f32,
    stroke: Stroke,
    gap_l: f32,
    gap_r: f32,
) {
    let hw = stroke.width * 0.5;
    let r = frame.shrink(hw);
    if r.width() <= 0.0 || r.height() <= 0.0 {
        return;
    }
    let rad = (radius - hw)
        .max(0.0)
        .min(r.width() * 0.5)
        .min(r.height() * 0.5);
    let mut pts = Vec::new();
    let push_arc = |pts: &mut Vec<Pos2>, c: Pos2, a0: f32, a1: f32| {
        const N: usize = 6;
        for i in 0..=N {
            let t = i as f32 / N as f32;
            let a = a0 + (a1 - a0) * t;
            pts.push(c + Vec2::new(rad * a.cos(), rad * a.sin()));
        }
    };
    let tau = std::f32::consts::TAU;
    // Clockwise from the right side of the title gap, around to the left side.
    if gap_r < r.right() - rad {
        pts.push(Pos2::new(gap_r.max(r.left() + rad), r.top()));
        pts.push(Pos2::new(r.right() - rad, r.top()));
    } else {
        pts.push(Pos2::new(r.right() - rad, r.top()));
    }
    if rad > 0.5 {
        push_arc(&mut pts, Pos2::new(r.right() - rad, r.top() + rad), -tau * 0.25, 0.0);
    }
    pts.push(Pos2::new(r.right(), r.bottom() - rad));
    if rad > 0.5 {
        push_arc(&mut pts, Pos2::new(r.right() - rad, r.bottom() - rad), 0.0, tau * 0.25);
    }
    pts.push(Pos2::new(r.left() + rad, r.bottom()));
    if rad > 0.5 {
        push_arc(&mut pts, Pos2::new(r.left() + rad, r.bottom() - rad), tau * 0.25, tau * 0.5);
    }
    pts.push(Pos2::new(r.left(), r.top() + rad));
    if rad > 0.5 {
        push_arc(&mut pts, Pos2::new(r.left() + rad, r.top() + rad), tau * 0.5, tau * 0.75);
    }
    if gap_l > r.left() + rad {
        pts.push(Pos2::new(gap_l.min(r.right() - rad), r.top()));
    } else {
        pts.push(Pos2::new(r.left() + rad, r.top()));
    }
    if pts.len() >= 2 {
        painter.add(Shape::line(pts, stroke));
    }
}

/// Space under the whole strip for one category rule and its title.
fn category_rule_band(p: &DockPaletteTokens) -> f32 {
    (p.category_label_size * 0.5 - p.group_label_lift + p.rule_offset).max(p.rule_stroke) + 2.0
}

/// Category name centered on one rule under every pallet in this flyout.
fn paint_rule_title(
    ui: &egui::Ui,
    frame: Rect,
    label: Option<&str>,
    font_px: f32,
    text: Color32,
    rule: Color32,
    p: &DockPaletteTokens,
    scale: f32,
) {
    let label = label.filter(|s| !s.is_empty());
    let rule_y = frame.bottom() + p.rule_offset * scale;
    let x0 = frame.left() - p.rule_extent * scale;
    let x1 = frame.right() + p.rule_extent * scale;
    let stroke_w = p.rule_stroke * scale;
    let gap = p.rule_text_gap * scale;

    let mut hole_l = x0;
    let mut hole_r = x0;
    if let Some(label) = label {
        if font_px >= 6.0 {
            let galley = ui.fonts(|f| {
                f.layout_no_wrap(label.to_owned(), FontId::proportional(font_px), text)
            });
            let text_pos = Pos2::new(
                (x0 + x1) * 0.5 - galley.size().x * 0.5,
                rule_y - galley.size().y * 0.5 - p.group_label_lift * scale,
            );
            hole_l = text_pos.x - gap;
            hole_r = text_pos.x + galley.size().x + gap;
            ui.painter().galley(text_pos, galley, text);
        }
    }

    if stroke_w > 0.0 && x1 > x0 {
        let stroke = Stroke::new(stroke_w.max(0.5), rule);
        let painter = ui.painter();
        if label.is_some() && hole_r > hole_l {
            if hole_l > x0 {
                painter.line_segment([Pos2::new(x0, rule_y), Pos2::new(hole_l, rule_y)], stroke);
            }
            if x1 > hole_r {
                painter.line_segment([Pos2::new(hole_r, rule_y), Pos2::new(x1, rule_y)], stroke);
            }
        } else {
            painter.line_segment([Pos2::new(x0, rule_y), Pos2::new(x1, rule_y)], stroke);
        }
    }
}

fn paint_icon_strip_visuals(
    ui: &egui::Ui,
    origin: Pos2,
    scale: f32,
    layout: &IconStripLayout,
    items: &[FlyoutItem<'_>],
    tokens: &DockTokens,
    th: &DockThemeTokens,
    hovered_id: Option<&str>,
    palette_name: Option<&str>,
    associate: bool,
) {
    let theme = flyout_list_theme(ui);
    let icon = flyout_icon_size(tokens) * scale;
    let p = &tokens.palette;
    let mut union = Rect::NOTHING;
    for g in &layout.groups {
        let frame = Rect::from_min_size(origin + g.origin * scale, g.frame * scale);
        union = union.union(frame);
        paint_fieldset_frame(ui, frame, g.label.as_deref(), th, p, scale, associate);
        for s in &g.slots {
            let rect = Rect::from_min_size(frame.min + s.off * scale, s.size * scale);
            let Some(item) = items.iter().find(|it| it.id == s.id) else {
                continue;
            };
            let hovered = hovered_id == Some(s.id);
            match item.role {
                FlyoutRole::Toggle => {
                    paint_tertiary_toggle(
                        ui,
                        rect,
                        item,
                        hovered,
                        theme,
                        icon,
                        &tokens.palette,
                        scale,
                    );
                }
                FlyoutRole::Icon => {
                    paint_secondary_icon(ui.painter(), rect, item, hovered, th, scale);
                }
            }
        }
    }
    if union.is_positive() {
        paint_rule_title(
            ui,
            union,
            palette_name,
            p.category_label_size * scale,
            th.category_color(),
            th.rule_color(),
            p,
            scale,
        );
    }
}

fn paint_secondary_icon(
    painter: &egui::Painter,
    rect: Rect,
    item: &FlyoutItem<'_>,
    hovered: bool,
    th: &DockThemeTokens,
    scale: f32,
) {
    let r = (rect.width() * 0.5 - 0.5 * scale).max(1.0);
    let stroke_c = if item.active || hovered {
        th.text_color()
    } else {
        th.muted_text_color().gamma_multiply(0.95)
    };
    if item.active {
        painter.circle_filled(
            rect.center(),
            r,
            th.icon_active_color().gamma_multiply(0.14),
        );
    } else if hovered {
        painter.circle_filled(rect.center(), r, th.icon_hover_color().gamma_multiply(0.10));
    }
    painter.circle_stroke(
        rect.center(),
        r,
        Stroke::new((1.0 * scale).max(0.6), stroke_c),
    );
    paint_builtin_icon(
        painter,
        rect.shrink((rect.width() * 0.22).max(2.5 * scale)),
        item.icon,
        th.text_color(),
    );
}

fn paint_tertiary_toggle(
    ui: &egui::Ui,
    rect: Rect,
    item: &FlyoutItem<'_>,
    hovered: bool,
    theme: SidebarTheme,
    icon: f32,
    p: &DockPaletteTokens,
    scale: f32,
) {
    let th = tertiary_slot_h(icon, p);
    let track_h = (rect.height() * 0.72).min(th * 0.78);
    let track_w = track_h * 1.9;
    let track = Rect::from_center_size(
        Pos2::new(rect.left() + track_w * 0.5, rect.center().y),
        Vec2::new(track_w, track_h),
    );
    let t = ui.ctx().animate_bool_with_time(
        ui.id().with(("strip_slide", item.id)),
        item.active,
        TOGGLE_SLIDE_SECS,
    );
    paint_sidebar_icon_pill(
        ui.painter(),
        track,
        item.active,
        hovered,
        t,
        theme,
        |p, r, c| paint_builtin_icon(p, r, item.icon, c),
    );
    let font_px = p.labeled_text_size * scale;
    if font_px < 6.0 {
        return;
    }
    let font = FontId::proportional(font_px);
    let color = if item.active { theme.ink } else { theme.sub };
    let galley = ui.fonts(|f| f.layout_no_wrap(item.label.to_owned(), font, color));
    ui.painter().galley(
        Pos2::new(track.right() + 4.0, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
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
/// (both Panel and Action items report clicks so apps can react), plus
/// a drop-to-canvas request from a strip's fourth dot.
///
/// `restore_pins`: panel ids to restore as pinned on the dock's first
/// rendered frame (persisted palettes, e.g. from `ChromePrefs`). Ids are
/// matched against `items`, so stale entries are ignored harmlessly. Read
/// the live set back with [`pinned_ids`] to persist changes.
/// `restore_icon_strips`: bodies that should open in free-space icon-strip mode.
/// `restore_hidden`: tools hidden from each palette's strip (`palette → tool ids`).
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
    restore_hidden: &[(String, Vec<String>)],
    restore_bar_collapsed: bool,
    mut panel_body: impl FnMut(&mut egui::Ui, &'static str),
) -> DockOutcome {
    let mut tokens = crate::tokens::current().dock;
    tokens.normalize();
    let th = theme(palette, &tokens);
    let state_id = egui::Id::new(("floating_dock", id));
    let mut state = ctx.data_mut(|d| d.get_temp::<DockState>(state_id).unwrap_or_default());
    let now = ctx.input(|i| i.time);
    let mut clicked: Option<&'static str> = None;
    let mut drop_to_canvas: Option<&'static str> = None;

    let visible: Vec<&DockItem<'_>> = items.iter().filter(|item| item.visible).collect();
    if visible.is_empty() {
        return DockOutcome::default();
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
        state.hidden = restore_hidden
            .iter()
            .cloned()
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect();
        state.bar_collapsed = restore_bar_collapsed;
    }
    sync_hidden_to_ctx(ctx, &state.hidden);

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
    if let Some(adv) = state.advanced {
        if !visible
            .iter()
            .any(|item| item.id == adv && item.kind.opens_body())
        {
            state.advanced = None;
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
        if crate::tuning::dock_advanced_preview()
            && visible
                .iter()
                .any(|item| item.id == forced && item.kind.opens_body())
        {
            state.icon_strip = true;
            state.advanced = Some(forced);
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
    let palette_hover: Option<&'static str> = pointer.and_then(|p| {
        state
            .last_panel_rects
            .iter()
            .find(|(_, r)| r.expand(4.0).contains(p))
            .map(|(id, _)| *id)
    });
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(DOCK_SIDE_ID), side));

    let mut icon_rects: HashMap<&'static str, Rect> = HashMap::new();
    let mut label_hover_candidate: Option<&'static str> = None;
    // Pointer is over a pinned / volatile-open icon — previous chips must die
    // immediately (the bar still counts as "inside", so close-delay alone
    // would leave the prior name stuck).
    let mut label_blocked_by_open = false;
    let mut hovered_icon: Option<&'static str> = None;
    let bar_response = (!state.bar_collapsed).then(|| bar_area.show(ctx, |ui| {
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
                let associated = palette_hover == Some(item.id)
                    || (resp.hovered() && (is_pinned || is_preview));
                let base = th.icon_fill_color();
                let fill = if associated {
                    associate_shade(base, ui.visuals().dark_mode, tokens.palette.associate_tint)
                } else if item.active || is_pinned || is_preview {
                    mix_icon_fill(base, th.icon_active_color(), ICON_ACTIVE_MIX)
                } else if hovered {
                    mix_icon_fill(base, th.icon_hover_color(), ICON_HOVER_MIX)
                } else {
                    base
                };
                let dark = ui.visuals().dark_mode;
                let (outline_w, outline_tint) = icon_outline(associated, is_pinned, &tokens.palette);
                let outline = if outline_tint > 0.0 {
                    associate_shade(th.border_color(), dark, outline_tint)
                } else {
                    th.border_color()
                };
                paint_squircle(
                    ui.painter(),
                    rect.shrink(0.5),
                    fill,
                    Stroke::new(outline_w, outline),
                    tokens.squircle_exponent,
                );
                paint_builtin_icon(ui.painter(), rect.shrink(7.0), item.icon, th.text_color());

                if resp.hovered() {
                    hovered_icon = Some(item.id);
                }
                if hovered {
                    state.last_inside_time = now;
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
                            if let Some(idx) = state.pinned.iter().position(|p| *p == item.id) {
                                state.pinned.remove(idx);
                            }
                            state.panel_open.remove(&item.id);
                            if state.advanced == Some(item.id) {
                                state.advanced = None;
                            }
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
    }));
    let bar_rect = if let Some(resp) = bar_response {
        state.last_icon_rects = icon_rects.clone();
        resp.response.rect
    } else {
        icon_rects = state.last_icon_rects.clone();
        Rect::NOTHING
    };

    let host_associate = hovered_icon
        .filter(|id| state.pinned.contains(id) || state.body_preview == Some(*id))
        .or(palette_hover);

    if !state.bar_collapsed && bar_rect.width() > 4.0 {
        let zones = collapse_zone_rects(side, bar_rect, canvas, tokens.palette.collapse_zone);
        let mut zone_hover = false;
        for (i, z) in zones.iter().enumerate() {
            if z.width() < 6.0 || z.height() < 6.0 {
                continue;
            }
            let shown = egui::Area::new(state_id.with(("cz", i)))
                .order(egui::Order::Foreground)
                .fixed_pos(z.min)
                .constrain(false)
                .show(ctx, |ui| ui.allocate_exact_size(z.size(), Sense::click()));
            let resp = shown.inner.1;
            if resp.hovered() {
                zone_hover = true;
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                state.bar_collapsed = true;
            }
        }
        if zone_hover {
            let tip = match side {
                DockSide::BottomCenter => {
                    Pos2::new(bar_rect.center().x, bar_rect.bottom() + 10.0)
                }
                DockSide::LeftCenter => Pos2::new(bar_rect.left() - 10.0, bar_rect.center().y),
            };
            paint_tiny_chevron(
                &ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Foreground,
                    state_id.with("cz_arrow"),
                )),
                tip,
                10.0,
                th.text_color().gamma_multiply(0.75),
                matches!(side, DockSide::BottomCenter),
            );
        }
    } else if state.bar_collapsed {
        let bp = &tokens.palette;
        let shown = egui::Area::new(state_id.with("blister"))
            .order(egui::Order::Foreground)
            .pivot(Align2::CENTER_TOP)
            .fixed_pos(Pos2::new(
                canvas.center().x,
                canvas.bottom() - bp.blister_height * 0.35 + bp.blister_sink,
            ))
            .constrain(false)
            .show(ctx, |ui| {
                ui.allocate_exact_size(
                    Vec2::new(bp.blister_width, bp.blister_height),
                    Sense::click(),
                )
            });
        let (rect, resp) = shown.inner;
        let dark = ctx.style().visuals.dark_mode;
        let fill = associate_shade(th.popover_fill_color(), dark, 0.12);
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            state_id.with("blister_paint"),
        ));
        painter.rect_filled(rect, bp.blister_height * 0.5, fill);
        painter.rect_stroke(
            rect,
            bp.blister_height * 0.5,
            Stroke::new(0.8_f32, th.border_color().gamma_multiply(0.45)),
            egui::StrokeKind::Inside,
        );
        if resp.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            paint_tiny_chevron(
                &painter,
                Pos2::new(rect.center().x, rect.top() - 8.0),
                11.0,
                th.text_color().gamma_multiply(0.85),
                false,
            );
        }
        if resp.clicked() {
            state.bar_collapsed = false;
        }
    }
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
                    tokens.palette.caption_height,
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
                    None,
                    host_associate == Some(preview_id)
                        || pointer.is_some_and(|p| {
                            last_size.is_some_and(|sz| {
                                body_rect_at(origin, pivot, sz).expand(2.0).contains(p)
                            })
                        }),
                    |ui| {
                        set_body_layout(ui.ctx(), layout);
                        set_current_palette(ui.ctx(), preview_id, &label);
                        panel_body(ui, preview_id);
                    },
                );
                apply_layout_toggle(&mut state, preview_id, render.toggle_layout);
                if render.advanced {
                    state.advanced = if state.advanced == Some(preview_id) {
                        None
                    } else {
                        Some(preview_id)
                    };
                }
                if render.drop_to_canvas {
                    drop_to_canvas = Some(preview_id);
                }
                if render.minimize {
                    state.body_preview = None;
                    state.panel_open.remove(&preview_id);
                    if state.advanced == Some(preview_id) {
                        state.advanced = None;
                    }
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
                state.last_panel_rects.insert(preview_id, render.rect);
                union_panels = render.rect;
                if state.advanced == Some(preview_id) {
                    let title = visible
                        .iter()
                        .find(|item| item.id == preview_id)
                        .map(|item| item.label)
                        .unwrap_or("tools");
                    let adv = show_advanced_overlay(
                        ctx,
                        state_id.with(("advanced", preview_id)),
                        &tokens,
                        canvas,
                        title,
                        open,
                        |ui| {
                            set_body_layout(ui.ctx(), DockBodyLayout::Advanced);
                            set_current_palette(ui.ctx(), preview_id, &label);
                            panel_body(ui, preview_id);
                        },
                    );
                    if adv.minimize {
                        state.advanced = None;
                    }
                    union_panels = union_panels.union(adv.rect);
                    if pointer.is_some_and(|p| adv.rect.contains(p)) {
                        state.last_inside_time = now;
                    }
                }
                if layout != DockBodyLayout::Icons {
                    if let Some(p) = pointer {
                        if border_hovered(render.rect, p, tokens.tracer_border_hit) {
                            if let Some(&icon_rect) = icon_rects.get(&preview_id) {
                                tracer_for = Some((preview_id, icon_rect, render.rect));
                            }
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
    let strip_ids: Vec<&'static str> = open
        .iter()
        .copied()
        .filter(|oid| {
            let kind = visible
                .iter()
                .find(|item| item.id == *oid)
                .map(|item| item.kind)
                .unwrap_or(DockItemKind::Dashboard);
            body_layout_for(&state, oid, kind) == DockBodyLayout::Icons
        })
        .collect();
    let usable = strip_icon_budget(side, canvas, &tokens);
    let naturals: HashMap<String, f32> = ctx.data(|d| {
        d.get_temp::<HashMap<String, f32>>(egui::Id::new(STRIP_NATURAL_ID))
            .unwrap_or_default()
    });
    // Only wrap inside a palette when the natural band no longer fits —
    // every category stacks one column in the same round.
    let icon_px = flyout_icon_size(&tokens);
    let icon_gap = (tokens.icon_gap * tokens.flyout_icon_scale).max(4.0);
    let strip_budgets = accordion_band_budgets(
        &strip_ids,
        &naturals,
        usable,
        tokens.stack_gap,
        icon_px + tokens.palette.group_pad * 2.0,
        icon_px + icon_gap,
    );
    let mut open_meta: Vec<(&'static str, Rect, Vec2)> = open
        .iter()
        .filter_map(|oid| {
            let icon = *icon_rects.get(oid)?;
            let size = panel_size_for(&state, oid, &tokens);
            Some((*oid, icon, size))
        })
        .collect();
    if let Some(budgets) = strip_budgets.as_ref() {
        for (id, _, size) in &mut open_meta {
            let Some(&b) = budgets.get(*id) else {
                continue;
            };
            match side {
                DockSide::BottomCenter => size.x = size.x.min(b),
                DockSide::LeftCenter => size.y = size.y.min(b),
            }
        }
    }
    let origins = layout_panel_origins(side, &open_meta, &tokens, canvas, state.bar_collapsed);
    // Strip ⇄ stacked switches every palette at once, so exactly one palette
    // offers it: the last one along the dock (rightmost on a bottom dock,
    // bottom-most on a left dock). The rest carry three dots.
    let toggle_owner = toggle_palette(side, &open, &origins);

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
        let mut last_size = state.panel_sizes.get(oid).copied();
        if let (Some(sz), Some(b)) = (
            last_size.as_mut(),
            strip_budgets.as_ref().and_then(|m| m.get(*oid).copied()),
        ) {
            match side {
                DockSide::BottomCenter => sz.x = sz.x.min(b),
                DockSide::LeftCenter => sz.y = sz.y.min(b),
            }
        }
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
            tokens.palette.caption_height,
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
            toggle_owner == Some(*oid),
            strip_budgets.as_ref().and_then(|m| m.get(*oid).copied()),
            host_associate == Some(*oid)
                || pointer.is_some_and(|p| {
                    last_size.is_some_and(|sz| {
                        body_rect_at(origin, pivot, sz).expand(2.0).contains(p)
                    })
                }),
            |ui| {
                set_body_layout(ui.ctx(), layout);
                set_current_palette(ui.ctx(), oid, &label);
                panel_body(ui, oid);
            },
        );
        apply_layout_toggle(&mut state, oid, render.toggle_layout);
        if render.advanced {
            state.advanced = if state.advanced == Some(*oid) {
                None
            } else {
                Some(*oid)
            };
        }
        if render.drop_to_canvas {
            drop_to_canvas = Some(*oid);
        }
        if render.minimize {
            if let Some(idx) = state.pinned.iter().position(|p| p == oid) {
                state.pinned.remove(idx);
            }
            state.panel_open.remove(oid);
            if state.advanced == Some(*oid) {
                state.advanced = None;
            }
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
        state.last_panel_rects.insert(*oid, panel_rect);
        if union_panels == Rect::NOTHING {
            union_panels = panel_rect;
        } else {
            union_panels = union_panels.union(panel_rect);
        }

        if state.advanced == Some(*oid) {
            let adv = show_advanced_overlay(
                ctx,
                state_id.with(("advanced", *oid)),
                &tokens,
                canvas,
                &label,
                open_anim,
                |ui| {
                    set_body_layout(ui.ctx(), DockBodyLayout::Advanced);
                    set_current_palette(ui.ctx(), oid, &label);
                    panel_body(ui, oid);
                },
            );
            if adv.minimize {
                state.advanced = None;
            }
            union_panels = union_panels.union(adv.rect);
            if pointer.is_some_and(|p| adv.rect.contains(p)) {
                state.last_inside_time = now;
            }
        }

        if layout != DockBodyLayout::Icons {
            if let Some(p) = pointer {
                if border_hovered(panel_rect, p, tokens.tracer_border_hit) {
                    if let Some(&icon_rect) = icon_rects.get(oid) {
                        tracer_for = Some((*oid, icon_rect, panel_rect));
                    }
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
    state.last_panel_rects.retain(|id, _| {
        new_sizes.contains_key(id) || state.body_preview == Some(*id)
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
        bar_rect.expand(tokens.palette.collapse_zone.max(4.0)).contains(p)
            || (state.bar_collapsed
                && Rect::from_center_size(
                    Pos2::new(canvas.center().x, canvas.bottom()),
                    Vec2::new(tokens.palette.blister_width + 16.0, 36.0),
                )
                .contains(p))
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
    state.hidden = hidden_from_ctx(ctx);
    if !state.icon_strip && !crate::tuning::dock_advanced_preview() {
        state.advanced = None;
    }
    state.last_union_panels = (union_panels != Rect::NOTHING).then_some(union_panels);
    ctx.data_mut(|d| d.insert_temp(state_id, state));

    DockOutcome {
        clicked,
        drop_to_canvas,
    }
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
    advanced: bool,
    drop_to_canvas: bool,
    rect: Rect,
    content_h: f32,
}

/// Which open palette offers the strip ⇄ stacked toggle.
///
/// The toggle is dock-wide, so repeating it on every palette is three copies
/// of one switch. It lives on the last palette along the dock — rightmost on a
/// bottom dock, bottom-most on a left dock — where the eye finishes reading
/// the band. `None` when nothing is pinned; the volatile body takes it then.
fn toggle_palette(
    side: DockSide,
    open: &[&'static str],
    origins: &HashMap<&'static str, Pos2>,
) -> Option<&'static str> {
    let along = |p: &Pos2| match side {
        DockSide::BottomCenter => p.x,
        DockSide::LeftCenter => p.y,
    };
    open.iter()
        .filter_map(|id| origins.get(id).map(|o| (*id, along(o))))
        .reduce(|best, next| if next.1 >= best.1 { next } else { best })
        .map(|(id, _)| id)
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
    caption_h: f32,
    last_size: Option<Vec2>,
) -> egui::Area {
    let default_size = if layout == DockBodyLayout::Icons {
        last_size.unwrap_or(Vec2::new(width.min(320.0), 48.0))
    } else {
        Vec2::new(width, body_max_h + caption_h + PANEL_SEPARATOR_H)
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
    _kind: DockItemKind,
    layout: DockBodyLayout,
    width: f32,
    body_max_h: f32,
    last_content_h: f32,
    open_anim: f32,
    canvas: Rect,
    side: DockSide,
    show_strip_toggle: bool,
    icon_budget: Option<f32>,
    associate: bool,
    add_body: impl FnOnce(&mut egui::Ui),
) -> PanelRender {
    set_strip_associate(ctx, associate);
    if layout == DockBodyLayout::Icons {
        set_strip_budget(
            ctx,
            icon_budget.unwrap_or_else(|| strip_icon_budget(side, canvas, tokens)),
        );
        show_free_strip(
            ctx,
            area,
            tokens,
            th,
            pinned,
            open_anim,
            show_strip_toggle,
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
            "Icon strip",
            width,
            body_max_h,
            last_content_h,
            open_anim,
            show_strip_toggle,
            add_body,
        )
    }
}

/// Fieldset icon strip: labeled frames of circular secondary icons and
/// two-high tertiary toggles. A tight dot column on the right of this
/// cluster (minimize, Advanced, drop to canvas, then stacked-view).
#[allow(clippy::too_many_arguments)]
fn show_free_strip(
    ctx: &egui::Context,
    area: egui::Area,
    tokens: &DockTokens,
    th: &DockThemeTokens,
    pinned: bool,
    open_anim: f32,
    show_toggle: bool,
    add_body: impl FnOnce(&mut egui::Ui),
) -> PanelRender {
    let mut minimize = false;
    let mut toggle_layout = false;
    let mut advanced = false;
    let mut drop_to_canvas = false;
    let size = flyout_icon_size(tokens);
    let mut content_h = 0.0;
    let response = area.show(ctx, |ui| {
        ui.set_opacity(open_anim);
        let body = ui.scope(add_body);
        let cluster = body.response.rect;
        content_h = cluster.height();
        let clicks = put_strip_dots(ui, cluster, size, th, &tokens.palette, pinned, show_toggle);
        minimize |= clicks.minimize;
        toggle_layout |= clicks.toggle_layout;
        advanced |= clicks.advanced;
        drop_to_canvas |= clicks.drop_to_canvas;
    });
    PanelRender {
        minimize,
        toggle_layout,
        advanced,
        drop_to_canvas,
        rect: response.response.rect,
        content_h,
    }
}

fn strip_primary_center(cluster: Rect, icon_size: f32, p: &DockPaletteTokens) -> f32 {
    cluster.bottom() - category_rule_band(p) - p.group_pad - icon_size * 0.5
}

#[derive(Default)]
struct StripDotClicks {
    minimize: bool,
    toggle_layout: bool,
    advanced: bool,
    drop_to_canvas: bool,
}

#[derive(Clone, Copy)]
enum DotAxis {
    Vertical,
    Horizontal,
}

/// How many dots a palette shows. Minimize, Advanced, and Drop act on *this*
/// palette, so every one carries them; the layout toggle switches all palettes
/// at once, so it rides on a single palette (see [`toggle_palette`]).
fn dot_count(show_toggle: bool) -> usize {
    if show_toggle {
        4
    } else {
        3
    }
}

/// Extent of the dot run, end dot to end dot.
fn dot_span(p: &DockPaletteTokens, show_toggle: bool) -> f32 {
    p.dot_pitch() * (dot_count(show_toggle) as f32 - 1.0)
}

#[allow(clippy::too_many_arguments)]
fn put_strip_dots(
    ui: &mut egui::Ui,
    cluster: Rect,
    icon_size: f32,
    th: &DockThemeTokens,
    p: &DockPaletteTokens,
    pinned: bool,
    show_toggle: bool,
) -> StripDotClicks {
    let cx = cluster.right() + p.dot_offset;
    let cy = strip_primary_center(cluster, icon_size, p);
    paint_dots(
        ui,
        Pos2::new(cx, cy),
        th,
        p,
        pinned,
        "Stacked view",
        DotAxis::Vertical,
        show_toggle,
    )
}

#[allow(clippy::too_many_arguments)]
fn paint_dots(
    ui: &mut egui::Ui,
    group_center: Pos2,
    th: &DockThemeTokens,
    p: &DockPaletteTokens,
    pinned: bool,
    toggle_label: &str,
    axis: DotAxis,
    show_toggle: bool,
) -> StripDotClicks {
    let pitch = p.dot_pitch();
    let span = dot_span(p, show_toggle);
    let cx = group_center.x;
    let cy = group_center.y;
    let top = cy - span * 0.5;
    let left = cx - span * 0.5;
    // Index into `clicks`, so omitting the toggle cannot shift what the
    // remaining dots do.
    let mut dots: Vec<(usize, &str)> = vec![
        (0, if pinned { "Minimize" } else { "Close" }),
        (2, "Advanced"),
        (3, "Drop to canvas"),
    ];
    if show_toggle {
        dots.push((1, toggle_label));
    }
    let n = dots.len();
    let mut clicks = [false; 4];
    let mut hovered_label: Option<&str> = None;
    for (i, (slot, label)) in dots.into_iter().enumerate() {
        let center = match axis {
            DotAxis::Vertical => Pos2::new(cx, top + i as f32 * pitch),
            DotAxis::Horizontal => Pos2::new(left + i as f32 * pitch, cy),
        };
        let last = i + 1 == n;
        let hit = match axis {
            DotAxis::Vertical => Rect::from_center_size(center, Vec2::new(p.dot_hit_x, pitch)),
            DotAxis::Horizontal => {
                let end = i == 0 || last;
                let w = if end { pitch + p.dot_hit_end } else { pitch };
                let mut r = Rect::from_center_size(center, Vec2::new(w, p.dot_hit_y));
                if i == 0 {
                    r = r.translate(Vec2::new(-p.dot_hit_end * 0.5, 0.0));
                } else if last {
                    r = r.translate(Vec2::new(p.dot_hit_end * 0.5, 0.0));
                }
                r
            }
        };
        let resp = ui
            .allocate_rect(hit, Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let hovered = resp.hovered();
        let is_toggle = slot == 1;
        let fill = if is_toggle {
            if hovered {
                th.text_color()
            } else {
                th.text_color().gamma_multiply(0.92)
            }
        } else if hovered {
            th.text_color()
        } else {
            th.muted_text_color().gamma_multiply(0.85)
        };
        ui.painter().circle_filled(center, p.dot_radius, fill);
        if hovered {
            hovered_label = Some(label);
        }
        clicks[slot] = resp.clicked();
    }
    if let Some(label) = hovered_label {
        let chip_pos = match axis {
            DotAxis::Vertical => Pos2::new(cx, top - p.dot_chip_gap),
            DotAxis::Horizontal => Pos2::new(cx, cy - p.dot_radius - p.dot_chip_gap),
        };
        show_hover_chip(
            ui.ctx(),
            ui.id().with("strip_dot_chip"),
            chip_pos,
            Align2::CENTER_BOTTOM,
            label,
            None,
            None,
            th,
        );
    }
    StripDotClicks {
        minimize: clicks[0],
        toggle_layout: clicks[1],
        advanced: clicks[2],
        drop_to_canvas: clicks[3],
    }
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
    toggle_label: &str,
    width: f32,
    body_max_h: f32,
    last_content_h: f32,
    open_anim: f32,
    show_toggle: bool,
    add_body: impl FnOnce(&mut egui::Ui),
) -> PanelRender {
    let mut minimize = false;
    let mut toggle_layout = false;
    let mut advanced = false;
    let mut drop_to_canvas = false;
    let mut content_h = 0.0;
    let needs_scroll = last_content_h > body_max_h + 1.0;
    let associated = strip_associate(ctx);
    let response = area.show(ctx, |ui| {
        ui.set_opacity(open_anim);
        let mut frame = popover_frame(tokens, th);
        if associated {
            frame = frame.stroke(Stroke::new(
                tokens.palette.associate_stroke.max(1.0),
                th.border_color(),
            ));
        }
        frame.show(ui, |ui| {
            ui.set_width((width - tokens.popover_padding * 2.0).max(1.0));
            let cap = panel_caption(
                ui,
                label,
                pinned,
                toggle_label,
                th,
                &tokens.palette,
                show_toggle,
            );
            minimize |= cap.minimize;
            toggle_layout |= cap.toggle_layout;
            advanced |= cap.advanced;
            drop_to_canvas |= cap.drop_to_canvas;
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
        advanced,
        drop_to_canvas,
        rect: response.response.rect,
        content_h,
    }
}

fn show_advanced_overlay(
    ctx: &egui::Context,
    id: egui::Id,
    tokens: &DockTokens,
    _canvas: Rect,
    _palette_label: &str,
    open_anim: f32,
    add_body: impl FnOnce(&mut egui::Ui),
) -> PanelRender {
    let adv = &tokens.advanced;
    let dark = ctx.style().visuals.dark_mode;
    let ath = adv.theme(dark);
    let mut minimize = false;
    let screen = ctx.screen_rect();

    // Middle — below the dock strip — so the Advanced dot can still close this.
    let area = egui::Area::new(id)
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .default_size(screen.size())
        .constrain(false);

    let mut content_h = 0.0;
    let response = area.show(ctx, |ui| {
        ui.set_opacity(open_anim);
        let (body, _) = ui.allocate_exact_size(screen.size(), Sense::hover());
        ui.painter().rect_filled(body, 0.0, ath.canvas_color());
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body), |ui| {
            ui.set_min_size(body.size());
            add_body(ui);
            content_h = ui.min_rect().height();
        });
    });

    if crate::dock_advanced::close_requested(ctx)
        || (ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && !crate::dock_advanced::escape_consumed(ctx))
    {
        minimize = true;
    }

    PanelRender {
        minimize,
        toggle_layout: false,
        advanced: false,
        drop_to_canvas: false,
        rect: response.response.rect,
        content_h,
    }
}

/// Panel title row: label left, optional "pinned", horizontal dot ellipsis
/// top-right. The right cluster is reserved so "pinned" cannot sit on the
/// dots. Stacked view carries the same actions as the strip column, Drop to
/// canvas included — the presentation changes, the palette's powers do not.
#[allow(clippy::too_many_arguments)]
fn panel_caption(
    ui: &mut egui::Ui,
    label: &str,
    pinned: bool,
    toggle_label: &str,
    th: &DockThemeTokens,
    p: &DockPaletteTokens,
    show_toggle: bool,
) -> StripDotClicks {
    let mut clicks = StripDotClicks::default();
    let pinned_px = (p.caption_text_size * 0.82).clamp(8.0, 16.0);
    let pinned_w = if pinned {
        ui.fonts(|f| {
            f.layout_no_wrap(
                "pinned".to_owned(),
                FontId::proportional(pinned_px),
                th.muted_text_color(),
            )
        })
        .size()
        .x
    } else {
        0.0
    };
    let gap = 12.0;
    let dots_w = dot_span(p, show_toggle) + p.dot_radius * 2.0;
    let row_h = p.dot_hit_y.max(p.caption_height);
    let right_w = dots_w + if pinned { pinned_w + gap } else { 0.0 };
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), row_h),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let title_w = (ui.available_width() - right_w - 8.0).max(24.0);
            ui.allocate_ui_with_layout(
                Vec2::new(title_w, row_h),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(label)
                                .size(p.caption_text_size)
                                .strong()
                                .color(th.text_color()),
                        )
                        .truncate()
                        .sense(Sense::hover()),
                    );
                },
            );
            ui.add_space((ui.available_width() - right_w).max(0.0));
            if pinned {
                ui.add(
                    egui::Label::new(
                        RichText::new("pinned")
                            .size(pinned_px)
                            .color(th.muted_text_color()),
                    )
                    .sense(Sense::hover()),
                );
                ui.add_space(gap);
            }
            let (rect, _) = ui.allocate_exact_size(Vec2::new(dots_w, row_h), Sense::hover());
            clicks = paint_dots(
                ui,
                rect.center(),
                th,
                p,
                pinned,
                toggle_label,
                DotAxis::Horizontal,
                show_toggle,
            );
        },
    );
    clicks
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
    fn contain_scale_is_uniform_and_does_not_shrink_when_one_axis_grows() {
        let src = Vec2::new(120.0, 36.0);
        assert!((contain_scale(Vec2::new(240.0, 36.0), src) - 1.0).abs() < 1e-5);
        assert!((contain_scale(Vec2::new(120.0, 72.0), src) - 1.0).abs() < 1e-5);
        assert!((contain_scale(Vec2::new(240.0, 72.0), src) - 2.0).abs() < 1e-5);
        assert!(contain_scale(Vec2::new(60.0, 18.0), src) < 1.0);
    }

    #[test]
    fn icon_strip_card_grows_only_under_contain() {
        let layout = IconStripLayout {
            size: Vec2::new(100.0, 20.0),
            groups: Vec::new(),
        };
        let mut tokens = DockTokens::default();
        tokens.normalize();
        let card = icon_strip_card_size(&layout, &tokens);
        let dest1 = Rect::from_min_size(Pos2::ZERO, card);
        let dest_wide = Rect::from_min_size(Pos2::ZERO, Vec2::new(card.x * 2.0, card.y));
        let dest_both = Rect::from_min_size(Pos2::ZERO, card * 2.0);
        let r1 = icon_strip_card_radius(dest1, &layout, &tokens);
        let r_wide = icon_strip_card_radius(dest_wide, &layout, &tokens);
        let r_both = icon_strip_card_radius(dest_both, &layout, &tokens);
        assert!(
            (r_wide - r1).abs() < 1e-4,
            "wider dest must not change fillet ({r_wide} vs {r1})"
        );
        assert!(
            (r_both / r1 - 2.0).abs() < 1e-4,
            "uniform grow must double fillet ({r_both} vs {r1})"
        );
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
    fn associated_palette_is_only_a_whisper_denser() {
        let p = DockPaletteTokens::default();
        assert!(
            p.associate_fill > 0.5 && p.associate_fill < 0.8,
            "hover fill should read clearly over idle 0.42, got {}",
            p.associate_fill
        );
        assert!(
            p.associate_stroke > 1.15 && p.associate_stroke <= 1.5,
            "hover stroke should thicken, got {}",
            p.associate_stroke
        );
        assert!(p.associate_tint > 0.15);
        assert!(
            p.pinned_stroke > 1.2 && p.pinned_stroke > 1.0,
            "pinned outline must read thicker than idle 1 px, got {}",
            p.pinned_stroke
        );
        assert!(p.pinned_tint > 0.1);
    }

    #[test]
    fn pinned_icon_outline_is_denser_than_idle() {
        let p = DockPaletteTokens::default();
        let idle = icon_outline(false, false, &p);
        let pinned = icon_outline(false, true, &p);
        let hover = icon_outline(true, false, &p);
        let both = icon_outline(true, true, &p);
        assert!(pinned.0 > idle.0 && pinned.1 > idle.1);
        assert!(hover.0 > idle.0);
        assert!(both.0 >= pinned.0 && both.0 >= hover.0);
        assert!(both.1 >= pinned.1 && both.1 >= hover.1);
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
    fn row_pack_fills_the_bottom_row_first() {
        // size 20, gap 4, pitch 24. Budget 100 → 4 on the primary row.
        let (offs, size) = row_pack_offsets(5, 20.0, 4.0, 100.0, DockSide::BottomCenter);
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
            top.x.abs() < 0.05,
            "overflow row stays left-aligned, got x={}",
            top.x
        );
        assert!(size.y > 20.0, "two rows must grow the cluster height");
    }

    #[test]
    fn row_pack_left_dock_fills_the_near_column_first() {
        let (offs, _) = row_pack_offsets(5, 20.0, 4.0, 100.0, DockSide::LeftCenter);
        let left_x = offs.iter().map(|o| o.x).fold(f32::INFINITY, f32::min);
        let primary = offs.iter().filter(|o| (o.x - left_x).abs() < 0.05).count();
        assert_eq!(primary, 4);
        let overflow = offs.iter().find(|o| (o.x - left_x).abs() > 0.05).unwrap();
        assert!(overflow.x > left_x, "extra column goes to the right");
    }

    #[test]
    fn accordion_peels_one_column_from_the_widest_group() {
        // shapes=2, curves=5, ink=2. One column of curves is 24px.
        let start = [44.0, 116.0, 44.0];
        let out = accordion_peel_inners(&[2, 5, 2], &start, 20.0, 4.0, 8.0, 10.0, 250.0);
        assert!(
            (out[1] - 92.0).abs() < 0.1,
            "curves should drop to 4-wide, got {}",
            out[1]
        );
        assert!((out[0] - 44.0).abs() < 0.1);
        assert!((out[2] - 44.0).abs() < 0.1);
    }

    #[test]
    fn accordion_keeps_rows_sideways_instead_of_towers() {
        // Equal-share of 200px would force 1-icon columns. Peel should stop
        // at 2-wide (44) once 2+2+2 frames fit: 60*3 + 10*2 = 200.
        let start = [44.0, 116.0, 44.0];
        let out = accordion_peel_inners(&[2, 5, 2], &start, 20.0, 4.0, 8.0, 10.0, 200.0);
        for (i, w) in out.iter().enumerate() {
            assert!(
                *w >= 43.0,
                "group {i} should stay at least 2-wide, got {w}"
            );
        }
    }

    #[test]
    fn accordion_band_stacks_every_category_before_overlap() {
        let mut naturals = HashMap::new();
        naturals.insert("tool.shapes".into(), 400.0);
        naturals.insert("tool.text".into(), 200.0);
        let ids = ["tool.shapes", "tool.text"];
        let map = accordion_band_budgets(&ids, &naturals, 500.0, 10.0, 40.0, 24.0)
            .expect("band overflows");
        let shapes = map["tool.shapes"];
        let text = map["tool.text"];
        let used = shapes + text + 10.0;
        assert!(
            used <= 500.5,
            "should peel until the band fits, got {shapes} + {text}"
        );
        assert!(
            used > 500.0 - 48.0,
            "should not peel a wasted extra round, got {shapes} + {text}"
        );
        assert!(
            text < 199.0,
            "the short category must stack too, not stay one row while the other wraps, got {text}"
        );
        assert!(
            shapes > text,
            "the wide strip still stays wider, got {shapes} vs {text}"
        );
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
        let origins = layout_panel_origins(DockSide::BottomCenter, &open, &tokens, canvas, false);
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
        let origins = layout_panel_origins(DockSide::LeftCenter, &open, &tokens, canvas, false);
        let y = origins.get("filters").expect("origin").y;
        assert!(
            y >= canvas.top(),
            "panel top {y} floated above canvas top {}",
            canvas.top()
        );
    }

    #[test]
    fn tertiary_pair_matches_secondary_icon() {
        let p = DockPaletteTokens::default();
        let icon = 22.0;
        let pair = tertiary_slot_h(icon, &p) * 2.0 + p.tertiary_stack_gap;
        assert!(
            (pair - icon).abs() < 0.01,
            "two tertiary slots ({pair}) must share one secondary icon ({icon})"
        );
    }

    #[test]
    fn stacked_icon_row_is_taller_than_the_pill() {
        assert!(
            crate::sidebar::SidebarTokens::ICON_ROW_HEIGHT
                >= crate::sidebar::SidebarTokens::TOGGLE_TRACK_H,
            "toggle rows must fit the track"
        );
        assert!(
            crate::sidebar::SidebarTokens::TOGGLE_TRACK_W
                > crate::sidebar::SidebarTokens::TOGGLE_TRACK_H,
            "toggle is a wide capsule, not a circle"
        );
    }

    #[test]
    fn strip_dots_pack_tighter_than_an_icon() {
        let mut p = DockPaletteTokens::default();
        p.normalize();
        let col_h = dot_span(&p, true) + p.dot_radius * 2.0;
        assert!(
            col_h < 22.0,
            "four dots must sit inside one flyout icon ({col_h})"
        );
        assert!(
            p.dot_hit_x > dot_span(&p, true),
            "X hit must be far wider than the dots so the pointer need not be exact"
        );
    }

    /// Strip and stacked are two presentations of one palette, so they offer
    /// the same actions. Dropping a copy on the canvas was once strip-only,
    /// which made the layout toggle a trapdoor: switch view, lose a power.
    #[test]
    fn both_presentations_can_drop_to_canvas() {
        for show_toggle in [false, true] {
            assert_eq!(
                dot_count(show_toggle),
                if show_toggle { 4 } else { 3 },
                "Minimize, Advanced, and Drop are per-palette and never omitted"
            );
        }
    }

    /// The layout toggle switches every palette at once, so it appears once —
    /// on the last palette along the dock.
    #[test]
    fn only_the_last_palette_offers_the_layout_toggle() {
        let mut origins = HashMap::new();
        origins.insert("tool.frame", Pos2::new(100.0, 800.0));
        origins.insert("tool.shapes", Pos2::new(420.0, 800.0));
        origins.insert("tool.text", Pos2::new(260.0, 800.0));
        let open = ["tool.frame", "tool.shapes", "tool.text"];
        assert_eq!(
            toggle_palette(DockSide::BottomCenter, &open, &origins),
            Some("tool.shapes"),
            "rightmost palette owns the toggle on a bottom dock"
        );

        let mut origins = HashMap::new();
        origins.insert("tool.frame", Pos2::new(10.0, 120.0));
        origins.insert("tool.shapes", Pos2::new(10.0, 640.0));
        let open = ["tool.frame", "tool.shapes"];
        assert_eq!(
            toggle_palette(DockSide::LeftCenter, &open, &origins),
            Some("tool.shapes"),
            "bottom-most palette owns the toggle on a left dock"
        );

        assert_eq!(
            toggle_palette(DockSide::BottomCenter, &[], &HashMap::new()),
            None,
            "nothing pinned — the volatile body carries the toggle instead"
        );
    }

    #[test]
    fn hidden_tools_are_per_palette() {
        let mut hidden = HashMap::new();
        hidden.insert("tool.shapes".into(), vec!["shape.eraser".into()]);
        assert!(tool_hidden_in(&hidden, "tool.shapes", "shape.eraser"));
        assert!(!tool_hidden_in(&hidden, "tool.shapes", "shape.rect"));
        assert!(!tool_hidden_in(&hidden, "tool.frame", "shape.eraser"));
    }
}
