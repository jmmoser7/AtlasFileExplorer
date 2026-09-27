//! Right-button tip HUDs beyond the brush: the open vector drawing tools
//! (Line, Polyline, Arc, Pen, Bezier) and Direct Select.
//!
//! Alt+right-drag sizes, Ctrl+right-drag opens the color wheel, and
//! Shift+right-drag sets opacity, exactly as for the Brush. A drawing tool
//! edits its create style (P1.curve.create-style), so the next curve uses
//! it. Direct Select edits its target curve live and journals one Patch
//! when the HUD closes. Softness stays brush-only: vector strokes are not
//! stamped, so there is nothing to blur.

use super::board::BoardTool;
use super::SlateApp;
use slate_doc::scene::{Node, NodeKind, Rgba, SceneCmd, ShapeKind};
use slate_doc::NodeId;

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
    /// The curve Direct Select is editing, when it is a stroked shape.
    pub(crate) fn hud_node(&self) -> Option<NodeId> {
        if self.board_tool != BoardTool::DirectSelect {
            return None;
        }
        let id = self.direct.node?;
        let node = self.doc().scene.node(id)?;
        match &node.kind {
            NodeKind::Shape(s)
                if matches!(s.shape, ShapeKind::Path | ShapeKind::Line) && !node.locked =>
            {
                Some(id)
            }
            _ => None,
        }
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
            BoardTool::DirectSelect => self.hud_node().is_some_and(|id| {
                matches!(
                    self.doc().scene.node(id).map(|n| &n.kind),
                    Some(NodeKind::Shape(s)) if s.stroke.paints_as_stamp()
                )
            }),
            _ => false,
        }
    }

    /// Width, softness, opacity for a curve tool or the Direct Select target.
    pub(crate) fn vector_tip(&self) -> Option<(f32, f32, f32)> {
        if curve_tool(self.board_tool) {
            let s = self.stroke_for_new_curve();
            return Some((s.width, 0.0, self.opacity_for_new_node(false)));
        }
        let node = self.doc().scene.node(self.hud_node()?)?;
        let NodeKind::Shape(s) = &node.kind else {
            return None;
        };
        Some((s.stroke.width, s.stroke.softness, node.opacity))
    }

    /// Write a curve tool's create style, or the Direct Select target live
    /// (journaled when the HUD closes). Returns false for other tools.
    pub(crate) fn set_vector_tip(&mut self, width: f32, softness: f32, opacity: f32) -> bool {
        if curve_tool(self.board_tool) {
            let mut s = self.stroke_for_new_curve();
            s.width = width;
            self.board_last_style.open.stroke = Some(s);
            self.board_last_style.open.opacity = Some(opacity.clamp(0.1, 1.0));
            self.flush_create_style_to_doc();
            return true;
        }
        let Some(id) = self.hud_node() else {
            return false;
        };
        let stamped = self.tip_hud_has_softness();
        if let Some(n) = self.doc_mut().scene.node_mut(id) {
            n.opacity = opacity.clamp(0.1, 1.0);
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
        if let Some(node) = self.hud_node().and_then(|id| self.doc().scene.node(id)) {
            if let NodeKind::Shape(s) = &node.kind {
                return s.stroke.color.0;
            }
        }
        self.board_colors.fg.0
    }

    /// Set the armed tool's color. Curve tools also move the foreground so
    /// the next brush and wire agree with the curve.
    pub(crate) fn set_active_rgba(&mut self, rgba: [u8; 4]) {
        if curve_tool(self.board_tool) {
            let mut s = self.stroke_for_new_curve();
            s.color = Rgba(rgba);
            self.board_last_style.open.stroke = Some(s);
            self.flush_create_style_to_doc();
            self.board_colors.fg.0[..3].copy_from_slice(&rgba[..3]);
            return;
        }
        if let Some(id) = self.hud_node() {
            if let Some(n) = self.doc_mut().scene.node_mut(id) {
                if let NodeKind::Shape(s) = &mut n.kind {
                    s.stroke.color = Rgba(rgba);
                }
            }
            return;
        }
        self.board_colors.fg.0 = rgba;
    }

    pub(crate) fn set_active_rgb(&mut self, rgb: [u8; 3]) {
        let mut c = self.active_rgba();
        c[..3].copy_from_slice(&rgb);
        self.set_active_rgba(c);
    }

    /// Snapshot of the Direct Select target, taken when a HUD or key edit
    /// starts.
    pub(crate) fn hud_node_snapshot(&self) -> Option<Node> {
        self.doc().scene.node(self.hud_node()?).cloned()
    }

    /// Journal the Direct Select target's HUD edit as one Patch.
    pub(crate) fn journal_hud_node(&mut self, before: Option<Node>) {
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

    /// Esc: put the Direct Select target back.
    pub(crate) fn restore_hud_node(&mut self, before: Option<Node>) {
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
use slate_doc::scene::{BrushTexture, Stroke, StrokeCap, StrokeJoin, WidthProfile};

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TipChoice {
    Texture(BrushTexture),
    Curve(CurveStyle),
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

/// Screen px between the size circle's bottom and the palette row.
const PALETTE_GAP: f32 = 30.0;
const PALETTE_SPACING: f32 = 32.0;
const PALETTE_ICON_R: f32 = 12.0;
const PALETTE_HIT_R: f32 = 15.0;
/// How narrow a taper gets at its thin end.
const TAPER_TIP: f32 = 0.12;

pub(crate) fn curve_style_of(stroke: &Stroke) -> CurveStyle {
    if stroke.arrow_end {
        return CurveStyle::Arrow;
    }
    match stroke.profile {
        WidthProfile::Taper { .. } => CurveStyle::TaperStart,
        WidthProfile::Ends { .. } => CurveStyle::TaperBoth,
        WidthProfile::Uniform if stroke.cap == StrokeCap::Round => CurveStyle::Round,
        WidthProfile::Uniform => CurveStyle::Square,
    }
}

pub(crate) fn apply_curve_style(stroke: &mut Stroke, style: CurveStyle) {
    let round = |s: &mut Stroke| {
        s.cap = StrokeCap::Round;
        s.join = StrokeJoin::Round;
    };
    stroke.arrow_end = false;
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

/// The band under the size circle where the style row lives. While the
/// pointer is in it, size and softness hold still so the row does not move
/// and the trip down does not keep scrubbing.
pub(crate) fn palette_zone(o: Pos2, r: f32, n: usize, pointer: Pos2) -> bool {
    if n == 0 {
        return false;
    }
    let half = (n as f32 - 1.0) * 0.5 * PALETTE_SPACING + PALETTE_HIT_R;
    pointer.y >= o.y + r + PALETTE_ZONE_TOP && (pointer.x - o.x).abs() <= half
}

/// The style band starts this far under the circle's rim.
const PALETTE_ZONE_TOP: f32 = 8.0;

impl SlateApp {
    /// The style choices the armed tool offers under its size circle.
    pub(crate) fn tip_choices(&self) -> &'static [TipChoice] {
        match self.board_tool {
            BoardTool::Brush | BoardTool::Eraser => &TEXTURE_CHOICES,
            _ if curve_tool(self.board_tool) => &CURVE_CHOICES,
            BoardTool::DirectSelect if self.hud_node().is_some() => {
                if self.tip_hud_has_softness() {
                    &TEXTURE_CHOICES
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
            _ if curve_tool(self.board_tool) => Some(TipChoice::Curve(curve_style_of(
                &self.stroke_for_new_curve(),
            ))),
            _ => {
                let s = self.hud_node_stroke()?;
                Some(if s.paints_as_stamp() {
                    TipChoice::Texture(s.texture)
                } else {
                    TipChoice::Curve(curve_style_of(&s))
                })
            }
        }
    }

    /// Apply a palette choice to the armed tool (Direct Select: live on its
    /// target, journaled when the HUD closes).
    pub(crate) fn apply_tip_choice(&mut self, choice: TipChoice) {
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
                self.board_last_style.open.stroke = Some(s);
                self.flush_create_style_to_doc();
            }
            (BoardTool::DirectSelect, choice) => {
                let Some(id) = self.hud_node() else {
                    return;
                };
                if let Some(n) = self.doc_mut().scene.node_mut(id) {
                    if let NodeKind::Shape(s) = &mut n.kind {
                        match choice {
                            TipChoice::Texture(t) => s.stroke.texture = t,
                            TipChoice::Curve(style) => apply_curve_style(&mut s.stroke, style),
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// The palette row under the size circle. Pointer-attached chrome
    /// (P2.GhostFollow), so icons stay screen-sized.
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
        let hovered = palette_hit(o, r, choices.len(), pointer);
        let ink = {
            let c = self.active_rgba();
            Color32::from_rgb(c[0], c[1], c[2])
        };
        for (i, choice) in choices.iter().enumerate() {
            let at = palette_slot(o, r, i, choices.len());
            let selected = current == Some(*choice);
            painter.circle_filled(
                at,
                PALETTE_ICON_R,
                Color32::from_rgba_unmultiplied(18, 18, 20, 230),
            );
            let ring = if hovered == Some(i) || selected {
                EStroke::new(2.0_f32, accent)
            } else {
                EStroke::new(1.0_f32, Color32::from_gray(90))
            };
            painter.circle_stroke(at, PALETTE_ICON_R, ring);
            paint_choice_glyph(painter, at, *choice, ink);
        }
        if let Some(i) = hovered {
            let at = palette_slot(o, r, i, choices.len());
            painter.text(
                at + egui::vec2(0.0, PALETTE_ICON_R + 4.0),
                egui::Align2::CENTER_TOP,
                choice_label(choices[i]),
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
    }
}

fn choice_label(choice: TipChoice) -> &'static str {
    match choice {
        TipChoice::Texture(t) => t.label(),
        TipChoice::Curve(CurveStyle::Square) => "Flat ends, square corners",
        TipChoice::Curve(CurveStyle::Round) => "Round ends, round corners",
        TipChoice::Curve(CurveStyle::Arrow) => "Arrow at the end",
        TipChoice::Curve(CurveStyle::TaperStart) => "Narrow at the start",
        TipChoice::Curve(CurveStyle::TaperBoth) => "Narrow at both ends",
    }
}

/// A small glyph for each choice, drawn in the tool's color.
fn paint_choice_glyph(painter: &egui::Painter, at: Pos2, choice: TipChoice, ink: Color32) {
    let v = egui::vec2;
    match choice {
        TipChoice::Texture(texture) => {
            let tip = vector_ink::StampStyle {
                diameter: 16.0,
                softness: 0.5,
                rgba: [ink.r(), ink.g(), ink.b(), 255],
                grain: texture.grain(),
            };
            // An 8x8 world-unit swatch of the real grain, sampled per cell.
            let cells = 8;
            let cell = 16.0 / cells as f32;
            for gy in 0..cells {
                for gx in 0..cells {
                    let lx = (gx as f32 + 0.5) * cell - 8.0;
                    let ly = (gy as f32 + 0.5) * cell - 8.0;
                    let dist = lx.hypot(ly);
                    let a = vector_ink::grain_coverage(
                        tip.grain,
                        dist,
                        8.0,
                        tip.softness,
                        lx * 3.0,
                        ly * 3.0,
                    );
                    if a <= 0.02 {
                        continue;
                    }
                    let min = at + v(lx - cell * 0.5, ly - cell * 0.5);
                    painter.rect_filled(
                        egui::Rect::from_min_size(min, v(cell, cell)),
                        0.0,
                        ink.gamma_multiply(a),
                    );
                }
            }
        }
        TipChoice::Curve(style) => {
            let a = at + v(-7.0, 5.0);
            let b = at + v(1.0, -5.0);
            let c = at + v(7.0, -5.0);
            match style {
                CurveStyle::Square => {
                    painter.add(egui::Shape::line(
                        vec![a, at + v(-7.0, -5.0), c],
                        EStroke::new(3.0_f32, ink),
                    ));
                }
                CurveStyle::Round => {
                    painter.add(egui::Shape::line(vec![a, b, c], EStroke::new(3.0_f32, ink)));
                    for p in [a, b, c] {
                        painter.circle_filled(p, 1.5, ink);
                    }
                }
                CurveStyle::Arrow => {
                    let tip = at + v(8.0, 0.0);
                    painter.line_segment(
                        [at + v(-8.0, 0.0), at + v(3.0, 0.0)],
                        EStroke::new(2.0_f32, ink),
                    );
                    painter.add(egui::Shape::convex_polygon(
                        vec![tip, at + v(2.0, -4.0), at + v(2.0, 4.0)],
                        ink,
                        EStroke::NONE,
                    ));
                }
                CurveStyle::TaperStart => {
                    painter.add(egui::Shape::convex_polygon(
                        vec![at + v(-8.0, 0.0), at + v(8.0, -3.5), at + v(8.0, 3.5)],
                        ink,
                        EStroke::NONE,
                    ));
                }
                CurveStyle::TaperBoth => {
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            at + v(-9.0, 0.0),
                            at + v(0.0, -3.5),
                            at + v(9.0, 0.0),
                            at + v(0.0, 3.5),
                        ],
                        ink,
                        EStroke::NONE,
                    ));
                }
            }
        }
    }
}
