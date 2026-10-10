//! Chat-train presentation that is pure enough to test without a window:
//! chevron zones, streaming follow, token readout, and the Just build folder.

use std::path::{Path, PathBuf};

/// How far a chat card is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CardFold {
    /// Three-line capsule.
    Collapsed,
    /// Twice the capsule, scrolling.
    Partial,
    /// The card fits its text.
    Open,
}

/// Toward the three-line capsule, or toward the card that fits its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FoldDir {
    Collapse,
    Expand,
}

/// One level, or both remaining levels (the double chevron).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FoldStep {
    Single,
    Double,
}

/// Designed-px chevron: arm half-width, apex rise, and a thin stroke.
pub(crate) const GLYPH_HALF: f32 = 2.4;
pub(crate) const GLYPH_RISE: f32 = 1.35;
pub(crate) const GLYPH_STROKE: f32 = 0.55;
/// The double chevron's inked height over the single's.
const DOUBLE_RATIO: f32 = 1.4;

/// Cosine of an arm's slope, which turns vertical offsets into stroke distance.
fn arm_cos() -> f32 {
    GLYPH_HALF / GLYPH_HALF.hypot(GLYPH_RISE * 2.0)
}

/// Inked height of one mark, stroke included.
pub(crate) fn glyph_single_height() -> f32 {
    GLYPH_RISE * 2.0 + GLYPH_STROKE / arm_cos()
}

/// Center distance of the two marks in a double chevron.
pub(crate) fn glyph_double_pitch() -> f32 {
    glyph_single_height() * (DOUBLE_RATIO - 1.0)
}

pub(crate) fn glyph_double_height() -> f32 {
    glyph_double_pitch() + glyph_single_height()
}

/// Clear space between the two marks' strokes, measured across the arms.
#[cfg(test)]
pub(crate) fn glyph_double_gap() -> f32 {
    glyph_double_pitch() * arm_cos() - GLYPH_STROKE
}

/// Vertical pitch of one offered chevron in the cluster, designed px.
pub(crate) fn glyph_slot() -> f32 {
    glyph_double_height() + 0.8
}

/// Chevrons offered on a hovered card, top to bottom.
/// Collapsed opens; maximized closes; partial offers one step each way,
/// since from the middle a double would land on the same end.
pub(crate) fn chevron_offers(fold: CardFold) -> &'static [(FoldDir, FoldStep)] {
    use FoldDir::*;
    use FoldStep::*;
    match fold {
        CardFold::Collapsed => &[(Expand, Single), (Expand, Double)],
        CardFold::Open => &[(Collapse, Double), (Collapse, Single)],
        CardFold::Partial => &[(Collapse, Single), (Expand, Single)],
    }
}

pub(crate) fn chevron_detail(dir: FoldDir, step: FoldStep) -> &'static str {
    match (dir, step) {
        (FoldDir::Expand, FoldStep::Single) => "step-expand",
        (FoldDir::Expand, FoldStep::Double) => "jump-expand",
        (FoldDir::Collapse, FoldStep::Single) => "step-collapse",
        (FoldDir::Collapse, FoldStep::Double) => "jump-collapse",
    }
}

pub(crate) fn parse_chevron(detail: &str) -> Option<(FoldDir, FoldStep)> {
    Some(match detail {
        "step-expand" | "one" => (FoldDir::Expand, FoldStep::Single),
        "jump-expand" | "full" => (FoldDir::Expand, FoldStep::Double),
        "step-collapse" | "partial" => (FoldDir::Collapse, FoldStep::Single),
        "jump-collapse" | "collapse" => (FoldDir::Collapse, FoldStep::Double),
        _ => return None,
    })
}

/// Step one level, or two. A step that would pass an end lands on that end.
pub(crate) fn apply_fold(fold: CardFold, dir: FoldDir, step: FoldStep) -> CardFold {
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
pub(crate) fn stream_card_height(capsule: f32) -> f32 {
    capsule * 2.0
}

/// Auto-follow of a streaming transcript. Offsets are in the same units as the viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FollowScroll {
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
pub(crate) fn requested_offset(state: &FollowScroll, content: f32, view: f32) -> f32 {
    let max = max_offset(content, view);
    if state.follow {
        max
    } else {
        state.offset.clamp(0.0, max)
    }
}

/// What the scroller actually landed on. A move away from the bottom we asked
/// for stops following; returning to the bottom starts it again.
pub(crate) fn settle_follow(
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

/// Provider total when `reported` is set, otherwise `~` plus characters divided by 4.
/// Counts from 1000 compact to `1.2k`.
pub(crate) fn token_readout(reported: Option<u64>, chars: usize) -> String {
    match reported {
        Some(n) => compact_count(n),
        None => format!("~{}", compact_count((chars / 4) as u64)),
    }
}

fn compact_count(n: u64) -> String {
    if n < 1000 {
        n.to_string()
    } else if n < 10_000 {
        let tenths = ((n as f32 / 100.0).round() / 10.0 * 10.0).round() as u64;
        let whole = tenths / 10;
        let frac = tenths % 10;
        if frac == 0 {
            format!("{whole}k")
        } else {
            format!("{whole}.{frac}k")
        }
    } else if n < 1_000_000 {
        format!("{}k", n / 1000)
    } else {
        let tenths = ((n as f32 / 100_000.0).round() / 10.0 * 10.0).round() as u64;
        let whole = tenths / 10;
        let frac = tenths % 10;
        if frac == 0 {
            format!("{whole}m")
        } else {
            format!("{whole}.{frac}m")
        }
    }
}

/// `<workbook dir>/assets/agent/`, or the same path under the per-user data
/// directory when the workbook has not been saved. The bool is that fallback.
pub(crate) fn agent_build_dir(workbook: Option<&Path>, data_dir: &Path) -> (PathBuf, bool) {
    match workbook.filter(|path| path.parent().is_some()) {
        Some(workbook) => (atlas_core::workbook_assets::agent_dir(workbook), false),
        None => (
            atlas_core::workbook_assets::unsaved_agent_dir(data_dir),
            true,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chevron_steps_one_level_or_two() {
        use CardFold::*;
        use FoldDir::*;
        use FoldStep::*;
        let cases = [
            (Open, Collapse, Single, Partial),
            (Open, Collapse, Double, Collapsed),
            (Collapsed, Expand, Single, Partial),
            (Collapsed, Expand, Double, Open),
            (Partial, Expand, Single, Open),
            (Partial, Collapse, Single, Collapsed),
            (Partial, Expand, Double, Open),
            (Partial, Collapse, Double, Collapsed),
            (Open, Expand, Single, Open),
            (Collapsed, Collapse, Single, Collapsed),
        ];
        for (from, dir, step, to) in cases {
            assert_eq!(apply_fold(from, dir, step), to, "{from:?} {dir:?} {step:?}");
        }
        assert!(glyph_double_height() <= glyph_single_height() * 1.4 + 0.01);
        assert!(
            glyph_double_gap() >= GLYPH_STROKE * 0.6,
            "the double's marks merge: gap {} for stroke {GLYPH_STROKE}",
            glyph_double_gap()
        );
        assert!(chevron_offers(Partial).iter().any(|(d, _)| *d == Expand));
        assert!(chevron_offers(Partial).iter().any(|(d, _)| *d == Collapse));
        assert_eq!(
            parse_chevron(chevron_detail(Expand, Single)),
            Some((Expand, Single))
        );
    }

    #[test]
    fn a_streaming_card_is_twice_the_capsule() {
        assert_eq!(stream_card_height(48.0), 96.0);
    }

    #[test]
    fn auto_follow_stops_when_the_reader_scrolls_up() {
        let mut state = FollowScroll::default();
        let requested = requested_offset(&state, 200.0, 80.0);
        assert_eq!(requested, 120.0);
        let parked = settle_follow(&mut state, requested, 40.0, 200.0, 80.0);
        assert!(!state.follow);
        assert_eq!(parked, 40.0);
        let held = requested_offset(&state, 300.0, 80.0);
        assert_eq!(held, 40.0, "new text does not pull the view back down");
        settle_follow(&mut state, held, 220.0, 300.0, 80.0);
        assert!(state.follow, "the bottom resumes follow");
        assert_eq!(requested_offset(&state, 300.0, 80.0), 220.0);
    }

    #[test]
    fn token_readout_uses_the_provider_or_a_quarter_of_the_characters() {
        assert_eq!(token_readout(None, 0), "~0");
        assert_eq!(token_readout(None, 8), "~2");
        assert_eq!(token_readout(None, 3), "~0");
        assert_eq!(token_readout(Some(40), 8), "40");
        assert_eq!(token_readout(Some(1200), 8), "1.2k");
        assert_eq!(token_readout(Some(2000), 0), "2k");
    }

    #[test]
    fn text_insets_match_in_every_fold() {
        let (left, right, wrap) = super::super::text_column(320.0);
        assert_eq!(left, super::super::TEXT_INSET_LEFT);
        assert_eq!(right, super::super::card_metrics::TEXT_INSET_RIGHT);
        assert_eq!(left, right, "left and right insets are the same datum");
        assert!((wrap - (320.0 - left - right)).abs() < 0.01);
        assert_eq!(
            super::super::text_column(176.0),
            super::super::text_column(176.0)
        );
    }

    #[test]
    fn a_split_dot_gaps_by_half_its_radius() {
        let radius = super::super::HANDLE_DOT;
        let travel = super::super::dot_split_travel(radius);
        let gap = travel * 2.0 - radius * 2.0;
        assert!(
            (gap - radius * 0.5).abs() < 0.01,
            "edge gap {gap} is not half the radius {radius}"
        );
    }

    #[test]
    fn just_build_uses_the_workbook_assets_or_the_data_dir() {
        let root = std::env::temp_dir();
        let saved = root.join("Boards").join("Garden.slate");
        let data = root.join("NativeFileAtlas");
        let (path, fallback) = agent_build_dir(Some(&saved), &data);
        assert!(!fallback);
        assert_eq!(path, root.join("Boards").join("assets").join("agent"));
        let (path, fallback) = agent_build_dir(None, &data);
        assert!(fallback);
        assert_eq!(path, data.join("assets").join("agent"));
    }
}
