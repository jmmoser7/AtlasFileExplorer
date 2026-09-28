//! Right-button tip HUDs beyond the brush: the open vector drawing tools
//! (Line, Polyline, Arc, Pen, Bezier) and Direct Select.
//!
//! Alt+right-drag sizes, Ctrl+right-drag opens the color wheel, and
//! Shift+right-drag sets opacity, exactly as for the Brush. A drawing tool
//! edits its create style (P1.curve.create-style), so the next curve uses
//! it. On a committed curve (Direct Select's target, or the Select tool's
//! one selected curve) the HUD edits the picked vertices, else the hovered
//! one, else the whole curve, live, and journals one Patch when the HUD
//! closes. Vertex edits go through the property owner
//! (`Property::apply_at`, P1.curve.vertex-style). Whole-curve edits are
//! relative (user, 28 September 2026, ed1): width scales every tip, the
//! color shifts every tip's hue, saturation and value, and opacity scales
//! the node's. The property strip stays absolute.
//! Softness stays brush-only: vector strokes are not stamped, so there is
//! nothing to blur.
//!
//! A chord released without travel opens that HUD's numeric entry instead
//! (`TipNumeric`): type size, softness, opacity, or hue/saturation/value,
//! applied through the same setters the drag uses.

use super::board::BoardTool;
use super::board_properties::{picked_tip, Property};
use super::SlateApp;
use slate_doc::scene::{Node, NodeKind, Rgba, SceneCmd, ShapeKind};
use slate_doc::NodeId;

/// The color a whole curve reads as: its first painted tip, else its stroke.
fn curve_color(node: &Node) -> Option<Rgba> {
    let NodeKind::Shape(s) = &node.kind else {
        return None;
    };
    Some(picked_tip(node, &[0]).map_or(s.stroke.color, |t| t.color))
}

/// Open vector drawing tools that take the tip HUD.
pub(crate) fn curve_tool(tool: BoardTool) -> bool {
    matches!(
        tool,
        BoardTool::Line
            | BoardTool::Polyline
            | BoardTool::Arc
            | BoardTool::Pen
            | BoardTool::BezierSpan
    )
}

impl SlateApp {
    /// The committed curve the tip HUD edits, and which of its vertices
    /// (grip indices, P1.curve.grips): the picked ones (Direct Select
    /// anchors or Select-tool grips), else the one under the pointer, else
    /// none, which is the whole curve. Direct Select arms on its target.
    /// The Select tool arms on a picked or hovered vertex, and on its one
    /// selected curve while the right button is held (user, 28 September
    /// 2026, ed1), so a plain selection does not turn every tip readout and
    /// key onto the curve. An open HUD keeps the target it opened on.
    pub(crate) fn hud_target(&self) -> Option<(NodeId, Vec<usize>)> {
        if self.hud_frozen.is_some() {
            return self.hud_frozen.clone();
        }
        let hovered = || self.hud_pointer.and_then(|p| self.hovered_vertex(p));
        let (id, points) = match self.board_tool {
            BoardTool::DirectSelect => {
                let id = self.direct.node?;
                let points = match self.picked_vertices() {
                    Some((_, picked)) => picked,
                    None => hovered().map(|(_, i)| vec![i]).unwrap_or_default(),
                };
                (id, points)
            }
            BoardTool::Select => match self.picked_vertices() {
                Some(picked) => picked,
                None => match hovered() {
                    Some((id, i)) => (id, vec![i]),
                    None if self.hud_right_held && self.board_sel.len() == 1 => {
                        (*self.board_sel.iter().next()?, Vec::new())
                    }
                    None => return None,
                },
            },
            _ => return None,
        };
        let node = self.doc().scene.node(id)?;
        match &node.kind {
            NodeKind::Shape(s)
                if (matches!(s.shape, ShapeKind::Path | ShapeKind::Line)
                    || slate_doc::vertex_style::closed_form_vertex_count(s).is_some())
                    && !node.locked =>
            {
                Some((id, points))
            }
            _ => None,
        }
    }

    /// The curve [`Self::hud_target`] edits.
    pub(crate) fn hud_node(&self) -> Option<NodeId> {
        self.hud_target().map(|(id, _)| id)
    }

    /// Fix the HUD's target and snapshot it for one journaled Patch.
    pub(crate) fn begin_hud_node(&mut self) {
        self.hud_frozen = None;
        self.hud_frozen = self.hud_target();
        self.hud_node_before = self.hud_node_snapshot();
    }

    /// Apply property edits to the HUD target: at its vertices when some
    /// are picked or hovered, else to the whole curve (`Property::apply_at`).
    fn edit_hud_node(&mut self, edits: &[Property]) -> bool {
        let Some((id, points)) = self.hud_target() else {
            return false;
        };
        if let Some(n) = self.doc_mut().scene.node_mut(id) {
            for edit in edits {
                edit.apply_at(n, None, &points);
            }
        }
        true
    }

    /// The painted tip at the first HUD vertex, when the HUD edits vertices.
    fn hud_vertex_tip(&self) -> Option<slate_doc::scene::StrokeSpan> {
        let (id, points) = self.hud_target()?;
        picked_tip(self.doc().scene.node(id)?, &points)
    }

    /// Any tool whose right-button chords open the tip HUD.
    pub(crate) fn tip_hud_armed(&self) -> bool {
        matches!(
            self.board_tool,
            BoardTool::Brush | BoardTool::Eraser | BoardTool::Smooth
        ) || curve_tool(self.board_tool)
            || self.hud_node().is_some()
    }

    /// Ctrl+right-drag opens the color wheel for tools that paint a color.
    pub(crate) fn tip_hud_has_color(&self) -> bool {
        self.board_tool == BoardTool::Brush
            || curve_tool(self.board_tool)
            || self.hud_node().is_some()
    }

    /// Softness applies to stamped ink only.
    pub(crate) fn tip_hud_has_softness(&self) -> bool {
        match self.board_tool {
            BoardTool::Brush | BoardTool::Eraser | BoardTool::Smooth => true,
            _ => self.hud_node().is_some_and(|id| {
                matches!(
                    self.doc().scene.node(id).map(|n| &n.kind),
                    Some(NodeKind::Shape(s)) if s.stroke.paints_as_stamp()
                )
            }),
        }
    }

    /// Width, softness, opacity for a curve tool or the HUD target. On
    /// vertices: the first one's painted width, its softness on a stamped
    /// stroke, and the opacity it paints at (node opacity × its alpha). On
    /// the whole curve: a stamped stroke's widest and softest tip.
    pub(crate) fn vector_tip(&self) -> Option<(f32, f32, f32)> {
        if curve_tool(self.board_tool) {
            let s = self.stroke_for_new_curve();
            return Some((s.width, 0.0, self.opacity_for_new_node(false)));
        }
        let node = self.doc().scene.node(self.hud_node()?)?;
        let NodeKind::Shape(s) = &node.kind else {
            return None;
        };
        if let Some(tip) = self.hud_vertex_tip() {
            let softness = if s.stroke.paints_as_stamp() {
                tip.softness
            } else {
                0.0
            };
            let opacity = node.opacity * tip.color.0[3] as f32 / 255.0;
            return Some((tip.width, softness, opacity));
        }
        let tip = slate_doc::vertex_style::curve_tip(s.path.as_deref(), &s.stroke);
        Some((tip.width, tip.softness, node.opacity))
    }

    /// Write a curve tool's create style, or the HUD target live (journaled
    /// when the HUD closes). Only what changed is written, so a size scrub
    /// keeps each vertex's opacity and an opacity scrub keeps each width.
    /// Returns false for other tools.
    pub(crate) fn set_vector_tip(&mut self, width: f32, softness: f32, opacity: f32) -> bool {
        if curve_tool(self.board_tool) {
            let mut s = self.stroke_for_new_curve();
            s.width = width;
            self.store_armed_curve_stroke(s, Some(opacity.clamp(0.0, 1.0)));
            return true;
        }
        let Some((id, points)) = self.hud_target() else {
            return false;
        };
        let Some((w0, s0, o0)) = self.vector_tip() else {
            return false;
        };
        let changed = |a: f32, b: f32| (a - b).abs() > 1e-4 * a.abs().max(1.0);
        let stamped = self.tip_hud_has_softness();
        let (resize, soften) = (changed(width, w0), stamped && changed(softness, s0));
        let fade = changed(opacity, o0);
        let base = self.hud_node_before.clone().filter(|b| b.id == id);
        let vertices = self.hud_vertex_tip().is_some();
        let Some(n) = self.doc_mut().scene.node_mut(id) else {
            return false;
        };
        if vertices {
            let softness = softness.clamp(0.0, 1.0);
            if resize || soften {
                super::board_properties::edit_vertex_tips(n, &points, |t| {
                    if resize {
                        t.width = width.max(0.0);
                    }
                    if soften {
                        t.softness = softness;
                    }
                });
            }
            if fade {
                super::board_properties::set_vertex_opacity(n, &points, opacity);
            }
            return true;
        }
        if fade {
            n.opacity = opacity.clamp(0.0, 1.0);
        }
        if (resize || soften)
            && !super::board_properties::scale_stamped_curve(
                n,
                base.as_ref(),
                width,
                Some(softness),
            )
        {
            if let NodeKind::Shape(s) = &mut n.kind {
                s.stroke.width = width;
                if stamped {
                    s.stroke.softness = softness;
                }
            }
        }
        true
    }

    /// The color the armed tool paints with (straight RGBA).
    pub(crate) fn active_rgba(&self) -> [u8; 4] {
        if curve_tool(self.board_tool) {
            return self.stroke_for_new_curve().color.0;
        }
        if let Some(tip) = self.hud_vertex_tip() {
            return tip.color.0;
        }
        if let Some(node) = self.hud_node().and_then(|id| self.doc().scene.node(id)) {
            if let Some(c) = curve_color(node) {
                return c.0;
            }
        }
        self.board_colors.fg.0
    }

    /// Set the armed tool's color. A curve tool keeps its own color and
    /// never moves another tool's (P1.curve.create-style).
    pub(crate) fn set_active_rgba(&mut self, rgba: [u8; 4]) {
        if curve_tool(self.board_tool) {
            let mut s = self.stroke_for_new_curve();
            s.color = Rgba(rgba);
            self.store_armed_curve_stroke(s, None);
            return;
        }
        let before = self.active_rgba();
        let rgb = [rgba[0], rgba[1], rgba[2]];
        let whole = self.hud_target().filter(|(_, points)| points.is_empty());
        let mut edits = Vec::new();
        if let Some((id, _)) = whole {
            self.shift_curve_color(id, rgb);
        } else {
            edits.push(Property::StrokeRgb(rgb));
        }
        if rgba[3] != before[3] {
            edits.push(Property::StrokeAlpha(rgba[3]));
        }
        if self.edit_hud_node(&edits) {
            return;
        }
        self.board_colors.fg.0 = rgba;
    }

    /// Whole-curve color (user, 28 September 2026, ed1): the change from
    /// the curve's color when the HUD opened to `rgb` shifts every tip and
    /// the stroke color from where each was (`shift_hsv`), so a gradient
    /// stays a gradient. A one-color curve lands on `rgb`.
    fn shift_curve_color(&mut self, id: NodeId, rgb: [u8; 3]) {
        let base = self
            .hud_node_before
            .clone()
            .filter(|b| b.id == id)
            .or_else(|| self.doc().scene.node(id).cloned());
        let Some(base) = base else {
            return;
        };
        let (Some(from), NodeKind::Shape(bs)) = (curve_color(&base), &base.kind) else {
            return;
        };
        let from = [from.0[0], from.0[1], from.0[2]];
        let shift = |base: Rgba, out: &mut Rgba| {
            let c = super::board_color::shift_hsv([base.0[0], base.0[1], base.0[2]], from, rgb);
            out.0[..3].copy_from_slice(&c);
        };
        let Some(n) = self.doc_mut().scene.node_mut(id) else {
            return;
        };
        let NodeKind::Shape(s) = &mut n.kind else {
            return;
        };
        shift(bs.stroke.color, &mut s.stroke.color);
        if let (Some(b), Some(t)) = (bs.stroke.tween_from, s.stroke.tween_from.as_mut()) {
            shift(b.color, &mut t.color);
        }
        if let (Some(bp), Some(p)) = (bs.path.as_ref(), s.path.as_mut()) {
            if bp.tips.len() == p.tips.len() && !p.tips.is_empty() {
                let p = std::sync::Arc::make_mut(p);
                for (b, t) in bp.tips.iter().zip(&mut p.tips) {
                    shift(b.color, &mut t.color);
                }
            }
        }
    }

    pub(crate) fn set_active_rgb(&mut self, rgb: [u8; 3]) {
        let mut c = self.active_rgba();
        c[..3].copy_from_slice(&rgb);
        self.set_active_rgba(c);
    }

    /// Snapshot of the HUD target, taken when a HUD or key edit starts.
    pub(crate) fn hud_node_snapshot(&self) -> Option<Node> {
        self.doc().scene.node(self.hud_node()?).cloned()
    }

    /// Journal the HUD target's edit as one Patch.
    pub(crate) fn journal_hud_node(&mut self, before: Option<Node>) {
        self.hud_frozen = None;
        let Some(before) = before else {
            return;
        };
        let Some(after) = self.doc().scene.node(before.id).cloned() else {
            return;
        };
        if after == before {
            return;
        }
        // Put the node back so the journal applies the change itself.
        if let Some(n) = self.doc_mut().scene.node_mut(before.id) {
            *n = before.clone();
        }
        self.last_board_edit = None;
        self.commit_scene(vec![SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        }]);
    }

    /// Esc: put the HUD target back.
    pub(crate) fn restore_hud_node(&mut self, before: Option<Node>) {
        self.hud_frozen = None;
        let Some(before) = before else {
            return;
        };
        if let Some(n) = self.doc_mut().scene.node_mut(before.id) {
            *n = before;
        }
    }
}

// ---------- tip style palette (under the Alt+right size circle) ----------

use eframe::egui::{self, Color32, Pos2, Stroke as EStroke};
use slate_doc::scene::{BrushTexture, Stroke, StrokeCap, StrokeEnd, StrokeJoin, WidthProfile};

/// End and corner treatment of an open vector curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CurveStyle {
    /// Flat ends, square corners.
    Square,
    /// Round ends, round corners.
    Round,
    /// An arrowhead on the last point.
    Arrow,
    /// Narrow at the first point, full at the last.
    TaperStart,
    /// Narrow at both ends.
    TaperBoth,
}

/// The condition of one picked end of an open curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndStyle {
    Flat,
    Round,
    Arrow,
    Narrow,
}

impl EndStyle {
    fn of(end: StrokeEnd) -> EndStyle {
        if end.arrow {
            EndStyle::Arrow
        } else if end.narrow {
            EndStyle::Narrow
        } else if end.cap == StrokeCap::Round {
            EndStyle::Round
        } else {
            EndStyle::Flat
        }
    }

    /// The end this style sets. A narrow end is round, as the whole-curve
    /// narrow styles are; an arrowed end is flat under its head.
    fn end(self) -> StrokeEnd {
        let (cap, arrow, narrow) = match self {
            EndStyle::Flat => (StrokeCap::Butt, false, false),
            EndStyle::Round => (StrokeCap::Round, false, false),
            EndStyle::Arrow => (StrokeCap::Butt, true, false),
            EndStyle::Narrow => (StrokeCap::Round, false, true),
        };
        StrokeEnd { cap, arrow, narrow }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TipChoice {
    Texture(BrushTexture),
    Curve(CurveStyle),
    /// The condition of the picked end(s) only.
    End(EndStyle),
}

const TEXTURE_CHOICES: [TipChoice; 5] = [
    TipChoice::Texture(BrushTexture::Smooth),
    TipChoice::Texture(BrushTexture::Graphite),
    TipChoice::Texture(BrushTexture::Pencil),
    TipChoice::Texture(BrushTexture::Ink),
    TipChoice::Texture(BrushTexture::Watercolor),
];

const CURVE_CHOICES: [TipChoice; 5] = [
    TipChoice::Curve(CurveStyle::Square),
    TipChoice::Curve(CurveStyle::Round),
    TipChoice::Curve(CurveStyle::Arrow),
    TipChoice::Curve(CurveStyle::TaperStart),
    TipChoice::Curve(CurveStyle::TaperBoth),
];

const END_CHOICES: [TipChoice; 4] = [
    TipChoice::End(EndStyle::Flat),
    TipChoice::End(EndStyle::Round),
    TipChoice::End(EndStyle::Arrow),
    TipChoice::End(EndStyle::Narrow),
];

/// Screen px between the size circle's bottom and the palette row's centers.
const PALETTE_GAP: f32 = 36.0;
const PALETTE_SPACING: f32 = 50.0;
const PALETTE_ICON_R: f32 = 21.0;
const PALETTE_HIT_R: f32 = 24.0;
/// Width of the preview stroke inside a swatch, screen px.
const SWATCH_STROKE: f32 = 11.0;
/// How narrow a taper gets at its thin end.
const TAPER_TIP: f32 = slate_doc::scene::NARROW_TIP;

/// The whole-curve style `stroke` has, or `None` when its two ends were
/// set apart (P1.curve.vertex-style end conditions).
pub(crate) fn curve_style_of(stroke: &Stroke) -> Option<CurveStyle> {
    if stroke.arrow_start || stroke.cap_start.is_some() || stroke.cap_end.is_some() {
        return None;
    }
    if stroke.arrow_end {
        return Some(CurveStyle::Arrow);
    }
    Some(match stroke.profile.narrow_ends() {
        [true, false] => CurveStyle::TaperStart,
        [true, true] => CurveStyle::TaperBoth,
        [false, true] => return None,
        [false, false] if stroke.cap == StrokeCap::Round => CurveStyle::Round,
        [false, false] => CurveStyle::Square,
    })
}

pub(crate) fn apply_curve_style(stroke: &mut Stroke, style: CurveStyle) {
    let round = |s: &mut Stroke| {
        s.cap = StrokeCap::Round;
        s.join = StrokeJoin::Round;
    };
    stroke.arrow_end = false;
    stroke.arrow_start = false;
    stroke.cap_start = None;
    stroke.cap_end = None;
    stroke.profile = WidthProfile::Uniform;
    match style {
        CurveStyle::Square => {
            stroke.cap = StrokeCap::Butt;
            stroke.join = StrokeJoin::Miter;
        }
        CurveStyle::Round => round(stroke),
        CurveStyle::Arrow => {
            stroke.cap = StrokeCap::Butt;
            stroke.join = StrokeJoin::Round;
            stroke.arrow_end = true;
        }
        CurveStyle::TaperStart => {
            round(stroke);
            stroke.profile = WidthProfile::Taper {
                start: TAPER_TIP,
                end: 1.0,
            };
        }
        CurveStyle::TaperBoth => {
            round(stroke);
            stroke.profile = WidthProfile::Ends { tip: TAPER_TIP };
        }
    }
}

/// Center of palette slot `i` of `n`, under a circle of radius `r` at `o`.
pub(crate) fn palette_slot(o: Pos2, r: f32, i: usize, n: usize) -> Pos2 {
    let x = (i as f32 - (n as f32 - 1.0) * 0.5) * PALETTE_SPACING;
    Pos2::new(o.x + x, o.y + r + PALETTE_GAP)
}

/// Palette slot under `pointer`, if any.
pub(crate) fn palette_hit(o: Pos2, r: f32, n: usize, pointer: Pos2) -> Option<usize> {
    (0..n).find(|&i| palette_slot(o, r, i, n).distance(pointer) <= PALETTE_HIT_R)
}

/// Top edge of the style band under a size circle of radius `r` at `o`.
pub(crate) fn palette_band_y(o: Pos2, r: f32) -> f32 {
    o.y + r + PALETTE_ZONE_TOP
}

/// The full-width band under the size circle where the style row lives.
/// While the pointer is anywhere in it, size and softness hold still, so the
/// row never moves under the pointer.
pub(crate) fn palette_zone(o: Pos2, r: f32, n: usize, pointer: Pos2) -> bool {
    n > 0 && pointer.y >= palette_band_y(o, r)
}

/// The style band starts this far under the circle's rim.
const PALETTE_ZONE_TOP: f32 = 8.0;

impl SlateApp {
    /// Which ends of the HUD's open curve its picked (or hovered) grips
    /// are, `[first point, last point]`, when at least one is: the row then
    /// sets those ends' conditions only (user, 28 September 2026, tl5).
    pub(crate) fn hud_end_picks(&self) -> Option<[bool; 2]> {
        let (id, points) = self.hud_target()?;
        let node = self.doc().scene.node(id)?;
        let NodeKind::Shape(s) = &node.kind else {
            return None;
        };
        let open = s
            .path
            .as_ref()
            .is_some_and(|p| !p.closed && p.extra.is_empty());
        if s.shape != ShapeKind::Path || !open || s.stroke.paints_as_stamp() {
            return None;
        }
        let grips = if super::board_line::line_endpoints(node).is_some() {
            2
        } else {
            match self.curve_grips_of(id)? {
                super::board_direct::CurveGrips::Arc(_) => 3,
                super::board_direct::CurveGrips::Anchors { anchors, .. } => anchors.len(),
            }
        };
        let last = grips.checked_sub(1).filter(|l| *l > 0)?;
        let ends = [points.contains(&0), points.contains(&last)];
        ends.contains(&true).then_some(ends)
    }

    /// The style choices the armed tool offers under its size circle.
    pub(crate) fn tip_choices(&self) -> &'static [TipChoice] {
        match self.board_tool {
            BoardTool::Brush | BoardTool::Eraser => &TEXTURE_CHOICES,
            _ if curve_tool(self.board_tool) => &CURVE_CHOICES,
            _ if self.hud_node().is_some() => {
                if self.tip_hud_has_softness() {
                    &TEXTURE_CHOICES
                } else if self.hud_end_picks().is_some() {
                    &END_CHOICES
                } else {
                    &CURVE_CHOICES
                }
            }
            _ => &[],
        }
    }

    fn hud_node_stroke(&self) -> Option<Stroke> {
        let node = self.doc().scene.node(self.hud_node()?)?;
        match &node.kind {
            NodeKind::Shape(s) => Some(s.stroke),
            _ => None,
        }
    }

    pub(crate) fn current_tip_choice(&self) -> Option<TipChoice> {
        match self.board_tool {
            BoardTool::Brush => Some(TipChoice::Texture(self.brush_texture)),
            BoardTool::Eraser => Some(TipChoice::Texture(self.eraser_texture)),
            _ if curve_tool(self.board_tool) => {
                curve_style_of(&self.stroke_for_new_curve()).map(TipChoice::Curve)
            }
            _ => {
                let s = self.hud_node_stroke()?;
                if s.paints_as_stamp() {
                    return Some(TipChoice::Texture(s.texture));
                }
                let Some(picked) = self.hud_end_picks() else {
                    return curve_style_of(&s).map(TipChoice::Curve);
                };
                let mut styles = (0..2)
                    .filter(|&i| picked[i])
                    .map(|i| EndStyle::of(s.end(i)));
                let first = styles.next()?;
                styles.all(|e| e == first).then_some(TipChoice::End(first))
            }
        }
    }

    /// Apply a palette choice to the armed tool (a committed curve: live on
    /// the whole curve, or on its picked ends for an end choice; journaled
    /// when the HUD closes).
    pub(crate) fn apply_tip_choice(&mut self, choice: TipChoice) {
        let picked = self.hud_end_picks();
        match (self.board_tool, choice) {
            (BoardTool::Brush, TipChoice::Texture(t)) => {
                self.brush_texture = t;
                self.settings.brush_texture = t;
            }
            (BoardTool::Eraser, TipChoice::Texture(t)) => {
                self.eraser_texture = t;
                self.settings.eraser_texture = t;
            }
            (tool, TipChoice::Curve(style)) if curve_tool(tool) => {
                let mut s = self.stroke_for_new_curve();
                apply_curve_style(&mut s, style);
                self.store_armed_curve_stroke(s, None);
            }
            (_, choice) => {
                let Some(id) = self.hud_node() else {
                    return;
                };
                if let Some(n) = self.doc_mut().scene.node_mut(id) {
                    if let NodeKind::Shape(s) = &mut n.kind {
                        match choice {
                            TipChoice::Texture(t) => s.stroke.texture = t,
                            TipChoice::Curve(style) => apply_curve_style(&mut s.stroke, style),
                            TipChoice::End(style) => {
                                let picked = picked.unwrap_or_default();
                                for i in (0..2).filter(|&i| picked[i]) {
                                    s.stroke.set_end(i, style.end());
                                }
                            }
                        }
                    }
                    if let TipChoice::Texture(t) = choice {
                        super::board_properties::edit_every_tip(n, |tip| tip.texture = t);
                    }
                }
            }
        }
    }

    /// The palette row under the size circle. Transient input chrome opened
    /// at the right-button press point for the life of the drag, so icons
    /// stay screen-sized (P0.9 pointer-attached exception).
    pub(crate) fn paint_tip_palette(
        &self,
        painter: &egui::Painter,
        o: Pos2,
        r: f32,
        pointer: Pos2,
        accent: Color32,
    ) {
        let choices = self.tip_choices();
        if choices.is_empty() {
            return;
        }
        let current = self.current_tip_choice();
        let both = self.hud_end_picks() == Some([true, true]);
        let hovered = palette_hit(o, r, choices.len(), pointer);
        // Full-strength ink so a faint tip still shows its style.
        let ink = if self.board_tool == BoardTool::Eraser {
            self.eraser_preview_color().to_opaque()
        } else {
            let c = self.active_rgba();
            Color32::from_rgb(c[0], c[1], c[2])
        };
        for (i, choice) in choices.iter().enumerate() {
            let at = palette_slot(o, r, i, choices.len());
            let selected = current == Some(*choice);
            painter.circle_filled(
                at,
                PALETTE_ICON_R,
                Color32::from_rgba_unmultiplied(18, 18, 20, 235),
            );
            let ring = if hovered == Some(i) {
                EStroke::new(2.5_f32, accent)
            } else if selected {
                EStroke::new(2.0_f32, accent.gamma_multiply(0.8))
            } else {
                EStroke::new(1.0_f32, Color32::from_gray(110))
            };
            paint_choice_glyph(painter, at, *choice, both, ink);
            painter.circle_stroke(at, PALETTE_ICON_R, ring);
        }
        let labelled = hovered.or_else(|| choices.iter().position(|c| current == Some(*c)));
        if let Some(i) = labelled {
            let at = palette_slot(o, r, i, choices.len());
            painter.text(
                at + egui::vec2(0.0, PALETTE_ICON_R + 5.0),
                egui::Align2::CENTER_TOP,
                choice_label(choices[i], both),
                egui::FontId::proportional(13.0),
                Color32::WHITE,
            );
        }
    }
}

/// A short stroke of `texture`, stamped once in white at three raster
/// pixels per screen px and kept in egui memory, so the row does no
/// rasterizing per frame. The rect is relative to the swatch center.
fn texture_swatch(ctx: &egui::Context, texture: BrushTexture) -> Option<(egui::TextureId, egui::Rect)> {
    type Swatch = (egui::TextureHandle, egui::Rect);
    let id = egui::Id::new(("slate.tip_swatch", texture.label()));
    if let Some((tex, rect)) = ctx.data(|d| d.get_temp::<Swatch>(id)) {
        return Some((tex.id(), rect));
    }
    let tip = vector_ink::StampStyle {
        diameter: SWATCH_STROKE,
        softness: 0.15,
        rgba: [255; 4],
        grain: texture.grain(),
    };
    let reach = PALETTE_ICON_R - SWATCH_STROKE * 0.5 - 3.0;
    let points: Vec<vector_ink::TipPoint> = (0..=32)
        .map(|k| {
            let t = k as f32 / 32.0;
            let x = (t - 0.5) * 2.0 * reach;
            let y = -(t * std::f32::consts::TAU).sin() * reach * 0.3;
            vector_ink::TipPoint { pos: [x, y], tip }
        })
        .collect();
    let img = vector_ink::stamp_tipped(&[points], 1.0 / 3.0)?;
    let pixels = img
        .rgba
        .chunks_exact(4)
        .map(|p| Color32::from_white_alpha(p[3]))
        .collect();
    let image = egui::ColorImage {
        size: [img.width as usize, img.height as usize],
        pixels,
    };
    let tex = ctx.load_texture(
        format!("tip-swatch-{}", texture.label()),
        image,
        egui::TextureOptions::LINEAR,
    );
    let rect = egui::Rect::from_min_size(
        Pos2::new(img.origin[0], img.origin[1]),
        egui::vec2(img.width as f32 * img.pixel, img.height as f32 * img.pixel),
    );
    ctx.data_mut(|d| d.insert_temp(id, (tex.clone(), rect)));
    Some((tex.id(), rect))
}

/// The curve `style` really produces, on a small bent path around the
/// swatch center: the same stroke style, taper, and arrowhead the board
/// paints. Tessellated once and kept in egui memory.
fn curve_glyph(ctx: &egui::Context, choice: TipChoice, both: bool) -> vector_ink::InkMesh {
    let key = match choice {
        TipChoice::End(style) => 16 + style as u8 * 2 + u8::from(both),
        TipChoice::Curve(style) => style as u8,
        TipChoice::Texture(_) => 255,
    };
    let id = egui::Id::new(("slate.tip_curve_glyph", key));
    if let Some(ink) = ctx.data(|d| d.get_temp::<vector_ink::InkMesh>(id)) {
        return ink;
    }
    let mut stroke = Stroke {
        width: 4.0,
        ..Stroke::default()
    };
    match choice {
        TipChoice::Curve(style) => apply_curve_style(&mut stroke, style),
        // The end being set is the glyph's right end; the left stays flat.
        TipChoice::End(style) => {
            let ends: &[usize] = if both { &[0, 1] } else { &[1] };
            for &i in ends {
                stroke.set_end(i, style.end());
            }
        }
        TipChoice::Texture(_) => {}
    }
    let mut bez = vector_ink::kurbo::BezPath::new();
    bez.move_to((-13.0, 7.0));
    bez.line_to((-2.0, -6.0));
    bez.line_to((14.0, 4.0));
    let ink_style = super::board_path::stroke_style_world(&stroke, 1.0);
    let feather = 0.8;
    let arrows = stroke.arrows();
    let body = slate_doc::geom::trim_arrow_ends(&bez, arrows, [stroke.width; 2]);
    let ends = stroke.end_caps().map(super::board_path::ink_cap);
    let mut ink = vector_ink::stroke_mesh_ends(
        &body,
        &ink_style,
        ends,
        None,
        None,
        vector_ink::TipEase::Linear,
        feather,
        0.05,
    );
    for end in (0..2).filter(|&i| arrows[i]) {
        super::board_path::push_arrow_ink(&mut ink, &bez, end, stroke.width, feather, None);
    }
    ctx.data_mut(|d| d.insert_temp(id, ink.clone()));
    ink
}

pub(crate) fn choice_label(choice: TipChoice, both: bool) -> &'static str {
    match choice {
        TipChoice::Texture(t) => t.label(),
        TipChoice::Curve(CurveStyle::Square) => "Flat ends, square corners",
        TipChoice::Curve(CurveStyle::Round) => "Round ends, round corners",
        TipChoice::Curve(CurveStyle::Arrow) => "Arrow at the end",
        TipChoice::Curve(CurveStyle::TaperStart) => "Narrow at the start",
        TipChoice::Curve(CurveStyle::TaperBoth) => "Narrow at both ends",
        TipChoice::End(style) => match (style, both) {
            (EndStyle::Flat, false) => "Flat end here",
            (EndStyle::Round, false) => "Round end here",
            (EndStyle::Arrow, false) => "Arrow here",
            (EndStyle::Narrow, false) => "Narrow here",
            (EndStyle::Flat, true) => "Flat at both ends",
            (EndStyle::Round, true) => "Round at both ends",
            (EndStyle::Arrow, true) => "Arrows at both ends",
            (EndStyle::Narrow, true) => "Narrow at both ends",
        },
    }
}

/// Each choice drawn as what it produces, in the tool's color.
fn paint_choice_glyph(
    painter: &egui::Painter,
    at: Pos2,
    choice: TipChoice,
    both: bool,
    ink: Color32,
) {
    match choice {
        TipChoice::Texture(texture) => {
            if let Some((tex, rect)) = texture_swatch(painter.ctx(), texture) {
                painter.image(
                    tex,
                    rect.translate(at.to_vec2()),
                    egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    ink,
                );
            }
        }
        TipChoice::Curve(_) | TipChoice::End(_) => {
            let ink_mesh = curve_glyph(painter.ctx(), choice, both);
            let mut mesh = egui::Mesh::default();
            mesh.vertices.reserve(ink_mesh.vertices.len());
            for v in &ink_mesh.vertices {
                mesh.colored_vertex(
                    at + egui::vec2(v.pos[0], v.pos[1]),
                    ink.gamma_multiply(v.alpha),
                );
            }
            mesh.indices = ink_mesh.indices;
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

// ---------- numeric entry (a tip chord released without travel) ----------

use super::board::{BoardDrag, BoardXf};
use super::board_color::{hsv_to_rgb, rgb_to_hsv, BrushHud, WHEEL_BACKDROP_RADIUS};
use atlas_shell::selection_tools as chrome;

/// Longest right press that still reads as a quick click (egui's click
/// duration). Travel is `draft.drag_threshold`.
pub(crate) const QUICK_CLICK_SECONDS: f64 = 0.8;
/// Panel width, row height and inset, in screen px (P2.GhostFollow).
const PANEL_WIDTH: f32 = 150.0;
const PANEL_ROW: f32 = 24.0;
const PANEL_PAD: f32 = 6.0;
const PANEL_GAP: f32 = 8.0;

/// One quantity the numeric panel types, in the units it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TipMetric {
    /// Board units.
    Size,
    /// Percent.
    Softness,
    /// Percent: opacity, or strength for the Eraser and Smooth.
    Opacity,
    /// Degrees.
    Hue,
    /// Percent.
    Saturation,
    /// Percent.
    Value,
}

impl TipMetric {
    fn range(self) -> std::ops::RangeInclusive<f32> {
        match self {
            TipMetric::Size => {
                super::settings::STROKE_WIDTH_MIN..=super::settings::STROKE_WIDTH_MAX
            }
            TipMetric::Hue => 0.0..=360.0,
            _ => 0.0..=100.0,
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            TipMetric::Size => " u",
            TipMetric::Hue => "°",
            _ => " %",
        }
    }
}

/// Numeric entry for the tip HUD: the right press it may open from, the
/// open panel, and the press that closed it.
#[derive(Default)]
pub(crate) struct TipNumeric {
    /// The HUD's right press (screen), when it landed (seconds), and the
    /// farthest the pointer has travelled from it.
    press: Option<(Pos2, f64, f32)>,
    /// The open panel: the metric the keyboard edits, and its text.
    panel: Option<(usize, Option<chrome::NumberEdit>)>,
    /// A press away from the panel closed it: the board ignores that press
    /// until every button is up.
    eat: bool,
}

impl TipNumeric {
    /// The HUD closed (commit or cancel).
    pub(crate) fn close(&mut self) {
        self.press = None;
        self.panel = None;
    }
}

impl SlateApp {
    pub(crate) fn tip_numeric_open(&self) -> bool {
        self.tip_numeric.panel.is_some()
    }

    /// A press that closed the panel still belongs to it.
    pub(crate) fn tip_numeric_eats(&self) -> bool {
        self.tip_numeric.eat
    }

    /// The HUD opened on a right press at `at`.
    pub(crate) fn note_hud_press(&mut self, at: Pos2) {
        self.tip_numeric.press = Some((at, self.frame_time, 0.0));
    }

    /// The held HUD saw the pointer at `p`.
    pub(crate) fn note_hud_travel(&mut self, p: Pos2) {
        if let Some((at, _, travel)) = &mut self.tip_numeric.press {
            *travel = travel.max(at.distance(p));
        }
    }

    /// The HUD's right button came up as a quick click: short, within the
    /// draft threshold, and not pausing a freehand stroke (that release
    /// resumes the stroke instead).
    pub(crate) fn hud_quick_click(&self) -> bool {
        let Some((_, t0, travel)) = self.tip_numeric.press else {
            return false;
        };
        let freehand = matches!(
            self.board_drag,
            Some(BoardDrag::FreehandPen { .. } | BoardDrag::FreehandBrush { .. })
        );
        travel <= super::board_place::place_tokens::DRAG_THRESHOLD
            && self.frame_time - t0 <= QUICK_CLICK_SECONDS
            && !freehand
            && !self.tip_metrics().is_empty()
    }

    /// Turn the open HUD into numeric entry on its first metric.
    pub(crate) fn open_tip_numeric(&mut self) {
        self.tip_numeric.press = None;
        let Some(&first) = self.tip_metrics().first() else {
            return;
        };
        let edit = chrome::NumberEdit::new(self.tip_metric_shown(first));
        self.tip_numeric.panel = Some((0, Some(edit)));
    }

    /// What the open HUD edits, in panel order: Alt size (and softness on
    /// stamped ink), Shift opacity or strength, Ctrl hue, saturation, value.
    pub(crate) fn tip_metrics(&self) -> &'static [TipMetric] {
        use TipMetric::*;
        match self.brush_hud {
            Some(BrushHud::Size { .. }) if self.tip_hud_has_softness() => &[Size, Softness],
            Some(BrushHud::Size { .. }) => &[Size],
            Some(BrushHud::Opacity { .. }) => &[Opacity],
            Some(BrushHud::Wheel { .. }) => &[Hue, Saturation, Value],
            None => &[],
        }
    }

    fn tip_metric_label(&self, metric: TipMetric) -> &'static str {
        match metric {
            TipMetric::Size => "Size",
            TipMetric::Softness => "Softness",
            TipMetric::Opacity
                if matches!(self.board_tool, BoardTool::Eraser | BoardTool::Smooth) =>
            {
                "Strength"
            }
            TipMetric::Opacity => "Opacity",
            TipMetric::Hue => "Hue",
            TipMetric::Saturation => "Saturation",
            TipMetric::Value => "Value",
        }
    }

    /// The metric's current value as the panel shows it (rounded, so an
    /// untouched field applies nothing).
    fn tip_metric_shown(&self, metric: TipMetric) -> f32 {
        let (width, softness, opacity) = self.active_tip();
        let hsv = match self.brush_hud {
            Some(BrushHud::Wheel { hsv, .. }) => hsv,
            _ => {
                let c = self.active_rgba();
                rgb_to_hsv([c[0], c[1], c[2]])
            }
        };
        let (value, step) = match metric {
            TipMetric::Size => (width, 100.0),
            TipMetric::Softness => (softness * 100.0, 10.0),
            TipMetric::Opacity => (opacity * 100.0, 10.0),
            TipMetric::Hue => (hsv[0] * 360.0, 10.0),
            TipMetric::Saturation => (hsv[1] * 100.0, 10.0),
            TipMetric::Value => (hsv[2] * 100.0, 10.0),
        };
        (value * step).round() / step
    }

    /// Apply a typed value through the setters the HUD drag writes with,
    /// so it lands where a scrub would (tool memory, or the HUD target).
    fn apply_tip_metric(&mut self, metric: TipMetric, value: f32) {
        let (width, softness, opacity) = self.active_tip();
        let channel = match metric {
            TipMetric::Size => return self.set_active_tip(value, softness, opacity),
            TipMetric::Softness => return self.set_active_tip(width, value / 100.0, opacity),
            TipMetric::Opacity => return self.set_active_tip(width, softness, value / 100.0),
            TipMetric::Hue => 0,
            TipMetric::Saturation => 1,
            TipMetric::Value => 2,
        };
        let Some(BrushHud::Wheel { hsv, .. }) = &mut self.brush_hud else {
            return;
        };
        hsv[channel] = value / if channel == 0 { 360.0 } else { 100.0 };
        let rgb = hsv_to_rgb(*hsv);
        self.set_active_rgb(rgb);
    }

    /// Apply the field being typed, if it parses and changed. False when
    /// it does not parse.
    fn apply_tip_field(&mut self) -> bool {
        let Some((field, Some(edit))) = &self.tip_numeric.panel else {
            return true;
        };
        let Some(&metric) = self.tip_metrics().get(*field) else {
            return true;
        };
        use atlas_shell::widgets::{typed_number_event, TypedNumber};
        match typed_number_event(&edit.text, metric.range(), true, false, false, false) {
            TypedNumber::Commit(v) => {
                if v != self.tip_metric_shown(metric) {
                    self.apply_tip_metric(metric, v);
                }
                true
            }
            _ => false,
        }
    }

    /// Runs before the global command map, like `shape_property_keys`:
    /// while the panel is open it owns the keyboard, so digits never reach
    /// type-to-command or tool letters. Esc closes the HUD and restores;
    /// Tab and Shift+Tab apply the field they leave and move.
    pub(crate) fn tip_numeric_keys(&mut self, ctx: &egui::Context) -> bool {
        if self.tip_numeric.panel.is_none() {
            return false;
        }
        if self.brush_hud.is_none() {
            self.tip_numeric.close();
            return false;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            ctx.memory_mut(|m| m.stop_text_input());
            self.cancel_brush_hud();
            return true;
        }
        let back = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
        let next = !back && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
        if (back || next) && self.apply_tip_field() {
            let n = self.tip_metrics().len().max(1) as i32;
            let field = self.tip_numeric.panel.as_ref().map_or(0, |(f, _)| *f) as i32;
            let field = (field + if back { -1 } else { 1 }).rem_euclid(n) as usize;
            let shown = self.tip_metric_shown(self.tip_metrics()[field]);
            self.tip_numeric.panel = Some((field, Some(chrome::NumberEdit::new(shown))));
        }
        true
    }

    /// Screen bounds of the open HUD: the size or opacity circle with its
    /// readout above, or the color wheel.
    fn tip_hud_bounds(&self, hud: BrushHud, xf: &BoardXf) -> egui::Rect {
        match hud {
            BrushHud::Size { origin, .. } | BrushHud::Opacity { origin, .. } => {
                let o = xf.w2s(origin);
                let r = (self.active_tip().0 * 0.5 * xf.z).max(1.5);
                let circle = egui::Rect::from_center_size(o, egui::Vec2::splat(2.0 * r));
                let readout = egui::Rect::from_min_max(
                    Pos2::new(o.x - 70.0, o.y - r - 32.0),
                    Pos2::new(o.x + 70.0, o.y - r),
                );
                circle.union(readout)
            }
            BrushHud::Wheel { center, .. } => {
                egui::Rect::from_center_size(center, egui::Vec2::splat(2.0 * WHEEL_BACKDROP_RADIUS))
            }
        }
    }

    /// The open numeric panel: one row per metric, a label and the shared
    /// inline number. Pointer-attached chrome (P2.GhostFollow), so screen
    /// px; `place_popup` puts it beside the HUD, on `POPUP_ORDER`. Enter
    /// applies and closes (one journaled step), a press away from it
    /// applies, closes, and is eaten (`tip_numeric_eats`). Runs before
    /// board input; true while the pointer is over it.
    pub(crate) fn tip_numeric_ui(&mut self, ui: &mut egui::Ui, xf: &BoardXf) -> bool {
        let ctx = ui.ctx().clone();
        let (pressed, down, released) = ctx.input(|i| {
            (
                i.pointer.any_pressed(),
                i.pointer.any_down(),
                i.pointer.any_released(),
            )
        });
        if self.tip_numeric.eat && !pressed && !down && !released {
            self.tip_numeric.eat = false;
        }
        let Some((mut field, mut input)) = self.tip_numeric.panel.take() else {
            return false;
        };
        let Some(hud) = self.brush_hud else {
            return false;
        };
        let metrics = self.tip_metrics();
        if metrics.is_empty() {
            return false;
        }
        field = field.min(metrics.len() - 1);
        let anchor = self.tip_hud_bounds(hud, xf);
        let size = egui::vec2(
            PANEL_WIDTH,
            PANEL_PAD * 2.0 + PANEL_ROW * metrics.len() as f32,
        );
        let rect = chrome::place_popup(size, anchor, anchor, PANEL_GAP, ui.clip_rect());
        atlas_shell::menu_wheel::claim(&ctx, rect);
        let theme = self.palette();
        let mut typed: Vec<(TipMetric, f32)> = Vec::new();
        let mut picked = None;
        chrome::popup_area(&ctx, egui::Id::new("tip_numeric"), ui.layer_id())
            .fixed_pos(rect.min)
            .constrain(false)
            .movable(false)
            .fade_in(false)
            .show(&ctx, |ui| {
                ui.set_min_size(rect.size());
                chrome::panel(ui, rect, 1.0, theme);
                for (i, &metric) in metrics.iter().enumerate() {
                    let y = rect.top() + PANEL_PAD + PANEL_ROW * (i as f32 + 0.5);
                    atlas_shell::canvas_text::text(
                        ui.painter(),
                        Pos2::new(rect.left() + 10.0, y),
                        egui::Align2::LEFT_CENTER,
                        self.tip_metric_label(metric),
                        atlas_shell::canvas_scale::font(12.0, 1.0),
                        if i == field { theme.ink } else { theme.sub },
                    );
                    let mut edit = if i == field { input.take() } else { None };
                    let result = chrome::inline_number(
                        ui,
                        egui::Id::new(("tip_numeric", i)),
                        Pos2::new(rect.right() - 45.0, y),
                        0.0,
                        "",
                        metric.suffix(),
                        self.tip_metric_shown(metric),
                        1.0,
                        theme,
                        theme.ink,
                        &mut edit,
                        metric.range(),
                        true,
                    );
                    if let Some(v) = result.value {
                        typed.push((metric, v));
                    }
                    if i == field {
                        input = edit;
                    } else if edit.is_some() {
                        picked = Some((i, edit));
                    }
                }
            });
        for (metric, v) in typed {
            self.apply_tip_metric(metric, v);
        }
        if let Some((i, edit)) = picked {
            field = i;
            input = edit;
        }
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let away = pressed && ctx.pointer_latest_pos().is_some_and(|p| !rect.contains(p));
        if (enter && input.is_none()) || away {
            ctx.memory_mut(|m| m.stop_text_input());
            if enter {
                ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            }
            self.commit_brush_hud();
            if away {
                self.tip_numeric.eat = true;
                if ctx.input(|i| i.pointer.primary_pressed()) {
                    self.board_align_eat_press = true;
                }
            }
            return false;
        }
        if input.is_none() {
            // A press on the panel between fields committed the one being
            // typed: keep typing into it.
            input = Some(chrome::NumberEdit::new(
                self.tip_metric_shown(metrics[field]),
            ));
        }
        self.tip_numeric.panel = Some((field, input));
        ctx.pointer_latest_pos().is_some_and(|p| rect.contains(p))
    }
}
