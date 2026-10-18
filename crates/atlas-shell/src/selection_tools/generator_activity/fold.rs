//! Shared transcript expansion and world-space streaming follow.
use crate::canvas_scale;
use eframe::egui::{self, Color32, Pos2};

/// How far a chat card is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardFold {
    /// Three-line capsule.
    Collapsed,
    /// Twice the capsule, scrolling.
    Partial,
    /// The card fits its text.
    Open,
}

/// Toward the three-line capsule, or toward the card that fits its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldDir {
    Collapse,
    Expand,
}

/// One level, or both remaining levels (the double chevron).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldStep {
    Single,
    Double,
}

/// Designed-px chevron: arm half-width, apex rise, and a thin stroke.
pub const GLYPH_HALF: f32 = 2.4;
pub const GLYPH_RISE: f32 = 1.35;
/// The double chevron's inked height over the single's.
const DOUBLE_RATIO: f32 = 1.4;

pub const GLYPH_STROKE: f32 = 0.55;

/// Cosine of an arm's slope, which turns vertical offsets into stroke distance.
fn arm_cos() -> f32 {
    GLYPH_HALF / GLYPH_HALF.hypot(GLYPH_RISE * 2.0)
}

/// Inked height of one mark, stroke included.
pub fn glyph_single_height() -> f32 {
    GLYPH_RISE * 2.0 + GLYPH_STROKE / arm_cos()
}

/// Center distance of the two marks in a double chevron.
pub fn glyph_double_pitch() -> f32 {
    glyph_single_height() * (DOUBLE_RATIO - 1.0)
}

pub fn glyph_double_height() -> f32 {
    glyph_double_pitch() + glyph_single_height()
}

/// Clear space between the two marks' strokes, measured across the arms.
pub fn glyph_double_gap() -> f32 {
    glyph_double_pitch() * arm_cos() - GLYPH_STROKE
}

/// Tuned chevron: arm half-width, proportional rise, stroke. World units.
pub fn chevron_metrics() -> (f32, f32, f32) {
    let card = crate::tokens::current().chat_card;
    let half = card.chevron_size;
    (half, half * (GLYPH_RISE / GLYPH_HALF), card.chevron_stroke)
}

fn arm_cos_of(half: f32, rise: f32) -> f32 {
    half / half.hypot(rise * 2.0)
}

pub fn glyph_double_pitch_of(half: f32, rise: f32, stroke: f32) -> f32 {
    let single = rise * 2.0 + stroke / arm_cos_of(half, rise);
    single * (DOUBLE_RATIO - 1.0)
}

pub fn glyph_slot_of(half: f32, rise: f32, stroke: f32) -> f32 {
    let single = rise * 2.0 + stroke / arm_cos_of(half, rise);
    single * DOUBLE_RATIO + 0.8
}

/// Chevrons offered on a hovered card, top to bottom.
/// Collapsed opens; maximized closes; partial offers one step each way,
/// since from the middle a double would land on the same end.
pub fn chevron_offers(fold: CardFold) -> &'static [(FoldDir, FoldStep)] {
    use FoldDir::*;
    use FoldStep::*;
    match fold {
        CardFold::Collapsed => &[(Expand, Single), (Expand, Double)],
        CardFold::Open => &[(Collapse, Double), (Collapse, Single)],
        CardFold::Partial => &[(Collapse, Single), (Expand, Single)],
    }
}

pub fn chevron_detail(dir: FoldDir, step: FoldStep) -> &'static str {
    match (dir, step) {
        (FoldDir::Expand, FoldStep::Single) => "step-expand",
        (FoldDir::Expand, FoldStep::Double) => "jump-expand",
        (FoldDir::Collapse, FoldStep::Single) => "step-collapse",
        (FoldDir::Collapse, FoldStep::Double) => "jump-collapse",
    }
}

pub fn parse_chevron(detail: &str) -> Option<(FoldDir, FoldStep)> {
    Some(match detail {
        "step-expand" | "one" => (FoldDir::Expand, FoldStep::Single),
        "jump-expand" | "full" => (FoldDir::Expand, FoldStep::Double),
        "step-collapse" | "partial" => (FoldDir::Collapse, FoldStep::Single),
        "jump-collapse" | "collapse" => (FoldDir::Collapse, FoldStep::Double),
        _ => return None,
    })
}

/// Step one level, or two. A step that would pass an end lands on that end.
pub fn apply_fold(fold: CardFold, dir: FoldDir, step: FoldStep) -> CardFold {
    let index = match fold {
        CardFold::Collapsed => 0,
        CardFold::Partial => 1,
        CardFold::Open => 2,
    };
    let delta = match step {
        FoldStep::Single => 1,
        FoldStep::Double => 2,
    };
    let next = match dir {
        FoldDir::Expand => (index + delta).min(2),
        FoldDir::Collapse => (index - delta).max(0),
    };
    match next {
        0 => CardFold::Collapsed,
        1 => CardFold::Partial,
        _ => CardFold::Open,
    }
}

/// Streaming default: twice the collapsed capsule, so new text can scroll.
pub fn stream_card_height(capsule: f32) -> f32 {
    capsule * 2.0
}

/// Auto-follow of a streaming transcript. Offsets are in the same units as the viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FollowScroll {
    pub follow: bool,
    pub offset: f32,
}

impl Default for FollowScroll {
    fn default() -> Self {
        Self {
            follow: true,
            offset: 0.0,
        }
    }
}

fn max_offset(content: f32, view: f32) -> f32 {
    (content - view).max(0.0)
}

/// The offset to hand the scroller this frame.
pub fn requested_offset(state: &FollowScroll, content: f32, view: f32) -> f32 {
    let max = max_offset(content, view);
    if state.follow {
        max
    } else {
        state.offset.clamp(0.0, max)
    }
}

/// What the scroller actually landed on. A move away from the bottom we asked
/// for stops following; returning to the bottom starts it again.
pub fn settle_follow(
    state: &mut FollowScroll,
    requested: f32,
    got: f32,
    content: f32,
    view: f32,
) -> f32 {
    const SLACK: f32 = 2.0;
    let max = max_offset(content, view);
    let got = if got.is_finite() { got } else { 0.0 };
    let at_bottom = got + SLACK >= max;
    if state.follow {
        if !at_bottom && (requested - got).abs() > SLACK {
            state.follow = false;
            state.offset = got.clamp(0.0, max);
        } else {
            state.offset = max;
        }
    } else if at_bottom {
        state.follow = true;
        state.offset = max;
    } else {
        state.offset = got.clamp(0.0, max);
    }
    state.offset
}

pub fn paint_chevron_glyph(painter: &egui::Painter, center: Pos2, z: f32, up: bool, ink: Color32) {
    let (half_w, rise_w, stroke_w) = chevron_metrics();
    let half = canvas_scale::px(half_w, z);
    let rise = canvas_scale::px(rise_w, z) * if up { -1.0 } else { 1.0 };
    let stroke = egui::Stroke::new(canvas_scale::px(stroke_w, z), ink);
    painter.add(egui::Shape::line(
        vec![
            center + egui::vec2(-half, -rise),
            center + egui::vec2(0.0, rise),
            center + egui::vec2(half, -rise),
        ],
        stroke,
    ));
}

/// Single mark, or two packed into [`glyph_double_height`].
pub fn paint_fold_glyph(
    painter: &egui::Painter,
    center: Pos2,
    z: f32,
    dir: FoldDir,
    step: FoldStep,
    ink: Color32,
) {
    let up = dir == FoldDir::Collapse;
    if step == FoldStep::Single {
        paint_chevron_glyph(painter, center, z, up, ink);
        return;
    }
    let (half, rise, stroke) = chevron_metrics();
    let pitch = canvas_scale::px(glyph_double_pitch_of(half, rise, stroke), z);
    paint_chevron_glyph(painter, center + egui::vec2(0.0, -pitch * 0.5), z, up, ink);
    paint_chevron_glyph(painter, center + egui::vec2(0.0, pitch * 0.5), z, up, ink);
}
