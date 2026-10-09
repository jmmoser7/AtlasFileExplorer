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

/// Where the pointer sits on the collapse chevron, in screen pixels from its center.
/// Negative is above. Bands are designed pixels scaled by the board zoom (P0.9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChevronZone {
    /// Further above center: collapse all the way.
    FullCollapse,
    /// Slightly above center: collapse one level.
    PartialCollapse,
    /// Dead center: open one level.
    OneStep,
    /// Below center: open all the way. The full-expand zone paints a double chevron.
    FullExpand,
}

const ZONE_CENTER: f32 = 2.0;
const ZONE_NEAR: f32 = 7.0;

impl ChevronZone {
    pub(crate) fn detail(self) -> &'static str {
        match self {
            Self::OneStep => "one",
            Self::FullExpand => "full",
            Self::PartialCollapse => "partial",
            Self::FullCollapse => "collapse",
        }
    }

    pub(crate) fn parse(detail: &str) -> Option<Self> {
        Some(match detail {
            "one" => Self::OneStep,
            "full" => Self::FullExpand,
            "partial" => Self::PartialCollapse,
            "collapse" => Self::FullCollapse,
            _ => return None,
        })
    }
}

/// `dy` is `pointer.y - center.y` in screen pixels. `z` is the board zoom.
pub(crate) fn chevron_zone(dy: f32, z: f32) -> ChevronZone {
    let z = if z.is_finite() && z > 0.0 { z } else { 0.01 };
    let center = atlas_shell::canvas_scale::px(ZONE_CENTER, z);
    let near = atlas_shell::canvas_scale::px(ZONE_NEAR, z);
    if dy < -near {
        ChevronZone::FullCollapse
    } else if dy < -center {
        ChevronZone::PartialCollapse
    } else if dy <= center {
        ChevronZone::OneStep
    } else {
        ChevronZone::FullExpand
    }
}

/// `(points_up, double)`. The full-expand zone is the animated double chevron.
pub(crate) fn chevron_glyph(zone: ChevronZone, fold: CardFold) -> (bool, bool) {
    match zone {
        ChevronZone::FullExpand => (false, true),
        ChevronZone::FullCollapse => (true, true),
        ChevronZone::PartialCollapse => (true, false),
        ChevronZone::OneStep => (matches!(fold, CardFold::Open), false),
    }
}

pub(crate) fn apply_chevron(fold: CardFold, zone: ChevronZone) -> CardFold {
    match zone {
        ChevronZone::FullCollapse => CardFold::Collapsed,
        ChevronZone::PartialCollapse => match fold {
            CardFold::Open => CardFold::Partial,
            CardFold::Partial | CardFold::Collapsed => CardFold::Collapsed,
        },
        ChevronZone::OneStep => match fold {
            CardFold::Collapsed => CardFold::Partial,
            CardFold::Partial | CardFold::Open => CardFold::Open,
        },
        ChevronZone::FullExpand => CardFold::Open,
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
pub(crate) fn token_readout(reported: Option<u64>, chars: usize) -> String {
    match reported {
        Some(n) => n.to_string(),
        None => format!("~{}", chars / 4),
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
    fn chevron_zones_follow_the_center_and_scale() {
        assert_eq!(chevron_zone(0.0, 1.0), ChevronZone::OneStep);
        assert_eq!(chevron_zone(1.5, 1.0), ChevronZone::OneStep);
        assert_eq!(chevron_zone(3.0, 1.0), ChevronZone::FullExpand);
        assert_eq!(chevron_zone(-4.0, 1.0), ChevronZone::PartialCollapse);
        assert_eq!(chevron_zone(-8.0, 1.0), ChevronZone::FullCollapse);
        assert_eq!(chevron_zone(3.0, 2.0), ChevronZone::OneStep);
        assert_eq!(chevron_zone(-10.0, 2.0), ChevronZone::PartialCollapse);
        assert_eq!(chevron_zone(-16.0, 2.0), ChevronZone::FullCollapse);
        assert_eq!(chevron_zone(5.0, 2.0), ChevronZone::FullExpand);
    }

    #[test]
    fn chevron_clicks_step_and_jump() {
        assert_eq!(
            apply_chevron(CardFold::Collapsed, ChevronZone::OneStep),
            CardFold::Partial
        );
        assert_eq!(
            apply_chevron(CardFold::Partial, ChevronZone::OneStep),
            CardFold::Open
        );
        assert_eq!(
            apply_chevron(CardFold::Open, ChevronZone::OneStep),
            CardFold::Open
        );
        assert_eq!(
            apply_chevron(CardFold::Collapsed, ChevronZone::FullExpand),
            CardFold::Open
        );
        assert_eq!(
            apply_chevron(CardFold::Open, ChevronZone::PartialCollapse),
            CardFold::Partial
        );
        assert_eq!(
            apply_chevron(CardFold::Partial, ChevronZone::PartialCollapse),
            CardFold::Collapsed
        );
        assert_eq!(
            apply_chevron(CardFold::Partial, ChevronZone::FullCollapse),
            CardFold::Collapsed
        );
        assert_eq!(
            chevron_glyph(ChevronZone::FullExpand, CardFold::Collapsed),
            (false, true)
        );
        assert_eq!(
            chevron_glyph(ChevronZone::FullCollapse, CardFold::Open),
            (true, true)
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
    }

    #[test]
    fn just_build_uses_the_workbook_assets_or_the_data_dir() {
        let saved = Path::new(r"C:\Boards\Garden.slate");
        let data = Path::new(r"C:\Users\me\AppData\Local\NativeFileAtlas");
        let (path, fallback) = agent_build_dir(Some(saved), data);
        assert!(!fallback);
        assert_eq!(path, PathBuf::from(r"C:\Boards\assets\agent"));
        let (path, fallback) = agent_build_dir(None, data);
        assert!(fallback);
        assert_eq!(
            path,
            PathBuf::from(r"C:\Users\me\AppData\Local\NativeFileAtlas\assets\agent")
        );
    }
}
