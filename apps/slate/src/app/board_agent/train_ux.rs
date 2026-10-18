//! Chat-train presentation that is pure enough to test without a window:
//! chevron zones, streaming follow, token readout, and the Just build folder.

use std::path::{Path, PathBuf};

pub(crate) use atlas_shell::selection_tools::generator::activity::{
    apply_fold, chevron_detail, chevron_metrics, chevron_offers, glyph_slot_of, parse_chevron,
    requested_offset, settle_follow, stream_card_height, CardFold, FollowScroll,
};
#[cfg(test)]
use atlas_shell::selection_tools::generator::activity::{
    glyph_double_gap, glyph_double_height, glyph_single_height, FoldDir, FoldStep, GLYPH_STROKE,
};

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
        let radius = 3.5;
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
