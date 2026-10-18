//! Tool/end palette state; legacy tapers remain readable, not offered.
use super::glyph::paint_choice_glyph;
use super::*;
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
    /// Ephemeral intent ink: solid 0.5 seconds, then fades for 0.5 seconds.
    Fade,
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
    pub(super) fn end(self) -> StrokeEnd {
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

const CURVE_CHOICES: [TipChoice; 3] = [
    TipChoice::Curve(CurveStyle::Square),
    TipChoice::Curve(CurveStyle::Round),
    TipChoice::Curve(CurveStyle::Arrow),
];

const CURVE_DRAW_CHOICES: [TipChoice; 4] = [
    TipChoice::Curve(CurveStyle::Square),
    TipChoice::Curve(CurveStyle::Round),
    TipChoice::Curve(CurveStyle::Arrow),
    TipChoice::Curve(CurveStyle::Fade),
];

const END_CHOICES: [TipChoice; 3] = [
    TipChoice::End(EndStyle::Flat),
    TipChoice::End(EndStyle::Round),
    TipChoice::End(EndStyle::Arrow),
];

/// Screen px between the size circle's bottom and the palette row's centers.
const PALETTE_GAP: f32 = 36.0;
const PALETTE_SPACING: f32 = 50.0;
pub(super) const PALETTE_ICON_R: f32 = 21.0;
const PALETTE_HIT_R: f32 = 24.0;
/// Width of the preview stroke inside a swatch, screen px.
pub(super) const SWATCH_STROKE: f32 = 11.0;
/// How narrow a taper gets at its thin end.
const TAPER_TIP: f32 = slate_doc::scene::NARROW_TIP;

/// The whole-curve style `stroke` has, or `None` when its two ends were
/// set apart (P1.curve.vertex-style end conditions). Two equal effective
/// end caps are the curve's end cap; `cap` itself caps the dash ends.
pub(crate) fn curve_style_of(stroke: &Stroke) -> Option<CurveStyle> {
    let [start, end] = stroke.end_caps();
    if stroke.arrow_start || start != end {
        return None;
    }
    if stroke.arrow_end {
        return Some(CurveStyle::Arrow);
    }
    Some(match stroke.profile.narrow_ends() {
        [true, false] => CurveStyle::TaperStart,
        [true, true] => CurveStyle::TaperBoth,
        [false, true] => return None,
        [false, false] if end == StrokeCap::Round => CurveStyle::Round,
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
        CurveStyle::Round | CurveStyle::Fade => round(stroke),
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
        let grips = if super::super::board_line::line_endpoints(node).is_some() {
            2
        } else {
            match self.curve_grips_of(id)? {
                super::super::board_direct::CurveGrips::Arc(_) => 3,
                super::super::board_direct::CurveGrips::Anchors { anchors, .. } => anchors.len(),
            }
        };
        let last = grips.checked_sub(1).filter(|l| *l > 0)?;
        let ends = [points.contains(&0), points.contains(&last)];
        ends.contains(&true).then_some(ends)
    }

    /// The style choices the armed tool offers under its size circle.
    pub(crate) fn tip_choices(&self) -> &'static [TipChoice] {
        if self.field_selection().is_some() {
            return &[];
        }
        match self.board_tool {
            BoardTool::Brush | BoardTool::Eraser => &TEXTURE_CHOICES,
            _ if curve_tool(self.board_tool) => &CURVE_DRAW_CHOICES,
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
                if self.direct.fade_armed {
                    return Some(TipChoice::Curve(CurveStyle::Fade));
                }
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
                self.set_curve_fade_armed(Some(style == CurveStyle::Fade));
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
                        super::super::board_properties::edit_every_tip(n, |tip| tip.texture = t);
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
        let chrome = self.palette();
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
                atlas_shell::theme::Palette::alpha(chrome.extreme_bg, 235),
            );
            let ring = if hovered == Some(i) {
                EStroke::new(2.5_f32, accent)
            } else if selected {
                EStroke::new(2.0_f32, accent.gamma_multiply(0.8))
            } else {
                EStroke::new(1.0_f32, chrome.line)
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
                chrome.ink,
            );
        }
    }
}

/// A short stroke of `texture`, stamped once in white at three raster
/// pixels per screen px and kept in egui memory, so the row does no
/// rasterizing per frame. The rect is relative to the swatch center.
pub(crate) fn choice_label(choice: TipChoice, both: bool) -> &'static str {
    match choice {
        TipChoice::Texture(t) => t.label(),
        TipChoice::Curve(CurveStyle::Square) => "Flat ends, square corners",
        TipChoice::Curve(CurveStyle::Round) => "Round ends, round corners",
        TipChoice::Curve(CurveStyle::Arrow) => "Arrow at the end",
        TipChoice::Curve(CurveStyle::TaperStart) => "Narrow at the start",
        TipChoice::Curve(CurveStyle::TaperBoth) => "Narrow at both ends",
        TipChoice::Curve(CurveStyle::Fade) => "Fade: solid 0.5 s, fade 0.5 s",
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
