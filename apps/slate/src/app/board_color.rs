//! Board color state + the expressive ink tools (keymap wave 2b, cluster A):
//! Brush (B), Eraser (E), Eyedropper (I + spring-loaded Alt from Brush),
//! Sticky note (N), fg/bg color state (D reset / X swap), and the shared
//! `[`/`]` width stepping.
//!
//! See `docs/keymap/specs/brush-color.md`. All stroke creation/removal goes
//! through the journal (Art. VI); color state is tool state (never journaled).

use super::board::{to_rgba, BoardTool, MIN_DRAW};
use super::{board_path, SlateApp};
use atlas_shell::desktop_color::PickMode;
use eframe::egui::{self, Color32, Pos2, Stroke as EStroke};
use slate_doc::scene::{
    Node, NodeKind, Rgba, ShapeKind, ShapeNode, Stroke, StrokeCap, StrokeJoin, StrokeSpan,
    TextAlign, TextNode, Typeface, WidthProfile, WorldRect,
};
use slate_doc::NodeId;
use vector_ink::kurbo::BezPath;

/// Whether a round pick of radius `slop` touches `node` anywhere on the
/// segment `from` to `to`. One fast pointer move is one long segment, so the
/// pick is tested at samples no more than half its radius apart, not only at
/// the two ends.
pub(crate) fn eraser_sweep_hits_node(
    node: &Node,
    from: Pos2,
    to: Pos2,
    slop: f32,
    zoom: f32,
) -> bool {
    if node.hidden || node.locked {
        return false;
    }
    let NodeKind::Shape(shape) = &node.kind else {
        return false;
    };
    let pad = slop + shape.stroke.width.max(1.0) * 0.5;
    let rect = node.rect.normalized();
    let reach = if node.rotation_deg.abs() > 0.01 {
        pad + rect.w.max(rect.h)
    } else {
        pad
    };
    let near = |p: &Pos2| {
        p.x >= rect.x - reach
            && p.x <= rect.x + rect.w + reach
            && p.y >= rect.y - reach
            && p.y <= rect.y + rect.h + reach
    };
    let steps = ((to - from).length() / (slop * 0.5).max(0.25)).ceil() as usize;
    let mut samples = (0..=steps)
        .map(|i| from.lerp(to, i as f32 / steps.max(1) as f32))
        .filter(near)
        .peekable();
    if samples.peek().is_none() {
        return false;
    }
    match shape.shape {
        ShapeKind::Path => {
            let Some(path) = shape.path.as_ref() else {
                return false;
            };
            if path.is_empty() || shape.stroke.paints_as_stamp() {
                return false;
            }
            let bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
            let style = board_path::stroke_style_world(&shape.stroke, zoom);
            samples.any(|p| vector_ink::hit_stroke(&bez, &style, [p.x, p.y], slop))
        }
        ShapeKind::Line => {
            let (a, b) = line_endpoints(node.rect, shape.flip, node.rotation_deg);
            samples.any(|p| dist_point_segment(p, a, b) <= pad)
        }
        _ => false,
    }
}

pub(crate) fn spot_reaches(node: &Node, bounds: WorldRect) -> bool {
    if node.hidden || node.locked {
        return false;
    }
    let NodeKind::Shape(shape) = &node.kind else {
        return false;
    };
    if shape.shape != ShapeKind::Path || shape.path.is_none() || !shape.stroke.paints_as_stamp() {
        return false;
    }
    let ink = shape.stroke.width * 0.5
        + 3.0 * shape.stroke.gaussian_blur.max(0.0)
        + shape
            .path
            .as_ref()
            .map(|path| {
                path.tips
                    .iter()
                    .map(|tip| tip.width * 0.5)
                    .fold(0.0, f32::max)
            })
            .unwrap_or(0.0);
    let rect = node.rect.normalized();
    let reach = if node.rotation_deg.abs() > 0.01 {
        ink + rect.w.max(rect.h)
    } else {
        ink
    };
    rect.x - reach <= bounds.x + bounds.w
        && rect.x + rect.w + reach >= bounds.x
        && rect.y - reach <= bounds.y + bounds.h
        && rect.y + rect.h + reach >= bounds.y
}

/// Whether an eraser disc of radius `r` swept from `from` to `to` touches the
/// painted ink of a stamped stroke: its tipped centerline, each vertex at its
/// own tip radius. Only strokes the pass really reaches get a live preview.
pub(crate) fn spot_touches(node: &Node, from: Pos2, to: Pos2, r: f32) -> bool {
    let NodeKind::Shape(shape) = &node.kind else {
        return false;
    };
    let Some(path) = shape.path.as_ref() else {
        return false;
    };
    let tolerance = (r as f64 * 0.25).max(0.05);
    let r = r + 3.0 * shape.stroke.gaussian_blur.max(0.0);
    let pos = |p: &vector_ink::TipPoint| Pos2::new(p.pos[0], p.pos[1]);
    board_path::stamped_contours(node, shape, path, tolerance)
        .iter()
        .any(|contour| match contour.as_slice() {
            [only] => dist_point_segment(pos(only), from, to) <= r + only.tip.diameter * 0.5,
            pts => pts.windows(2).any(|s| {
                let reach = r + s[0].tip.diameter.max(s[1].tip.diameter) * 0.5;
                dist_segment_segment(from, to, pos(&s[0]), pos(&s[1])) <= reach
            }),
        })
}

fn dist_segment_segment(a0: Pos2, a1: Pos2, b0: Pos2, b1: Pos2) -> f32 {
    let cross = |o: Pos2, p: Pos2, q: Pos2| (p - o).x * (q - o).y - (p - o).y * (q - o).x;
    let (d1, d2) = (cross(b0, b1, a0), cross(b0, b1, a1));
    let (d3, d4) = (cross(a0, a1, b0), cross(a0, a1, b1));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    dist_point_segment(a0, b0, b1)
        .min(dist_point_segment(a1, b0, b1))
        .min(dist_point_segment(b0, a0, a1))
        .min(dist_point_segment(b1, a0, a1))
}

pub(crate) fn stamp_erase_mark(
    before: &Node,
    points: &[Pos2],
    span: StrokeSpan,
) -> Option<(Node, bool)> {
    let after = with_erase_mark(before, points, span);
    let (ink, gone) = erased_result(&after);
    ink.then_some((after, gone))
}

/// `before` with one more erase pass along world `points`.
pub(crate) fn with_erase_mark(before: &Node, points: &[Pos2], span: StrokeSpan) -> Node {
    let mut after = before.clone();
    let (rect, rotation) = (after.rect, after.rotation_deg);
    if let NodeKind::Shape(shape) = &mut after.kind {
        if let Some(path) = shape.path.as_mut() {
            std::sync::Arc::make_mut(path)
                .erase
                .push(slate_doc::scene::EraseMark {
                    points: points
                        .iter()
                        .map(|point| board_path::world_to_node_norm(*point, rect, rotation))
                        .collect(),
                    tips: vec![span],
                });
        }
    }
    after
}

pub(crate) enum DesktopDestination {
    Tool {
        background: bool,
        alt_selects_background: bool,
    },
    Nodes {
        ids: Vec<NodeId>,
        panel: super::board_properties::Panel,
        preview: bool,
    },
}
pub struct DesktopSample {
    picker: atlas_shell::desktop_color::DesktopColorPicker,
    tab: u64,
    destination: DesktopDestination,
}

/// Color transitions at journal boundaries; geometry/alpha-only edits contribute nothing.
pub(super) fn committed_colors<'a>(
    nodes: impl IntoIterator<
        Item = (
            Option<&'a slate_doc::scene::Node>,
            &'a slate_doc::scene::Node,
        ),
    >,
) -> Vec<[u8; 3]> {
    fn colors(n: &slate_doc::scene::Node) -> [Option<[u8; 3]>; 2] {
        use slate_doc::scene::{fill_of, stroke_of};
        [fill_of(n), stroke_of(n).map(|s| s.color)].map(|c| c.map(|c| [c.0[0], c.0[1], c.0[2]]))
    }
    nodes
        .into_iter()
        .flat_map(|(before, after)| {
            let before = before.map(colors).unwrap_or([None, None]);
            colors(after)
                .into_iter()
                .zip(before)
                .filter_map(|(a, b)| if a != b { a } else { None })
        })
        .collect()
}

impl SlateApp {
    pub(crate) fn seed_document_colors(&mut self) {
        if self.doc().view.recent_colors.is_none() {
            let colors = committed_colors(self.doc().scene.nodes.iter().map(|n| (None, n)));
            self.tab_mut().doc.view.seed_recent_colors(colors);
        }
    }

    pub(crate) fn remember_document_colors(&mut self, colors: Vec<[u8; 3]>) {
        self.seed_document_colors();
        let before = self.doc().view.recent_colors.clone();
        for color in colors {
            self.tab_mut().doc.view.remember_color(color);
        }
        if self.doc().view.recent_colors != before {
            self.tab_mut().dirty = true;
        }
    }

    pub(crate) fn begin_desktop_sample(&mut self, destination: DesktopDestination, mode: PickMode) {
        if self.desktop_sample.is_some() {
            return;
        }
        // Headless input tests never create native windows or consume desktop clicks.
        #[cfg(test)]
        {
            self.desktop_sample_requests.push(mode);
            let _ = destination;
        }
        #[cfg(not(test))]
        {
            self.desktop_sample = Some(DesktopSample {
                picker: atlas_shell::desktop_color::DesktopColorPicker::begin(mode),
                tab: self.tab().id,
                destination,
            });
        }
    }
    pub(crate) fn start_tool_desktop_sample(&mut self, background: bool, temporary: bool) {
        self.begin_desktop_sample(
            DesktopDestination::Tool {
                background,
                alt_selects_background: self.board_tool == BoardTool::Eyedropper,
            },
            if temporary {
                PickMode::AltHeld
            } else {
                PickMode::Click
            },
        );
    }
    pub(crate) fn start_node_desktop_sample(
        &mut self,
        ids: &[NodeId],
        panel: super::board_properties::Panel,
    ) {
        self.begin_desktop_sample(
            DesktopDestination::Nodes {
                ids: ids.to_vec(),
                panel,
                preview: false,
            },
            PickMode::Click,
        );
    }
    pub(crate) fn desktop_sample_frame(&mut self, ctx: &egui::Context) {
        if self.desktop_sample.is_none() {
            if !self.alt_down {
                self.desktop_alt_latched = false;
            }
            let secondary = ctx.input(|i| {
                i.pointer.button_down(egui::PointerButton::Secondary)
                    || i.pointer.button_pressed(egui::PointerButton::Secondary)
            });
            let over_window = ctx.input(|i| i.pointer.hover_pos().is_some());
            // Alt+right-drag is the size / color HUD, so the fullscreen sampler
            // stays down while the pointer is over this window. It still opens
            // once Alt is held and the pointer leaves, which is how a sample
            // reaches another application.
            if self.board_tool == BoardTool::Brush
                && self.alt_down
                && !self.desktop_alt_latched
                && !over_window
                && self.brush_hud.is_none()
                && !secondary
            {
                self.desktop_alt_latched = true;
                self.start_tool_desktop_sample(false, true);
            }
        }
        let Some(session) = &self.desktop_sample else {
            return;
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
        let Some(result) = session.picker.poll() else {
            return;
        };
        let session = self.desktop_sample.take().unwrap();
        let sample = match result {
            Ok(Some(rgb)) => rgb,
            Ok(None) => return,
            Err(error) => {
                self.toast(&error);
                return;
            }
        };
        let rgb = sample.rgb;
        if self.tab().id != session.tab {
            self.toast("Color pick canceled: the requesting workbook changed");
            return;
        }
        use super::board_properties::{Panel, Property, PropertyRequest};
        match session.destination {
            DesktopDestination::Tool {
                background,
                alt_selects_background,
            } => {
                let background = background || (alt_selects_background && sample.alt);
                let color = if background {
                    &mut self.board_colors.bg
                } else {
                    &mut self.board_colors.fg
                };
                color.0[..3].copy_from_slice(&rgb);
                self.save_board_colors();
                self.remember_document_colors(vec![rgb]);
            }
            DesktopDestination::Nodes {
                ids,
                panel,
                preview,
            } => {
                if self.tab().read_only
                    || ids
                        .iter()
                        .any(|id| self.doc().scene.node(*id).is_none_or(|n| n.locked))
                {
                    self.toast("Color pick canceled: its target is no longer editable");
                    return;
                }
                let edit = if panel == Panel::Fill {
                    Property::FillRgb(rgb)
                } else if panel == Panel::Text {
                    Property::TextRgb(rgb)
                } else {
                    Property::StrokeRgb(rgb)
                };
                if preview
                    && self.shape_properties.panel == Some(panel)
                    && self.shape_properties.tab == session.tab
                    && self.shape_properties.ids == ids
                {
                    self.preview_shape_property(edit);
                } else if preview {
                    self.toast("Color pick canceled: its selection changed");
                } else {
                    let request = PropertyRequest {
                        ids,
                        edits: vec![edit],
                        points: Vec::new(),
                    };
                    self.dispatch(
                        ctx,
                        atlas_commands::CommandId("board.shape.edit"),
                        serde_json::to_string(&request).ok(),
                    );
                }
            }
        }
    }
}

/// Sticky note preset: fixed size, white fill, dark ink, default text size
/// (no autosize in P1 — overflow clips, matching the artifact's
/// `overflow:hidden`). A subtle drop shadow is painted, not stored.
pub const STICKY_SIZE: f32 = 200.0;
pub const STICKY_GAP: f32 = 24.0;
pub use slate_doc::scene::{STICKY_FILL, STICKY_INK};

/// A short primary press while Alt or Shift is held. Travel past this
/// many screen pixels is a drag: Alt does not sample, and a Shift segment
/// takes the direction constraints instead of connecting to the click.
pub(crate) const BRUSH_MOD_CLICK_PX: f32 = 8.0;

/// An Alt primary press waiting for a short click (eyedropper sample).
#[derive(Clone, Copy)]
pub(crate) struct BrushModClick {
    pub origin: Pos2,
}

/// The shared foreground/background color pair consumed by Brush strokes,
/// wires, and the sticky/eyedropper flow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoardColors {
    pub fg: Rgba,
    pub bg: Rgba,
}

impl BoardColors {
    /// Theme-aware defaults: fg = ink, bg = canvas paper.
    pub fn theme_default(dark_mode: bool) -> BoardColors {
        let palette = atlas_shell::theme::Palette::for_mode(dark_mode);
        BoardColors {
            fg: to_rgba(palette.ink),
            bg: to_rgba(palette.bg),
        }
    }

    /// Startup resolution: persisted values win, else theme defaults.
    pub fn from_settings(settings: &super::settings::SlateSettings, dark_mode: bool) -> Self {
        let d = Self::theme_default(dark_mode);
        BoardColors {
            fg: settings.board_fg.map(Rgba).unwrap_or(d.fg),
            bg: settings.board_bg.map(Rgba).unwrap_or(d.bg),
        }
    }
}

/// Photoshop's `[`/`]` width stepping table, in **screen pixels**:
/// `<10 px → ±1 · 10–50 → ±5 · 50–100 → ±10 · >100 → ±25`. Stepping down at
/// a tier boundary uses the lower tier so up/down are inverses
/// (10 → 9, 50 → 45). Result clamps at 1 px.
pub fn step_width_px(px: f32, up: bool) -> f32 {
    fn tier(p: f32) -> f32 {
        if p < 10.0 {
            1.0
        } else if p < 50.0 {
            5.0
        } else if p < 100.0 {
            10.0
        } else {
            25.0
        }
    }
    let px = px.max(0.0);
    if up {
        px + tier(px)
    } else {
        (px - tier((px - 0.01).max(0.0))).max(1.0)
    }
}

pub(crate) const SOFTNESS_STEP: f32 = 0.25;
pub(crate) const SOFTNESS_DRAG_PX: f32 = 100.0;
/// Screen px of diameter per screen px of horizontal Alt+right-drag travel.
pub(crate) const SIZE_DRAG_GAIN: f32 = 2.0;
/// Vertical Shift+right-drag travel across the full opacity range.
pub(crate) const OPACITY_DRAG_PX: f32 = 100.0;
pub(crate) const WHEEL_SLOT_RADIUS: f32 = 130.0;
pub(crate) const WHEEL_SV_RADIUS: f32 = 84.0;
/// Dead band between the saturation/value disk and the hue ring. Neither
/// control picks in it.
pub(crate) const WHEEL_RING_GAP: f32 = 8.0;
pub(crate) const WHEEL_HUE_INNER: f32 = WHEEL_SV_RADIUS + WHEEL_RING_GAP;
pub(crate) const WHEEL_HUE_OUTER: f32 = WHEEL_HUE_INNER + 20.0;
/// Radius of the snap regions at the disk's pure white and pure black.
pub(crate) const WHEEL_SNAP_RADIUS: f32 = 8.0;
/// Painted swatch radius. Swatches sit clear of the hue ring: ring edge
/// 112, swatch inner edge 122, so neither hit zone reaches the other.
pub(crate) const WHEEL_DOT_RADIUS: f32 = 8.0;
/// The wheel's dark backdrop. Inside it the wheel keeps the last value;
/// past it the hold samples the canvas.
pub(crate) const WHEEL_BACKDROP_RADIUS: f32 = WHEEL_SLOT_RADIUS + 16.0;
/// A little past the painted swatch, and short of the hue ring.
pub(crate) const WHEEL_DOT_HIT: f32 = 10.0;
/// The hue ring picks this far past its painted outer edge.
pub(crate) const WHEEL_HUE_HIT_PAD: f32 = 3.0;

/// Quarter-step softness. 0 is a hard edge, 1 is the softest.
pub fn step_softness(current: f32, up: bool) -> f32 {
    let next = if up {
        current + SOFTNESS_STEP
    } else {
        current - SOFTNESS_STEP
    };
    next.clamp(0.0, 1.0)
}

/// Horizontal HUD travel adds [`SIZE_DRAG_GAIN`] screen px of diameter per px.
pub fn scrub_diameter_px(start_px: f32, dx: f32) -> f32 {
    (start_px + dx * SIZE_DRAG_GAIN).max(1.0)
}

/// Vertical HUD travel. Screen +y is down, so dragging up increases softness.
pub fn scrub_softness(start: f32, dy_screen: f32) -> f32 {
    (start - dy_screen / SOFTNESS_DRAG_PX).clamp(0.0, 1.0)
}

/// Vertical Shift+right-drag. Screen +y is down, so dragging up increases
/// opacity, down to 0%.
pub fn scrub_opacity(start: f32, dy_screen: f32) -> f32 {
    (start - dy_screen / OPACITY_DRAG_PX).clamp(0.0, 1.0)
}

/// Slot 0 sits at 6 o'clock. Later slots step clockwise. +y is down.
pub fn wheel_slot_offset(slot: usize, slots: usize, radius: f32) -> [f32; 2] {
    let theta = slot as f32 / slots.max(1) as f32 * std::f32::consts::TAU;
    [-theta.sin() * radius, theta.cos() * radius]
}

/// Screen center of recent-color swatch `slot` on a wheel centered at
/// `center`. Painting, picking, and the pointer warp all read this.
pub fn wheel_slot_center(center: Pos2, slot: usize) -> Pos2 {
    let [x, y] = wheel_slot_offset(
        slot,
        slate_doc::ViewState::WHEEL_COLOR_LIMIT,
        WHEEL_SLOT_RADIUS,
    );
    center + egui::vec2(x, y)
}

pub fn rgb_to_hsv(rgb: [u8; 3]) -> [f32; 3] {
    let r = rgb[0] as f32 / 255.0;
    let g = rgb[1] as f32 / 255.0;
    let b = rgb[2] as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    let s = if max == 0.0 { 0.0 } else { d / max };
    [h, s, max]
}

pub fn hsv_to_rgb(hsv: [f32; 3]) -> [u8; 3] {
    let h = hsv[0].rem_euclid(1.0) * 6.0;
    let s = hsv[1].clamp(0.0, 1.0);
    let v = hsv[2].clamp(0.0, 1.0);
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ]
}

/// Screen position of a color on the wheel's saturation/value disc.
/// The wheel stays put; a swatch pick moves the pointer here.
#[allow(dead_code)]
pub fn wheel_sv_cursor(center: Pos2, hsv: [f32; 3]) -> Pos2 {
    center + sv_offset(hsv[1], hsv[2])
}

/// The saturation/value square stretched over the whole disk (elliptical
/// grid mapping), so every corner of the square is on the disk: pure white
/// at its upper-left rim, the pure hue at its upper-right rim, and black
/// along its lower rim.
fn sv_offset(sat: f32, val: f32) -> egui::Vec2 {
    let x = sat.clamp(0.0, 1.0) * 2.0 - 1.0;
    let y = 1.0 - val.clamp(0.0, 1.0) * 2.0;
    egui::vec2(
        x * (1.0 - y * y * 0.5).sqrt(),
        y * (1.0 - x * x * 0.5).sqrt(),
    ) * WHEEL_SV_RADIUS
}

/// Inverse of [`sv_offset`] for a point on or inside the disk.
fn sv_at(local: egui::Vec2) -> (f32, f32) {
    let mut d = local / WHEEL_SV_RADIUS;
    if d.length_sq() > 1.0 {
        d = d.normalized();
    }
    let (u2, v2) = (d.x * d.x, d.y * d.y);
    let k = 2.0 * std::f32::consts::SQRT_2;
    let x = 0.5 * (2.0 + u2 - v2 + k * d.x).max(0.0).sqrt()
        - 0.5 * (2.0 + u2 - v2 - k * d.x).max(0.0).sqrt();
    let y = 0.5 * (2.0 - u2 + v2 + k * d.y).max(0.0).sqrt()
        - 0.5 * (2.0 - u2 + v2 - k * d.y).max(0.0).sqrt();
    (
        ((x + 1.0) * 0.5).clamp(0.0, 1.0),
        ((1.0 - y) * 0.5).clamp(0.0, 1.0),
    )
}

/// Wheel-local centers of the pure white and pure black snap regions, and
/// the saturation/value each returns.
pub(crate) fn wheel_snaps() -> [(egui::Vec2, [f32; 2]); 2] {
    [
        (sv_offset(0.0, 1.0), [0.0, 1.0]),
        (sv_offset(0.5, 0.0), [0.5, 0.0]),
    ]
}

/// What the pointer is over inside the color wheel, in wheel-local pixels.
#[derive(Debug)]
pub enum WheelHit {
    /// On the wheel between controls, including the gap around the disk:
    /// keep the current value.
    Keep,
    /// Past the wheel's backdrop: sample the canvas.
    Outside,
    Field([u8; 3], [f32; 3]),
    /// Recent swatch: its color and slot.
    Dot([u8; 3], usize),
}

pub fn sample_wheel(local: [f32; 2], hsv: [f32; 3], recents: &[[u8; 3]]) -> WheelHit {
    let slots = slate_doc::ViewState::WHEEL_COLOR_LIMIT;
    let mut nearest: Option<(f32, [u8; 3], usize)> = None;
    for (index, color) in recents.iter().enumerate().take(slots) {
        let at = wheel_slot_center(Pos2::ZERO, index);
        let dx = local[0] - at.x;
        let dy = local[1] - at.y;
        let d2 = dx * dx + dy * dy;
        if d2 <= WHEEL_DOT_HIT * WHEEL_DOT_HIT
            && nearest.as_ref().is_none_or(|(best, _, _)| d2 < *best)
        {
            nearest = Some((d2, *color, index));
        }
    }
    if let Some((_, rgb, slot)) = nearest {
        return WheelHit::Dot(rgb, slot);
    }
    let dist = (local[0] * local[0] + local[1] * local[1]).sqrt();
    if dist <= WHEEL_SV_RADIUS {
        let p = egui::vec2(local[0], local[1]);
        let snap = wheel_snaps()
            .into_iter()
            .find(|(at, _)| (p - *at).length() <= WHEEL_SNAP_RADIUS);
        let (sat, val) = match snap {
            Some((_, [s, v])) => (s, v),
            None => sv_at(p),
        };
        let next = [hsv[0], sat, val];
        return WheelHit::Field(hsv_to_rgb(next), next);
    }
    if (WHEEL_HUE_INNER..=WHEEL_HUE_OUTER + WHEEL_HUE_HIT_PAD).contains(&dist) {
        let mut hue = (-local[1]).atan2(local[0]) / std::f32::consts::TAU;
        if hue < 0.0 {
            hue += 1.0;
        }
        let next = [hue, hsv[1], hsv[2]];
        return WheelHit::Field(hsv_to_rgb(next), next);
    }
    if dist > WHEEL_BACKDROP_RADIUS {
        return WheelHit::Outside;
    }
    WheelHit::Keep
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct BrushSettingUndo {
    pub width: f32,
    pub softness: f32,
    pub opacity: f32,
    pub fg: [u8; 4],
    pub eraser_width: f32,
    pub eraser_softness: f32,
    pub eraser_opacity: f32,
    pub brush_texture: slate_doc::scene::BrushTexture,
    pub eraser_texture: slate_doc::scene::BrushTexture,
}

#[derive(Clone, Copy)]
pub(crate) struct BrushTip {
    pub width: f32,
    pub softness: f32,
    pub color: Rgba,
    pub texture: slate_doc::scene::BrushTexture,
}

impl BrushTip {
    pub fn span(self) -> slate_doc::scene::StrokeSpan {
        slate_doc::scene::StrokeSpan {
            width: self.width,
            softness: self.softness,
            color: self.color,
            texture: self.texture,
        }
    }

    pub fn stamp(self) -> vector_ink::StampStyle {
        vector_ink::StampStyle {
            diameter: self.width.max(0.0),
            softness: self.softness,
            rgba: self.color.0,
            grain: self.texture.grain(),
        }
    }
}

/// End of the last brush mark: where the next Shift segment starts, with the
/// tip it had and the stroke it can extend.
#[derive(Clone, Copy)]
pub(crate) struct BrushAnchor {
    pub pos: Pos2,
    pub tip: BrushTip,
    pub node: Option<NodeId>,
}

pub(crate) struct BrushStraight {
    pub start: Pos2,
    pub start_screen: Pos2,
    pub tip: BrushTip,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum BrushHud {
    /// Alt+right-drag: size horizontally, softness vertically, for the
    /// armed Brush or Eraser. Pen, Line, Arc, Polyline, and Bézier take the
    /// width only.
    Size {
        /// The press point in world space, so the circle stays pinned to the
        /// canvas under it for the whole hold.
        origin: Pos2,
        width0: f32,
        softness0: f32,
    },
    Wheel {
        fg0: [u8; 4],
        center: Pos2,
        hsv: [f32; 3],
        /// Pointer left the wheel, so this hold samples the canvas instead.
        sampling: bool,
        /// The swatch the pointer last entered. Entering another one moves
        /// the pointer to its center.
        dot: Option<usize>,
    },
    /// Shift+right-drag: opacity (Brush) or strength (Eraser). The circle
    /// stays on the press point.
    Opacity { origin: Pos2, opacity0: f32 },
}

impl SlateApp {
    // ---------- color state ----------

    /// `D` — reset fg/bg to the theme defaults.
    pub(crate) fn reset_board_colors(&mut self) {
        self.board_colors = BoardColors::theme_default(self.dark_mode);
        self.save_board_colors();
    }

    /// `X` — swap fg ⇄ bg.
    pub(crate) fn swap_board_colors(&mut self) {
        std::mem::swap(&mut self.board_colors.fg, &mut self.board_colors.bg);
        self.save_board_colors();
    }

    pub(crate) fn save_board_colors(&mut self) {
        self.settings.board_fg = Some(self.board_colors.fg.0);
        self.settings.board_bg = Some(self.board_colors.bg.0);
        self.settings.save();
    }

    /// `[` / `]` — step the active width (eraser while the Eraser tool is
    /// armed, brush otherwise) through the PS tier table in screen px,
    /// converted by the current zoom. Returns (new world width, is_eraser).
    pub(crate) fn step_active_width(&mut self, up: bool) -> (f32, bool) {
        let z = self.tab().cam.z.max(f32::EPSILON);
        let eraser = matches!(self.board_tool, BoardTool::Eraser | BoardTool::Smooth);
        let before = self.brush_setting_snapshot();
        let node_before = self.hud_node_snapshot();
        let (w, soft, opacity) = self.active_tip();
        let px = step_width_px(w * z, up);
        let new_w = (px / z).clamp(
            super::settings::STROKE_WIDTH_MIN,
            super::settings::STROKE_WIDTH_MAX,
        );
        self.set_active_tip(new_w, soft, opacity);
        self.settings.save();
        self.push_brush_setting_undo(before);
        self.journal_hud_node(node_before);
        (new_w, eraser)
    }

    /// Width, softness, and opacity (erase strength) of the armed tip.
    pub(crate) fn active_tip(&self) -> (f32, f32, f32) {
        if let Some(tip) = self.vector_tip() {
            return tip;
        }
        match self.board_tool {
            BoardTool::Eraser => (self.eraser_width, self.eraser_softness, self.eraser_opacity),
            BoardTool::Smooth => (
                self.smooth_width,
                self.smooth_softness,
                self.smooth_strength,
            ),
            _ => (self.brush_width, self.brush_softness, self.brush_opacity),
        }
    }

    /// True when Alt+right-drag sizes the armed tool: the brush, eraser,
    /// and smooth tips, and the pen, line, arc, polyline, and Bézier widths.
    pub(crate) fn board_tool_takes_width_chord(&self) -> bool {
        matches!(
            self.board_tool,
            BoardTool::Brush | BoardTool::Eraser | BoardTool::Smooth
        ) || self.armed_stroke_tool().is_some()
    }

    /// The tip the size chord scrubs. A stroke tool's own width, always
    /// hard; otherwise [`Self::active_tip`].
    fn chord_tip(&self) -> (f32, f32, f32) {
        match self.armed_stroke_tool() {
            Some(tool) => (
                self.stroke_for_tool(tool).width,
                0.0,
                self.opacity_for_tool(tool),
            ),
            None => self.active_tip(),
        }
    }

    /// A stroke tool takes only the width: it offers no softness control.
    fn set_chord_tip(&mut self, width: f32, softness: f32, opacity: f32) {
        match self.armed_stroke_tool() {
            Some(tool) => self.set_tool_width(tool, width),
            None => self.set_active_tip(width, softness, opacity),
        }
    }

    /// Size HUD label: the brush readout, or a stroke tool's width.
    pub(crate) fn size_hud_label(&self) -> Option<String> {
        if let Some(line) = self.brush_status_line() {
            return Some(line);
        }
        if self.doc().view.active_view != slate_doc::ViewKind::Board {
            return None;
        }
        let z = self.tab().cam.z.max(f32::EPSILON);
        self.armed_stroke_tool()
            .map(|tool| format!("{:.0} px", self.stroke_for_tool(tool).width * z))
    }

    /// Write the armed tip into app state and settings (not saved).
    pub(crate) fn set_active_tip(&mut self, width: f32, softness: f32, opacity: f32) {
        if self.set_vector_tip(width, softness, opacity) {
            return;
        }
        if self.board_tool == BoardTool::Eraser {
            self.eraser_width = width;
            self.eraser_softness = softness;
            self.eraser_opacity = opacity;
            self.settings.eraser_width = width;
            self.settings.eraser_softness = softness;
            self.settings.eraser_opacity = opacity;
        } else if self.board_tool == BoardTool::Smooth {
            self.smooth_width = width;
            self.smooth_softness = softness;
            self.smooth_strength = opacity.clamp(0.0, 1.0);
        } else {
            self.brush_width = width;
            self.brush_softness = softness;
            self.brush_opacity = opacity;
            self.settings.brush_width = width;
            self.settings.brush_softness = softness;
            self.settings.brush_opacity = opacity;
        }
    }

    /// `Shift+[` / `Shift+]` — softness of the armed tip in quarter steps.
    pub(crate) fn step_brush_softness(&mut self, up: bool) -> f32 {
        let before = self.brush_setting_snapshot();
        let node_before = self.hud_node_snapshot();
        let (w, soft, opacity) = self.active_tip();
        let soft = step_softness(soft, up);
        self.set_active_tip(w, soft, opacity);
        self.settings.save();
        self.push_brush_setting_undo(before);
        self.journal_hud_node(node_before);
        soft
    }

    pub(crate) fn brush_setting_snapshot(&self) -> BrushSettingUndo {
        BrushSettingUndo {
            width: self.brush_width,
            softness: self.brush_softness,
            opacity: self.brush_opacity,
            fg: self.board_colors.fg.0,
            eraser_width: self.eraser_width,
            eraser_softness: self.eraser_softness,
            eraser_opacity: self.eraser_opacity,
            brush_texture: self.brush_texture,
            eraser_texture: self.eraser_texture,
        }
    }

    pub(crate) fn push_brush_setting_undo(&mut self, before: BrushSettingUndo) {
        let now = self.brush_setting_snapshot();
        if before == now {
            return;
        }
        self.brush_setting_undo.push(before);
        if self.brush_setting_undo.len() > 16 {
            self.brush_setting_undo.remove(0);
        }
    }

    /// Ctrl+Z restores the last size, softness, color, or opacity edit of the
    /// Brush or Eraser when no scene action has happened since.
    pub(crate) fn undo_brush_setting(&mut self) -> bool {
        let Some(before) = self.brush_setting_undo.pop() else {
            return false;
        };
        self.brush_width = before.width;
        self.brush_softness = before.softness;
        self.brush_opacity = before.opacity;
        self.board_colors.fg.0 = before.fg;
        self.eraser_width = before.eraser_width;
        self.eraser_softness = before.eraser_softness;
        self.eraser_opacity = before.eraser_opacity;
        self.brush_texture = before.brush_texture;
        self.eraser_texture = before.eraser_texture;
        self.settings.brush_texture = before.brush_texture;
        self.settings.eraser_texture = before.eraser_texture;
        self.settings.brush_width = before.width;
        self.settings.brush_softness = before.softness;
        self.settings.brush_opacity = before.opacity;
        self.settings.eraser_width = before.eraser_width;
        self.settings.eraser_softness = before.eraser_softness;
        self.settings.eraser_opacity = before.eraser_opacity;
        self.settings.save();
        self.save_board_colors();
        true
    }

    // ---------- brush (B) ----------

    pub(crate) fn brush_stroke(&self) -> Stroke {
        Stroke {
            width: self.brush_width,
            color: self.brush_rgba(),
            dash: slate_doc::scene::Dash::Solid,
            cap: StrokeCap::Round,
            join: StrokeJoin::Round,
            profile: WidthProfile::Uniform,
            softness: self.brush_softness,
            stamp: true,
            tween_from: None,
            gaussian_blur: 0.0,
            arrow_end: false,
            texture: self.brush_texture,
        }
    }

    fn brush_rgba(&self) -> Rgba {
        let c = self.board_colors.fg.0;
        let a = (c[3] as f32 * self.brush_opacity.clamp(0.0, 1.0)).round() as u8;
        Rgba([c[0], c[1], c[2], a])
    }

    fn brush_preview_color(&self) -> Color32 {
        let c = self.brush_rgba().0;
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
    }

    /// The eraser's rim color: the theme's eraser neutral, whatever the
    /// foreground is.
    fn eraser_rim_color(&self) -> Color32 {
        atlas_shell::tokens::current()
            .board_eraser
            .preview(self.dark_mode)
    }

    /// The eraser's tip and size-HUD fill: its rim color at the erase strength.
    pub(crate) fn eraser_preview_color(&self) -> Color32 {
        self.eraser_rim_color()
            .gamma_multiply(self.eraser_opacity.clamp(0.0, 1.0))
    }

    /// Rim of the tip disc: the eraser neutral for the Eraser, white otherwise.
    fn tip_rim_color(&self) -> Color32 {
        if self.board_tool == BoardTool::Eraser {
            self.eraser_rim_color()
        } else {
            Color32::WHITE.gamma_multiply(0.9)
        }
    }

    /// Commit a freehand brush path node, with one stamped tip per vertex
    /// when `tips` vary (P1.curve.tip-chord). One stroke = one journaled
    /// Add; the Brush tool stays armed and the stroke's end becomes the
    /// anchor of the next Shift segment.
    fn commit_brush_bez(&mut self, bez: &BezPath, end: Pos2, tips: Vec<StrokeSpan>) {
        let (rect, mut data) = board_path::bezpath_to_path_data(bez, false);
        if data.is_empty() {
            return;
        }
        let mut stroke = self.brush_stroke();
        if let (Some(&first), Some(&last)) = (tips.first(), tips.last()) {
            let uniform = tips.iter().all(|t| *t == first);
            if slate_doc::vertex_style::set_vertex_tips(&mut data, &mut stroke, tips) {
                if !data.tips.is_empty() {
                    stroke.softness = last.softness;
                    stroke.texture = last.texture;
                    stroke.color = last.color;
                }
            } else if uniform {
                slate_doc::vertex_style::set_stroke_tip(&mut stroke, first);
            }
        }
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke,
                corner: slate_doc::scene::Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),

                text: None,
            }),
        );
        let ids = self.commit_created_nodes(vec![node]);
        let id = ids.first().copied();
        self.hold_brush_live(id);
        self.set_brush_anchor(end, id);
        self.push_history(
            atlas_commands::CommandId("board.brush.stroke"),
            Some("stroke".into()),
        );
    }

    /// The live brush canvas shows stroke `id` as committed; it stands in
    /// until the stroke's own raster lands, so the commit frame builds none.
    fn hold_brush_live(&mut self, id: Option<NodeId>) {
        let Some(id) = id else {
            return;
        };
        let key = self
            .doc()
            .scene
            .node(id)
            .and_then(board_path::node_stamp_key);
        if let Some(canvas) = self.brush_live.as_mut() {
            canvas.hold(id, key);
        }
    }

    /// [`Self::hold_brush_live`] for a committed Shift segment from `a` to
    /// `b`, each end at its tip.
    fn hold_brush_line(&mut self, id: Option<NodeId>, a: (Pos2, BrushTip), b: (Pos2, BrushTip)) {
        let Some(id) = id else {
            return;
        };
        let key = self
            .doc()
            .scene
            .node(id)
            .and_then(board_path::node_stamp_key);
        if let Some(canvas) = self.brush_live.as_mut() {
            let at = |(p, tip): (Pos2, BrushTip)| vector_ink::TipPoint {
                pos: [p.x, p.y],
                tip: tip.stamp(),
            };
            canvas.hold_line(id, key, at(a), at(b));
        }
    }

    fn set_brush_anchor(&mut self, pos: Pos2, node: Option<NodeId>) {
        self.brush_line_anchor = Some(BrushAnchor {
            pos,
            tip: self.tip_now(),
            node,
        });
    }

    /// Freehand brush release at the brush tip now: same fitter as the Pen,
    /// expressive defaults.
    pub(crate) fn finish_freehand_brush(&mut self, points: Vec<Pos2>) {
        let Some(&first) = points.first() else {
            return;
        };
        let tip = self.tip_now().span();
        let mut stroke = board_path::FreehandTips::new(first, tip);
        let zoom = self.tab().cam.z;
        for p in &points[1..] {
            stroke.push(*p, tip, zoom);
        }
        self.finish_freehand_brush_stroke(&stroke);
    }

    /// Freehand brush release: one fit, split at its tip blends, one
    /// stamped tip per fitted vertex as the stroke drew it
    /// (P1.curve.tip-chord).
    pub(crate) fn finish_freehand_brush_stroke(
        &mut self,
        drawn: &board_path::FreehandTips<StrokeSpan>,
    ) {
        let points = &drawn.points;
        if points.is_empty() {
            return;
        }
        if points.len() == 1 {
            let (rect, data) = board_path::points_to_path_data(points, false);
            let tip = drawn.tips[0];
            let stroke = Stroke {
                width: tip.width,
                softness: tip.softness,
                color: tip.color,
                texture: tip.texture,
                ..self.brush_stroke()
            };
            let node = self.doc_mut().scene.build_node(
                rect,
                NodeKind::Shape(ShapeNode {
                    shape: ShapeKind::Path,
                    fill: None,
                    stroke,
                    corner: slate_doc::scene::Corner::Square,
                    sides: slate_doc::scene::default_regular_sides(),
                    phase_deg: 0.0,
                    flip: false,
                    path: Some(data.into()),
                    text: None,
                }),
            );
            let ids = self.commit_created_nodes(vec![node]);
            let id = ids.first().copied();
            self.hold_brush_live(id);
            self.set_brush_anchor(points[0], id);
            self.push_history(
                atlas_commands::CommandId("board.brush.stroke"),
                Some("dab".into()),
            );
            return;
        }
        let zoom = self.tab().cam.z.max(f32::EPSILON);
        let tol = board_path::FREEHAND_FIT_ERROR_PX / zoom;
        let spacing = board_path::FREEHAND_SAMPLE_SPACING_PX / zoom;
        let Some((bez, tips)) = drawn.fit(tol, spacing) else {
            return;
        };
        let end = *points.last().expect("len >= 2");
        self.commit_brush_bez(&bez, end, tips);
    }

    pub(crate) fn tip_now(&self) -> BrushTip {
        BrushTip {
            width: self.brush_width,
            softness: self.brush_softness,
            color: self.brush_rgba(),
            texture: self.brush_texture,
        }
    }

    /// Straight Shift segment from `a` to `b`. When `a` is the end of the
    /// brush stroke `anchor` just drew, the segment extends that stroke, so
    /// the whole chain is one stamp: joints and self-overlaps keep one
    /// opacity. The tip lerps from the press to the release, so a
    /// right-button size, color, or opacity change shows along the segment.
    pub(crate) fn commit_tween_line(
        &mut self,
        a: Pos2,
        b: Pos2,
        start: BrushTip,
        anchor: Option<NodeId>,
    ) {
        if (b - a).length() <= 0.5 {
            return;
        }
        let end = self.tip_now();
        if let Some(id) = anchor {
            if self.extend_brush_chain(id, a, b, end) {
                self.hold_brush_line(Some(id), (a, start), (b, end));
                self.set_brush_anchor(b, Some(id));
                self.push_history(
                    atlas_commands::CommandId("board.brush.stroke"),
                    Some("line".into()),
                );
                return;
            }
        }
        let (rect, mut data) = board_path::points_to_path_data(&[a, b], false);
        let mut stroke = self.brush_stroke();
        stroke.width = start.width.max(end.width);
        stroke.softness = end.softness;
        stroke.color = end.color;
        if start.span() != end.span() {
            data.tips = vec![start.span(), end.span()];
        }
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Path,
                fill: None,
                stroke,
                corner: slate_doc::scene::Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: Some(data.into()),
                text: None,
            }),
        );
        let ids = self.commit_created_nodes(vec![node]);
        self.hold_brush_line(ids.first().copied(), (a, start), (b, end));
        self.set_brush_anchor(b, ids.first().copied());
        self.push_history(
            atlas_commands::CommandId("board.brush.stroke"),
            Some("line".into()),
        );
    }

    /// Append a straight segment to stamped brush path `id`, whose last
    /// vertex must be `from`. Journaled through `patch_nodes`.
    fn extend_brush_chain(&mut self, id: NodeId, from: Pos2, to: Pos2, end: BrushTip) -> bool {
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        if node.locked || node.hidden {
            return false;
        }
        let NodeKind::Shape(shape) = &node.kind else {
            return false;
        };
        let Some(path) = shape.path.as_ref() else {
            return false;
        };
        if shape.shape != ShapeKind::Path
            || !shape.stroke.paints_as_stamp()
            || path.closed
            || !path.extra.is_empty()
        {
            return false;
        }
        let mut bez = board_path::path_data_to_world_bez(path, node.rect, node.rotation_deg);
        let last = match bez.elements().last() {
            Some(vector_ink::kurbo::PathEl::MoveTo(p) | vector_ink::kurbo::PathEl::LineTo(p)) => *p,
            Some(vector_ink::kurbo::PathEl::QuadTo(_, p)) => *p,
            Some(vector_ink::kurbo::PathEl::CurveTo(_, _, p)) => *p,
            _ => return false,
        };
        let slop = 1.0 / self.tab().cam.z.max(0.05);
        if ((last.x as f32 - from.x).powi(2) + (last.y as f32 - from.y).powi(2)).sqrt() > slop {
            return false;
        }
        let vertices = 1 + path.segs.len();
        let mut tips = path.paint_tips(&shape.stroke);
        if tips.len() != vertices {
            tips = vec![slate_doc::scene::StrokeSpan::of(&shape.stroke); vertices];
        }
        tips.push(end.span());
        bez.line_to((to.x as f64, to.y as f64));
        let (rect, mut data) = board_path::bezpath_to_path_data(&bez, false);
        let widest = tips.iter().map(|t| t.width).fold(0.0_f32, f32::max);
        data.tips = tips;
        // Each Shift segment is its own undo step, not a coalesced edit.
        self.last_board_edit = None;
        self.patch_nodes(&[id], |n| {
            n.rect = rect;
            n.rotation_deg = 0.0;
            if let NodeKind::Shape(s) = &mut n.kind {
                s.path = Some(data.clone().into());
                s.stroke.width = widest;
                s.stroke.softness = end.softness;
                s.stroke.color = end.color;
                s.stroke.tween_from = None;
            }
        });
        true
    }

    /// Where the live Shift segment starts: the end of the last brush mark,
    /// else the press. With the tip there and the stroke it may extend.
    pub(crate) fn brush_straight_from(&self) -> Option<(Pos2, BrushTip, Option<NodeId>)> {
        let g = self.brush_straight.as_ref()?;
        Some(
            self.brush_line_anchor
                .map_or((g.start, g.tip, None), |a| (a.pos, a.tip, a.node)),
        )
    }

    /// The end of a painted straight segment (the Brush or Eraser Shift
    /// line) from `from`, pressed at `press`, with the pointer at `world`.
    /// A click connects to the click point exactly. A drag past the click
    /// threshold takes the Tab direction lock, else 45° steps from `from`.
    /// On painted ink Shift is the straight-line modifier itself, so F8 adds
    /// the steps but never cancels them (the vector tools keep Rhino's
    /// Shift-inverts-ortho). Preview and release both read this.
    pub(crate) fn painted_segment_end(
        &self,
        from: Pos2,
        press: Pos2,
        world: Pos2,
        shift: bool,
    ) -> Pos2 {
        let dragged = (world - press).length() * self.tab().cam.z > BRUSH_MOD_CLICK_PX;
        let ortho = dragged && (shift || self.board_ortho);
        self.constrain_segment_end(from, world, ortho)
    }

    /// [`Self::painted_segment_end`] for the live Brush Shift segment.
    pub(crate) fn brush_straight_end(&self, world: Pos2, shift: bool) -> Option<Pos2> {
        let g = self.brush_straight.as_ref()?;
        let (from, ..) = self.brush_straight_from()?;
        Some(self.painted_segment_end(from, g.start, world, shift))
    }

    pub(crate) fn release_brush_straight(&mut self, end_screen: Pos2, end_world: Pos2, shift: bool) {
        let (Some(end_world), Some((from, tip, node))) = (
            self.brush_straight_end(end_world, shift),
            self.brush_straight_from(),
        ) else {
            return;
        };
        let Some(gesture) = self.brush_straight.take() else {
            return;
        };
        self.draft_lock = None;
        let travel = end_screen.distance(gesture.start_screen);
        if travel <= BRUSH_MOD_CLICK_PX && self.brush_line_anchor.is_none() {
            self.brush_line_anchor = Some(BrushAnchor {
                pos: end_world,
                tip: gesture.tip,
                node: None,
            });
        } else if (end_world - from).length() > 0.5 {
            self.commit_tween_line(from, end_world, tip, node);
        } else {
            self.set_brush_anchor(end_world, node);
        }
    }

    // ---------- eraser (E) ----------

    /// Path/Line stroke nodes under the eraser circle at `world`
    /// (pick radius = eraser width / 2). Images, text, frames, and
    /// connectors are never erasable; hidden/locked strokes are skipped.
    #[cfg(test)]
    pub(crate) fn eraser_hits_at(&self, world: Pos2) -> Vec<NodeId> {
        self.eraser_hits_along(world, world)
    }

    /// The same strokes, for the eraser circle swept from `from` to `to`.
    pub(crate) fn eraser_hits_along(&self, from: Pos2, to: Pos2) -> Vec<NodeId> {
        self.vector_sweep_hits_along(from, to, self.eraser_width)
    }

    /// Vector Path/Line strokes the circular pick touches anywhere along the
    /// segment `from` to `to`.
    pub(crate) fn vector_sweep_hits_along(
        &self,
        from: Pos2,
        to: Pos2,
        pick_width: f32,
    ) -> Vec<NodeId> {
        let zoom = self.tab().cam.z;
        let slop = (pick_width * 0.5).max(1.0);
        let reach = slop + super::settings::STROKE_WIDTH_MAX * 0.5;
        let query = slate_doc::scene::WorldRect::new(
            from.x.min(to.x) - reach,
            from.y.min(to.y) - reach,
            (from.x - to.x).abs() + reach * 2.0,
            (from.y - to.y).abs() + reach * 2.0,
        );
        let mut hits = Vec::new();
        for id in self.doc().scene.query_rect(query) {
            let Some(n) = self.doc().scene.node(id) else {
                continue;
            };
            if eraser_sweep_hits_node(n, from, to, slop, zoom) {
                hits.push(n.id);
            }
        }
        hits
    }

    /// The eraser tip on painted strokes. Alpha is the erase strength.
    pub(crate) fn eraser_tip(&self) -> vector_ink::StampStyle {
        vector_ink::StampStyle {
            diameter: self.eraser_width.max(0.0),
            softness: self.eraser_softness,
            rgba: [
                0,
                0,
                0,
                (self.eraser_opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            ],
            grain: self.eraser_texture.grain(),
        }
    }

    /// Press with the Eraser. Shift makes the pass a straight line from the
    /// end of the last pass (or from the press when there is none).
    pub(crate) fn begin_erase(&mut self, world: Pos2, shift: bool) -> super::board::BoardDrag {
        self.erase_live.clear();
        let points = if shift {
            vec![self.eraser_anchor.unwrap_or(world), world]
        } else {
            vec![world]
        };
        let mut drag = super::board::BoardDrag::Erase {
            touched: Vec::new(),
            points,
            straight: shift,
            spot: Vec::new(),
            press: world,
        };
        self.collect_erase_hits(&mut drag, world);
        drag
    }

    /// Extend the pass to `world`. A Shift pass is a straight line whose end
    /// is a [`Self::painted_segment_end`], as for the Brush.
    pub(crate) fn update_erase(&mut self, world: Pos2) {
        let Some(mut drag) = self.board_drag.take() else {
            return;
        };
        let zoom = self.tab().cam.z;
        let mut hit_end = world;
        if let super::board::BoardDrag::Erase {
            points,
            straight,
            press,
            ..
        } = &mut drag
        {
            if *straight {
                let end = points.first().map_or(world, |from| {
                    self.painted_segment_end(*from, *press, world, self.shift_down)
                });
                if let Some(last) = points.last_mut() {
                    *last = end;
                }
                hit_end = end;
            } else if points
                .last()
                .is_none_or(|p| (*p - world).length() * zoom >= 0.75)
            {
                points.push(world);
            }
        }
        self.collect_erase_hits(&mut drag, hit_end);
        self.board_drag = Some(drag);
    }

    /// Vector strokes under the cursor join `touched`. Painted strokes whose
    /// ink bounds reach the pass join `spot`.
    fn collect_erase_hits(&self, drag: &mut super::board::BoardDrag, world: Pos2) {
        let super::board::BoardDrag::Erase {
            touched,
            points,
            straight,
            spot,
            ..
        } = drag
        else {
            return;
        };
        // The stretch of the pass since the last hit test: the whole line
        // for a straight pass, otherwise from the previous pointer sample.
        let from = if *straight {
            points.first().copied()
        } else {
            points.iter().rev().find(|p| **p != world).copied()
        }
        .unwrap_or(world);
        for h in self
            .eraser_hits_along(from, world)
            .into_iter()
            .chain(self.eraser_hits_active_layer_along(from, world))
        {
            if !touched.contains(&h) {
                touched.push(h);
            }
        }
        self.collect_layer_erase_spot(spot, points, *straight);
        let r = self.eraser_width * 0.5;
        let (x0, y0) = (from.x.min(world.x) - r, from.y.min(world.y) - r);
        let (x1, y1) = (from.x.max(world.x) + r, from.y.max(world.y) + r);
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return;
        }
        let ink =
            super::settings::STROKE_WIDTH_MAX * 0.5 + 3.0 * super::board_smooth::SMOOTH_BLUR_MAX;
        let query = slate_doc::scene::WorldRect::new(
            x0 - ink,
            y0 - ink,
            (x1 - x0) + ink * 2.0,
            (y1 - y0) + ink * 2.0,
        );
        let candidates = self.doc().scene.query_rect(query);
        for id in candidates {
            let Some(n) = self.doc().scene.node(id) else {
                continue;
            };
            if spot.contains(&n.id) {
                continue;
            }
            if spot_reaches(n, WorldRect::new(x0, y0, x1 - x0, y1 - y0))
                && spot_touches(n, from, world, r)
            {
                spot.push(n.id);
            }
        }
    }

    /// Eraser release. Painted strokes keep an erase pass (one journal group
    /// of patches); ones left with no visible ink, and vector strokes the
    /// pass crossed, are removed.
    pub(crate) fn finish_erase(
        &mut self,
        touched: Vec<NodeId>,
        points: Vec<Pos2>,
        spot: Vec<NodeId>,
    ) {
        let mut live = std::mem::take(&mut self.erase_live);
        self.draft_lock = None;
        if let Some(last) = points.last() {
            self.eraser_anchor = Some(*last);
        }
        let tip = self.eraser_tip();
        let span = slate_doc::scene::StrokeSpan {
            width: tip.diameter,
            softness: tip.softness,
            color: Rgba([0, 0, 0, tip.rgba[3]]),
            texture: self.eraser_texture,
        };
        let mut cmds = Vec::new();
        let mut board_removes = Vec::new();
        for id in touched {
            if let Some(loc) = slate_doc::image_paint::find_layer_node(&self.doc().scene, id) {
                let Some(host) = self.doc().scene.node(loc.image) else {
                    continue;
                };
                let NodeKind::Image(img) = &host.kind else {
                    continue;
                };
                let Some(layer) = img.paint_layers.get(loc.layer_index) else {
                    continue;
                };
                let Some(node) = layer.nodes.get(loc.node_index).cloned() else {
                    continue;
                };
                cmds.push(slate_doc::scene::SceneCmd::LayerNodeRemove {
                    host: loc.image,
                    layer: layer.id,
                    index: loc.node_index,
                    node,
                });
            } else if let Some((index, node)) = self
                .doc()
                .scene
                .nodes
                .iter()
                .position(|node| node.id == id)
                .and_then(|index| self.doc().scene.node(id).cloned().map(|node| (index, node)))
            {
                board_removes.push((index, node));
            }
        }
        // Strokes whose preview becomes their stand-in once the patch lands.
        let mut stand_ins = Vec::new();
        for id in &spot {
            // Only strokes the pass visibly changed. The live preview already
            // holds the result, so release reads it instead of stamping the
            // stroke again on the frame loop. Without one (headless), fall
            // back to stamping the result.
            if live.get(id).is_some_and(|l| !l.changed) {
                continue;
            }
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let result = match live.get(id) {
                Some(l) => Some((with_erase_mark(&before, &points, span), !l.left_ink())),
                None => stamp_erase_mark(&before, &points, span),
            };
            let Some((after, gone)) = result else {
                continue;
            };
            if !gone && live.contains_key(id) {
                stand_ins.push((*id, after.rect));
            }
            if gone {
                if let Some(index) = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .position(|node| node.id == *id)
                {
                    board_removes.push((index, before));
                }
            } else {
                cmds.push(slate_doc::scene::SceneCmd::Patch {
                    before: Box::new(before),
                    after: Box::new(after),
                });
            }
        }
        let (layer_cmds, _) = self.finish_erase_layer_spot(&spot, &points, span, &live);
        cmds.extend(layer_cmds);
        board_removes.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
        cmds.extend(
            board_removes
                .into_iter()
                .map(|(index, node)| slate_doc::scene::SceneCmd::Remove { index, node }),
        );
        let n = cmds.len();
        if !cmds.is_empty() {
            self.last_board_edit = None;
            self.commit_scene(cmds);
        }
        for (id, rect) in stand_ins {
            if let Some(l) = live.remove(&id) {
                let gpu = l.into_stand_in(rect, self.frame_no);
                self.brush_stamps.insert(id, (0, gpu));
            }
        }
        if n > 0 {
            self.push_history(
                atlas_commands::CommandId("board.eraser.stroke"),
                Some(format!("{n} stroke(s)")),
            );
        }
    }

    // ---------- eyedropper (I / Alt while Brush) ----------

    /// All eyedroppers enter the shared desktop picker, including image pixels
    /// and other applications. Tool alpha is preserved when RGB is accepted.
    pub(crate) fn eyedropper_click(&mut self, _world: Pos2, to_bg: bool) {
        let temporary = self.board_tool == BoardTool::Brush && self.alt_down;
        if temporary && !to_bg {
            if let Some(rgb) = atlas_shell::desktop_color::sample_cursor() {
                self.adopt_sampled_brush_color(rgb);
                return;
            }
        }
        self.start_tool_desktop_sample(to_bg, temporary);
    }

    /// A temporary Alt sample becomes the foreground (alpha kept) and a
    /// recent color.
    pub(crate) fn adopt_sampled_brush_color(&mut self, rgb: [u8; 3]) {
        self.board_colors.fg.0[..3].copy_from_slice(&rgb);
        self.save_board_colors();
        self.remember_document_colors(vec![rgb]);
    }

    /// Whether the eyedropper is live this frame: the tool itself, or
    /// spring-loaded Alt while Brush is armed (Alt-drag duplicate is a
    /// Select-tool gesture and is untouched).
    pub(crate) fn eyedropper_active(&self) -> bool {
        if self.brush_hud.is_some() {
            return false;
        }
        self.board_tool == BoardTool::Eyedropper
            || (self.board_tool == BoardTool::Brush && self.alt_down)
    }

    // ---------- sticky note (N) ----------

    /// Click-to-place sticky: a Text-node preset (white fill).
    /// The caret enters immediately and the tool returns to Select, so the
    /// place ghost does not stay under the pointer.
    pub(crate) fn place_sticky_at(&mut self, world: Pos2) {
        let rect = WorldRect::new(
            world.x - STICKY_SIZE * 0.5,
            world.y - STICKY_SIZE * 0.5,
            STICKY_SIZE.max(MIN_DRAW),
            STICKY_SIZE.max(MIN_DRAW),
        );
        let id = self.insert_sticky(rect);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.text_edit = Some((id, String::new()));
        self.disarm_create();
        self.push_history(
            atlas_commands::CommandId("board.tool.sticky"),
            Some("placed".into()),
        );
    }

    fn insert_sticky(&mut self, rect: WorldRect) -> NodeId {
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Text(TextNode {
                text: String::new(),
                family: Typeface::Sans,
                size: 24.0,
                color: STICKY_INK,
                align: TextAlign::Center,
                fill: Some(STICKY_FILL),
                stroke: Default::default(),
                agent: None,
            }),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        id
    }

    /// Tab / Shift+Tab while editing a sticky: spawn a sibling to the right
    /// (left with Shift), same size/fill, 24-unit gap, caret moves there.
    pub(crate) fn spawn_adjacent_sticky(&mut self, from: NodeId, dir: f32) {
        let Some(src) = self.doc().scene.node(from).cloned() else {
            return;
        };
        let NodeKind::Text(t) = &src.kind else {
            return;
        };
        let fill = t.fill;
        let (family, size, color, align) = (t.family, t.size, t.color, t.align);
        let rect = src.rect.translated(dir * (src.rect.w + STICKY_GAP), 0.0);
        let node = self.doc_mut().scene.build_node(
            rect,
            NodeKind::Text(TextNode {
                text: String::new(),
                family,
                size,
                color,
                align,
                fill,
                stroke: Default::default(),
                agent: None,
            }),
        );
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.text_edit = Some((id, String::new()));
        self.push_history(
            atlas_commands::CommandId("board.tool.sticky"),
            Some(
                if dir >= 0.0 {
                    "tab-spawn right"
                } else {
                    "tab-spawn left"
                }
                .into(),
            ),
        );
    }

    // ---------- cursor feedback ----------

    /// Screen radius, softness, and color of the tip disc for Brush, Eraser,
    /// Smooth, and Pen; `None` for every other tool.
    pub(crate) fn width_cursor_disc(&self) -> Option<(f32, f32, Color32)> {
        let z = self.tab().cam.z;
        let (w, softness, strength) = self.chord_tip();
        let ink = match self.board_tool {
            BoardTool::Brush => self.brush_preview_color(),
            BoardTool::Eraser => self.eraser_preview_color(),
            BoardTool::Smooth => Color32::from_gray(160).gamma_multiply(strength.clamp(0.0, 1.0)),
            BoardTool::Pen => {
                super::board::rgba32(self.stroke_for_tool(slate_doc::StrokeTool::Pen).color)
            }
            _ => return None,
        };
        Some(((w * 0.5 * z).max(1.5), softness, ink))
    }

    /// Filled tip while Brush, Eraser, Smooth, or Pen is armed. Softness is
    /// a solid core that fades to the rim, the same stamp Photoshop shows for
    /// the brush.
    pub(crate) fn paint_width_cursor(&self, painter: &egui::Painter, pointer: Pos2) {
        let Some((r, softness, ink)) = self.width_cursor_disc() else {
            return;
        };
        paint_soft_disc(painter, pointer, r, softness, ink, self.tip_rim_color());
        if self.shift_down {
            if let Some(label) = self.brush_status_line() {
                painter.text(
                    pointer + egui::vec2(0.0, -r - 14.0),
                    egui::Align2::CENTER_BOTTOM,
                    label,
                    egui::FontId::proportional(13.0),
                    Color32::WHITE,
                );
            }
        }
    }

    /// Eyedropper sampling ring: outer half = hovered candidate color,
    /// inner = current fg (PS sampling-ring adaptation).
    pub(crate) fn paint_eyedropper_cursor(
        &self,
        painter: &egui::Painter,
        pointer: Pos2,
        world: Pos2,
    ) {
        let _ = world;
        let candidate: Option<Rgba> = None;
        let fg = super::board::rgba32(self.board_colors.fg);
        if let Some(c) = candidate {
            painter.circle_stroke(
                pointer,
                11.0,
                EStroke::new(4.0_f32, super::board::rgba32(c)),
            );
        } else {
            painter.circle_stroke(
                pointer,
                11.0,
                EStroke::new(1.2_f32, Color32::from_gray(150)),
            );
        }
        // The sample reads the screen pixel under the hotspot, so the
        // foreground swatch is a ring that leaves that pixel showing the canvas.
        painter.circle_stroke(pointer, 5.0, EStroke::new(3.0_f32, fg));
        painter.circle_stroke(
            pointer,
            6.8,
            EStroke::new(1.0_f32, Color32::from_black_alpha(90)),
        );
        painter.circle_stroke(
            pointer,
            3.2,
            EStroke::new(1.0_f32, Color32::from_black_alpha(90)),
        );
    }

    /// Bottom-readout line while Brush or Eraser is armed.
    pub(crate) fn brush_status_line(&self) -> Option<String> {
        if self.doc().view.active_view != slate_doc::ViewKind::Board {
            return None;
        }
        let tool = self.board_tool;
        let vector = super::board_tip_hud::curve_tool(tool) || self.hud_node().is_some();
        if !vector
            && !matches!(
                tool,
                BoardTool::Brush | BoardTool::Eraser | BoardTool::Smooth
            )
        {
            return None;
        }
        let z = self.tab().cam.z.max(f32::EPSILON);
        let (width, softness, opacity) = self.active_tip();
        let px = width * z;
        if vector && !self.tip_hud_has_softness() {
            return Some(format!("{px:.0} px · {:.0}% opacity", opacity * 100.0));
        }
        let soft = if softness <= 0.001 {
            "hard".to_string()
        } else {
            format!("{:.0}% soft", softness * 100.0)
        };
        let amount = match tool {
            BoardTool::Eraser | BoardTool::Smooth => "strength",
            _ => "opacity",
        };
        Some(format!(
            "{px:.0} px · {soft} · {:.0}% {amount}",
            opacity * 100.0
        ))
    }

    pub(crate) fn drive_brush_hud(
        &mut self,
        pointer: Option<Pos2>,
        secondary_down: bool,
        secondary_pressed: bool,
    ) -> bool {
        if self.brush_hud.is_some() {
            if secondary_down {
                if let Some(pointer) = pointer {
                    self.update_brush_hud(pointer);
                }
                return true;
            }
            self.commit_brush_hud();
            return true;
        }
        let armed = self.tip_hud_armed();
        // A held button counts, not only the press edge. The modifier often
        // arrives on the same chord a frame after `button_pressed` has passed.
        if (!secondary_down && !secondary_pressed) || !armed {
            return false;
        }
        let Some(pointer) = pointer else {
            return false;
        };
        if self.tip_hud_has_color() && self.ctrl_down {
            let fg = self.active_rgba();
            let hsv = rgb_to_hsv([fg[0], fg[1], fg[2]]);
            self.begin_hud_node();
            self.brush_hud = Some(BrushHud::Wheel {
                fg0: fg,
                center: pointer - sv_offset(hsv[1], hsv[2]),
                hsv,
                sampling: false,
                dot: None,
            });
            return true;
        }
        if self.alt_down {
            let (width0, softness0, _) = self.chord_tip();
            self.brush_hud_before = Some(self.brush_setting_snapshot());
            self.begin_hud_node();
            self.brush_hud = Some(BrushHud::Size {
                origin: self.board_xf().s2w(pointer),
                width0,
                softness0,
            });
            return true;
        }
        // Shift alone. Ctrl and Alt already claimed the button above.
        if self.shift_down && armed {
            self.brush_hud_before = Some(self.brush_setting_snapshot());
            self.begin_hud_node();
            self.brush_hud = Some(BrushHud::Opacity {
                origin: self.board_xf().s2w(pointer),
                opacity0: self.active_tip().2,
            });
            return true;
        }
        false
    }

    fn update_brush_hud(&mut self, pointer: Pos2) {
        let Some(mut hud) = self.brush_hud else {
            return;
        };
        match &mut hud {
            BrushHud::Size {
                origin,
                width0,
                softness0,
            } => {
                use super::board_tip_hud::{palette_band_y, palette_hit, palette_zone};
                let z = self.tab().cam.z.max(f32::EPSILON);
                let o = self.board_xf().w2s(*origin);
                let r_now = (self.active_tip().0 * 0.5 * z).max(1.5);
                let choices = self.tip_choices();
                let softens = self.tip_hud_has_softness();
                // Anywhere in the band under the circle the pointer picks a
                // style: size holds, and softness stays at the hardest edge
                // the way down reached, so each style is entered hard.
                if palette_zone(o, r_now, choices.len(), pointer) {
                    if softens {
                        let (w, _, opacity) = self.active_tip();
                        self.set_active_tip(w, 0.0, opacity);
                    }
                    if let Some(i) = palette_hit(o, r_now, choices.len(), pointer) {
                        self.apply_tip_choice(choices[i]);
                    }
                    self.brush_hud = Some(hud);
                    return;
                }
                let px = scrub_diameter_px(*width0 * z, pointer.x - o.x);
                let width = (px / z).clamp(
                    super::settings::STROKE_WIDTH_MIN,
                    super::settings::STROKE_WIDTH_MAX,
                );
                let dy = pointer.y - o.y;
                let soft = if !softens {
                    *softness0
                } else if choices.is_empty() || dy <= 0.0 {
                    scrub_softness(*softness0, dy)
                } else {
                    let r = (width * 0.5 * z).max(1.5);
                    let reach = (palette_band_y(o, r) - o.y).max(1.0);
                    scrub_softness(*softness0, dy).min(*softness0 * (1.0 - dy / reach).max(0.0))
                };
                let opacity = self.active_tip().2;
                self.set_active_tip(width, soft, opacity);
            }
            BrushHud::Opacity { origin, opacity0 } => {
                let (w, soft, _) = self.active_tip();
                let o = self.board_xf().w2s(*origin);
                self.set_active_tip(w, soft, scrub_opacity(*opacity0, pointer.y - o.y));
            }
            BrushHud::Wheel {
                center,
                hsv,
                sampling,
                dot,
                ..
            } => {
                let local = pointer - *center;
                let hit = sample_wheel(
                    [local.x, local.y],
                    *hsv,
                    self.doc().view.recent_colors.as_deref().unwrap_or(&[]),
                );
                let entered = match hit {
                    WheelHit::Dot(_, slot) => Some(slot),
                    _ => None,
                };
                let newly = entered.is_some() && entered != *dot;
                *dot = entered;
                *sampling = matches!(hit, WheelHit::Outside);
                match hit {
                    WheelHit::Keep => {}
                    WheelHit::Outside => {
                        if let Some(rgb) = atlas_shell::desktop_color::sample_cursor() {
                            self.set_active_rgb(rgb);
                        }
                    }
                    WheelHit::Dot(rgb, slot) => {
                        self.set_active_rgb(rgb);
                        *hsv = rgb_to_hsv(rgb);
                        let target = wheel_slot_center(*center, slot);
                        if newly && pointer.distance(target) > 0.25 {
                            self.brush_cursor_warp = Some((pointer, target));
                        }
                    }
                    WheelHit::Field(rgb, next) => {
                        self.set_active_rgb(rgb);
                        *hsv = next;
                    }
                }
            }
        }
        self.brush_hud = Some(hud);
    }

    pub(crate) fn commit_brush_hud(&mut self) {
        self.brush_cursor_warp = None;
        let before = self.brush_hud_before.take();
        let node_before = self.hud_node_before.take();
        let Some(hud) = self.brush_hud.take() else {
            return;
        };
        match hud {
            BrushHud::Size { .. } if self.armed_stroke_tool().is_some() => {
                self.flush_create_style_to_doc();
            }
            BrushHud::Size { .. } | BrushHud::Opacity { .. } => self.settings.save(),
            BrushHud::Wheel { .. } => {
                let c = self.active_rgba();
                self.save_board_colors();
                self.remember_document_colors(vec![[c[0], c[1], c[2]]]);
            }
        }
        if let Some(before) = before {
            self.push_brush_setting_undo(before);
        }
        self.journal_hud_node(node_before);
        self.resume_freehand_after_hud();
    }

    /// A tip chord paused a freehand Pen or Brush stroke: move the pointer
    /// back onto the last drawn sample, through the swatch warp's path, so
    /// the stroke continues from where it paused instead of drawing a
    /// straight jump across the scrub.
    fn resume_freehand_after_hud(&mut self) {
        let last = match &self.board_drag {
            Some(super::board::BoardDrag::FreehandPen { stroke }) => stroke.last(),
            Some(super::board::BoardDrag::FreehandBrush { stroke }) => stroke.last(),
            _ => return,
        };
        let target = self.board_xf().w2s(last);
        self.brush_cursor_warp = Some((target, target));
        self.freehand_resume = true;
    }

    pub(crate) fn cancel_brush_hud(&mut self) {
        self.brush_cursor_warp = None;
        self.brush_hud_before = None;
        let node_before = self.hud_node_before.take();
        let Some(hud) = self.brush_hud.take() else {
            return;
        };
        match hud {
            BrushHud::Size {
                width0, softness0, ..
            } => {
                let opacity = self.chord_tip().2;
                self.set_chord_tip(width0, softness0, opacity);
            }
            BrushHud::Opacity { opacity0, .. } => {
                let (w, soft, _) = self.active_tip();
                self.set_active_tip(w, soft, opacity0);
            }
            BrushHud::Wheel { fg0, .. } => self.set_active_rgba(fg0),
        }
        self.restore_hud_node(node_before);
        self.resume_freehand_after_hud();
    }

    /// Pointer-attached HUD. Numbers stay in screen px (P2.GhostFollow).
    /// The size and opacity circles stay pinned on the press point.
    pub(crate) fn paint_brush_hud(&self, painter: &egui::Painter, pointer: Pos2, accent: Color32) {
        match self.brush_hud {
            Some(BrushHud::Size { origin, .. }) => {
                let o = self.board_xf().w2s(origin);
                self.paint_size_hud(painter, o, accent);
                let z = self.tab().cam.z.max(f32::EPSILON);
                let r = (self.active_tip().0 * 0.5 * z).max(1.5);
                self.paint_tip_palette(painter, o, r, pointer, accent);
            }
            Some(BrushHud::Opacity { origin, .. }) => {
                self.paint_size_hud(painter, self.board_xf().w2s(origin), accent)
            }
            Some(BrushHud::Wheel {
                center,
                hsv,
                sampling,
                ..
            }) => {
                if sampling {
                    self.paint_color_wheel(painter, center, hsv);
                    self.paint_eyedropper_cursor(painter, pointer, pointer);
                } else {
                    self.paint_color_wheel(painter, center, hsv)
                }
            }
            None => {}
        }
    }

    fn paint_size_hud(&self, painter: &egui::Painter, pointer: Pos2, _accent: Color32) {
        let z = self.tab().cam.z.max(f32::EPSILON);
        let eraser = self.board_tool == BoardTool::Eraser;
        let (width, softness, opacity) = self.active_tip();
        let r = (width * 0.5 * z).max(1.5);
        let ink = if eraser {
            self.eraser_preview_color()
        } else if self.vector_tip().is_some() {
            let c = self.active_rgba();
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]).gamma_multiply(opacity)
        } else {
            self.brush_preview_color()
        };
        paint_soft_disc(painter, pointer, r, softness, ink, self.tip_rim_color());
        let label = self.size_hud_label().unwrap_or_default();
        painter.text(
            pointer + egui::vec2(0.0, -r - 14.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            egui::FontId::proportional(13.0),
            Color32::WHITE,
        );
    }

    fn paint_color_wheel(&self, painter: &egui::Painter, center: Pos2, hsv: [f32; 3]) {
        painter.circle_filled(
            center,
            WHEEL_BACKDROP_RADIUS,
            Color32::from_rgba_unmultiplied(18, 18, 20, 235),
        );
        let (disk, ring) = wheel_meshes(painter.ctx(), center, hsv[0]);
        painter.add(egui::Shape::Mesh(disk));
        painter.add(egui::Shape::Mesh(ring));
        for (at, _) in wheel_snaps() {
            let inside = at - at.normalized() * (WHEEL_SNAP_RADIUS * 0.5);
            painter.circle_stroke(
                center + inside,
                2.5,
                EStroke::new(1.0_f32, Color32::from_gray(128)),
            );
        }
        let recents = self.doc().view.recent_colors.as_deref().unwrap_or(&[]);
        let slots = slate_doc::ViewState::WHEEL_COLOR_LIMIT;
        for (index, color) in recents.iter().enumerate().take(slots) {
            let at = wheel_slot_center(center, index);
            painter.circle_filled(
                at,
                WHEEL_DOT_RADIUS,
                Color32::from_rgb(color[0], color[1], color[2]),
            );
            painter.circle_stroke(at, WHEEL_DOT_RADIUS, EStroke::new(1.5_f32, Color32::WHITE));
        }
        let mark = center + sv_offset(hsv[1], hsv[2]);
        painter.circle_stroke(mark, 7.0, EStroke::new(2.0_f32, Color32::WHITE));
        let hue_at = center
            + egui::vec2(
                (hsv[0] * std::f32::consts::TAU).cos() * (WHEEL_HUE_INNER + WHEEL_HUE_OUTER) * 0.5,
                -(hsv[0] * std::f32::consts::TAU).sin() * (WHEEL_HUE_INNER + WHEEL_HUE_OUTER) * 0.5,
            );
        painter.circle_stroke(hue_at, 6.0, EStroke::new(2.0_f32, Color32::WHITE));
    }
}

/// The wheel's saturation/value disk and hue ring at screen `center`,
/// tessellated when the wheel opens and again only when the hue moves (the
/// disk) or the wheel does (Art. II). The wheel is screen chrome, so zoom
/// and pixel density do not change either mesh.
fn wheel_meshes(
    ctx: &egui::Context,
    center: Pos2,
    hue: f32,
) -> (std::sync::Arc<egui::Mesh>, std::sync::Arc<egui::Mesh>) {
    type Cached = ([u32; 3], std::sync::Arc<egui::Mesh>);
    let at = [center.x.to_bits(), center.y.to_bits()];
    let disk_key = [at[0], at[1], hue.to_bits()];
    let ring_key = [at[0], at[1], 0];
    let (disk_id, ring_id) = (
        egui::Id::new("slate.wheel_disk"),
        egui::Id::new("slate.wheel_ring"),
    );
    let cached = |id: egui::Id, key: [u32; 3]| {
        ctx.data(|d| d.get_temp::<Cached>(id))
            .filter(|(k, _)| *k == key)
            .map(|(_, mesh)| mesh)
    };
    let disk = cached(disk_id, disk_key).unwrap_or_else(|| {
        let mesh = std::sync::Arc::new(wheel_disk_mesh(center, hue));
        ctx.data_mut(|d| d.insert_temp(disk_id, (disk_key, mesh.clone())));
        mesh
    });
    let ring = cached(ring_id, ring_key).unwrap_or_else(|| {
        let mesh = std::sync::Arc::new(wheel_ring_mesh(center));
        ctx.data_mut(|d| d.insert_temp(ring_id, (ring_key, mesh.clone())));
        mesh
    });
    (disk, ring)
}

fn wheel_disk_mesh(center: Pos2, hue: f32) -> egui::Mesh {
    let rings = 32u32;
    let slices = 96u32;
    let mut mesh = egui::Mesh::default();
    let color_at = |local: egui::Vec2| {
        let (sat, val) = sv_at(local);
        let rgb = hsv_to_rgb([hue, sat, val]);
        Color32::from_rgb(rgb[0], rgb[1], rgb[2])
    };
    mesh.colored_vertex(center, color_at(egui::Vec2::ZERO));
    // The disk ends at its own radius: the gap out to the hue ring is
    // the backdrop, and picks nothing.
    for ring in 1..=rings {
        let dist = WHEEL_SV_RADIUS * ring as f32 / rings as f32;
        for slice in 0..slices {
            let angle = slice as f32 / slices as f32 * std::f32::consts::TAU;
            let local = egui::vec2(angle.cos() * dist, angle.sin() * dist);
            mesh.colored_vertex(center + local, color_at(local));
        }
    }
    for slice in 0..slices {
        let a = 1 + slice;
        let b = 1 + (slice + 1) % slices;
        mesh.add_triangle(0, a, b);
    }
    for ring in 1..rings {
        let inner = 1 + (ring - 1) * slices;
        let outer = inner + slices;
        for slice in 0..slices {
            let next = (slice + 1) % slices;
            mesh.add_triangle(inner + slice, outer + slice, outer + next);
            mesh.add_triangle(inner + slice, outer + next, inner + next);
        }
    }
    mesh
}

fn wheel_ring_mesh(center: Pos2) -> egui::Mesh {
    let hue_steps = 120u32;
    let mut ring = egui::Mesh::default();
    for i in 0..hue_steps {
        let a = i as f32 / hue_steps as f32 * std::f32::consts::TAU;
        let c = hsv_to_rgb([a / std::f32::consts::TAU, 1.0, 1.0]);
        let c = Color32::from_rgb(c[0], c[1], c[2]);
        let dir = egui::vec2(a.cos(), -a.sin());
        ring.colored_vertex(center + dir * WHEEL_HUE_INNER, c);
        ring.colored_vertex(center + dir * WHEEL_HUE_OUTER, c);
    }
    for i in 0..hue_steps {
        let (a, b) = (2 * i, 2 * ((i + 1) % hue_steps));
        ring.add_triangle(a, a + 1, b + 1);
        ring.add_triangle(a, b + 1, b);
    }
    ring
}

/// Stamp an erased painted stroke once, as committed (blur included):
/// `(pass touched ink, nothing left)`.
pub(crate) fn erased_result(node: &slate_doc::Node) -> (bool, bool) {
    let NodeKind::Shape(shape) = &node.kind else {
        return (false, false);
    };
    let Some(path) = shape.path.as_ref() else {
        return (false, false);
    };
    let contours = board_path::stamped_contours(node, shape, path, 0.5);
    let widest = contours
        .iter()
        .flatten()
        .map(|p| p.tip.diameter)
        .fold(0.0_f32, f32::max);
    let pixel = (widest / 64.0).max(1.0);
    let marks = board_path::stamped_erase_marks(node, shape, path);
    board_path::note_stamp_on_this_thread();
    let (older, newest) = marks.split_at(marks.len().saturating_sub(1));
    let Some(mut img) =
        vector_ink::stamp_blurred(&contours, older, pixel, shape.stroke.gaussian_blur)
    else {
        return (false, true);
    };
    let before: Vec<u8> = img.rgba.iter().skip(3).step_by(4).copied().collect();
    vector_ink::apply_erase(&mut img, newest);
    let touched = img
        .rgba
        .iter()
        .skip(3)
        .step_by(4)
        .zip(&before)
        .any(|(after, before)| after != before);
    let left = img.rgba.iter().skip(3).step_by(4).any(|a| *a > 8);
    (touched, !left)
}

/// World endpoints of a Line shape node (same convention as the painter:
/// `flip` = ↗ diagonal, else ↘), rotated with the node.
pub(crate) fn line_endpoints(rect: WorldRect, flip: bool, rotation_deg: f32) -> (Pos2, Pos2) {
    let (a, b) = if flip {
        (
            Pos2::new(rect.x, rect.y + rect.h),
            Pos2::new(rect.x + rect.w, rect.y),
        )
    } else {
        (
            Pos2::new(rect.x, rect.y),
            Pos2::new(rect.x + rect.w, rect.y + rect.h),
        )
    };
    if rotation_deg.abs() < 0.01 {
        return (a, b);
    }
    let (cx, cy) = rect.center();
    let rot = |p: Pos2| {
        let rad = rotation_deg.to_radians();
        let (sin, cos) = rad.sin_cos();
        let (dx, dy) = (p.x - cx, p.y - cy);
        Pos2::new(cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    };
    (rot(a), rot(b))
}

pub(crate) fn dist_point_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// Filled brush tip. The core is opaque and the rim fades to clear; a thin
/// `rim` ring marks the diameter.
pub(crate) fn paint_soft_disc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    softness: f32,
    color: Color32,
    rim: Color32,
) {
    let radius = radius.max(1.5);
    let n = 48u32;
    let radial = if softness <= 0.001 { 1u32 } else { 24 };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color);
    for k in 0..=radial {
        let t = k as f32 / radial as f32;
        let dist = radius * t;
        let sample = if softness <= 0.001 {
            color
        } else {
            color.gamma_multiply(vector_ink::tip_coverage(dist, radius, softness))
        };
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(center + egui::vec2(a.cos(), a.sin()) * dist, sample);
        }
    }
    let rings = radial + 1;
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.add_triangle(0, 1 + i, 1 + j);
    }
    for k in 0..rings - 1 {
        let inner = 1 + k * n;
        let outer = inner + n;
        for i in 0..n {
            let j = (i + 1) % n;
            mesh.add_triangle(inner + i, outer + i, outer + j);
            mesh.add_triangle(inner + i, outer + j, inner + j);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.circle_stroke(center, radius, EStroke::new(1.0_f32, rim));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_stepping_tiers_match_photoshop_table() {
        // <10 px → ±1
        assert_eq!(step_width_px(5.0, true), 6.0);
        assert_eq!(step_width_px(5.0, false), 4.0);
        // 10–50 → ±5
        assert_eq!(step_width_px(20.0, true), 25.0);
        assert_eq!(step_width_px(20.0, false), 15.0);
        // 50–100 → ±10
        assert_eq!(step_width_px(60.0, true), 70.0);
        assert_eq!(step_width_px(60.0, false), 50.0);
        // >100 → ±25
        assert_eq!(step_width_px(120.0, true), 145.0);
        assert_eq!(step_width_px(120.0, false), 95.0);
    }

    #[test]
    fn width_stepping_is_reversible_at_tier_boundaries() {
        // Down from a boundary uses the lower tier so up undoes down.
        assert_eq!(step_width_px(10.0, false), 9.0);
        assert_eq!(step_width_px(9.0, true), 10.0);
        assert_eq!(step_width_px(50.0, false), 45.0);
        assert_eq!(step_width_px(45.0, true), 50.0);
        assert_eq!(step_width_px(100.0, false), 90.0);
        assert_eq!(step_width_px(90.0, true), 100.0);
    }

    #[test]
    fn width_stepping_clamps_at_one_pixel() {
        assert_eq!(step_width_px(1.0, false), 1.0);
        assert_eq!(step_width_px(0.5, false), 1.0);
    }

    #[test]
    fn point_segment_distance() {
        let a = Pos2::new(0.0, 0.0);
        let b = Pos2::new(10.0, 0.0);
        assert!((dist_point_segment(Pos2::new(5.0, 3.0), a, b) - 3.0).abs() < 1e-5);
        assert!((dist_point_segment(Pos2::new(-4.0, 0.0), a, b) - 4.0).abs() < 1e-5);
    }

    #[test]
    fn softness_steps_are_quarters_and_the_hud_scrubs_two_pixels_of_diameter() {
        assert_eq!(step_softness(0.0, true), 0.25);
        assert_eq!(step_softness(1.0, true), 1.0);
        assert_eq!(step_softness(0.5, false), 0.25);
        assert_eq!(SIZE_DRAG_GAIN, 2.0);
        assert_eq!(scrub_diameter_px(10.0, 40.0), 90.0);
        assert_eq!(scrub_diameter_px(10.0, -40.0), 1.0);
        assert!((scrub_softness(0.0, -50.0) - 0.5).abs() < 1e-5);
        assert!((scrub_opacity(1.0, 50.0) - 0.5).abs() < 1e-4);
        assert!((scrub_opacity(0.2, -100.0) - 1.0).abs() < 1e-4);
        assert_eq!(scrub_opacity(0.5, 1000.0), 0.0);
        let slot0 = wheel_slot_offset(0, 24, 10.0);
        assert!(slot0[0].abs() < 1e-4);
        assert!((slot0[1] - 10.0).abs() < 1e-4);
        let left = wheel_slot_offset(6, 24, 10.0);
        assert!((left[0] + 10.0).abs() < 1e-3);
        assert!(left[1].abs() < 1e-3);
    }

    #[test]
    fn softness_fills_from_the_center_out() {
        assert_eq!(vector_ink::tip_coverage(19.0, 20.0, 0.0), 1.0);
        assert_eq!(vector_ink::tip_coverage(0.0, 20.0, 1.0), 1.0);
        assert_eq!(vector_ink::tip_coverage(10.0, 20.0, 0.5), 1.0);
        assert!(vector_ink::tip_coverage(15.0, 20.0, 0.5) < 1.0);
        let hard = slate_doc::scene::Stroke {
            width: 40.0,
            softness: 0.0,
            ..Default::default()
        };
        assert_eq!(hard.paint_profile(), (40.0, 0.0));
        let soft = slate_doc::scene::Stroke {
            width: 40.0,
            softness: 1.0,
            ..Default::default()
        };
        let (w, feather) = soft.paint_profile();
        assert!((w - 20.0).abs() < 1e-3);
        assert!((feather - 20.0).abs() < 1e-3);
    }

    #[test]
    fn color_wheel_dot_adopts_that_recent_color() {
        let hit = sample_wheel(
            wheel_slot_offset(0, 24, WHEEL_SLOT_RADIUS),
            [0.0, 1.0, 1.0],
            &[[9, 8, 7], [1, 2, 3]],
        );
        match hit {
            WheelHit::Dot(rgb, _) => assert_eq!(rgb, [9, 8, 7]),
            _ => panic!("expected the 6 o'clock dot"),
        }
        let off = {
            let at = wheel_slot_offset(0, 24, WHEEL_SLOT_RADIUS);
            [at[0] + 7.0, at[1] - 3.0]
        };
        assert!(matches!(
            sample_wheel(off, [0.0, 1.0, 1.0], &[[9, 8, 7]]),
            WheelHit::Dot([9, 8, 7], _)
        ));
        // The hue ring's outer edge and the swatches' inner edge never share
        // a pick: just inside the swatch zone is hue-free, and the ring's
        // padded edge is swatch-free.
        let dot_inner = WHEEL_SLOT_RADIUS - WHEEL_DOT_HIT;
        let ring_outer = WHEEL_HUE_OUTER + WHEEL_HUE_HIT_PAD;
        assert!(dot_inner > ring_outer + 4.0, "{dot_inner} vs {ring_outer}");
        let ring_edge = [0.0, ring_outer - 0.5];
        assert!(matches!(
            sample_wheel(ring_edge, [0.0, 1.0, 1.0], &[[9, 8, 7]]),
            WheelHit::Field(..)
        ));
        let gap = [0.0, (ring_outer + dot_inner) * 0.5];
        assert!(matches!(
            sample_wheel(gap, [0.0, 1.0, 1.0], &[[9, 8, 7]]),
            WheelHit::Keep
        ));
    }

    #[test]
    fn the_wheel_zones_leave_a_dead_strip_between_disk_and_ring() {
        assert!(WHEEL_HUE_INNER - WHEEL_SV_RADIUS >= 6.0);
        let hsv = [0.6, 0.4, 0.7];
        for k in 0..36 {
            let a = k as f32 / 36.0 * std::f32::consts::TAU;
            let at = |r: f32| [a.cos() * r, a.sin() * r];
            assert!(matches!(sample_wheel(at(WHEEL_SV_RADIUS - 0.5), hsv, &[]), WheelHit::Field(..)));
            assert!(matches!(sample_wheel(at(WHEEL_SV_RADIUS + 0.5), hsv, &[]), WheelHit::Keep));
            assert!(matches!(sample_wheel(at(WHEEL_HUE_INNER - 0.5), hsv, &[]), WheelHit::Keep));
            match sample_wheel(at(WHEEL_HUE_INNER + 0.5), hsv, &[]) {
                WheelHit::Field(_, next) => {
                    assert_eq!(next[1..], hsv[1..], "the ring changes hue only")
                }
                other => panic!("ring at {a}: {other:?}"),
            }
            assert!(matches!(sample_wheel(at(WHEEL_BACKDROP_RADIUS + 1.0), hsv, &[]), WheelHit::Outside));
        }
    }

    #[test]
    fn the_wheel_snaps_to_exact_white_and_black() {
        let [(white, _), (black, _)] = wheel_snaps();
        for hue in [0.0, 0.33, 0.8] {
            for nudge in [egui::Vec2::ZERO, egui::vec2(3.0, -2.0), egui::vec2(-4.0, 5.0)] {
                let p = white + nudge;
                match sample_wheel([p.x, p.y], [hue, 0.7, 0.4], &[]) {
                    WheelHit::Field(rgb, _) => assert_eq!(rgb, [255, 255, 255]),
                    other => panic!("white snap: {other:?}"),
                }
                let p = black + nudge;
                if p.length() <= WHEEL_SV_RADIUS {
                    match sample_wheel([p.x, p.y], [hue, 0.7, 0.4], &[]) {
                        WheelHit::Field(rgb, _) => assert_eq!(rgb, [0, 0, 0]),
                        other => panic!("black snap: {other:?}"),
                    }
                }
            }
        }
        assert!(white.length() <= WHEEL_SV_RADIUS + 1e-3);
        assert!(black.length() <= WHEEL_SV_RADIUS + 1e-3);
    }

    #[test]
    fn the_sv_disk_reaches_every_corner_of_the_square() {
        for (s, v) in [(0.0, 1.0), (1.0, 1.0), (0.0, 0.0), (1.0, 0.0), (0.3, 0.6)] {
            let at = sv_offset(s, v);
            assert!(at.length() <= WHEEL_SV_RADIUS + 1e-3, "({s},{v}) at {at:?}");
            let (s2, v2) = sv_at(at);
            assert!((s2 - s).abs() < 1e-3 && (v2 - v).abs() < 1e-3, "({s},{v}) -> ({s2},{v2})");
        }
        // Pure white sits on the rim, reachable without the snap.
        let (s, v) = sv_at(sv_offset(0.0, 1.0));
        assert_eq!(hsv_to_rgb([0.4, s, v]), [255, 255, 255]);
    }
}
