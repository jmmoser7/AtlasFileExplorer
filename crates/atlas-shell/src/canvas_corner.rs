//! Lower-left canvas chrome layout: readout chevron, suggestion box, dock baseline.
//! Window chrome is screen-sized (P0.9 exception).

use crate::tokens;
use eframe::egui::{Pos2, Rect};

/// Screen-space rect of the bottom-center tool palette icon strip.
pub fn bottom_tool_palette_rect(canvas: Rect) -> Rect {
    let dock = tokens::current().dock.clone();
    let top = canvas.bottom() - dock.bottom_margin - dock.icon_size;
    Rect::from_min_max(
        Pos2::new(canvas.left(), top),
        Pos2::new(canvas.right(), canvas.bottom() - dock.bottom_margin),
    )
}

/// Hit rect for the readout expand/collapse chevron (pivot `LEFT_BOTTOM`).
pub fn readout_chevron_hit_rect(canvas: Rect) -> Rect {
    let t = tokens::current().readouts.clone();
    let palette = bottom_tool_palette_rect(canvas);
    let hit = t.chevron_hit.max(t.chevron_size);
    let bottom = palette.bottom() - (palette.height() - hit).max(0.0) * 0.5;
    let anchor = Pos2::new(t.chevron_inset_x, bottom);
    Rect::from_min_max(
        Pos2::new(anchor.x, anchor.y - hit),
        Pos2::new(anchor.x + hit, anchor.y),
    )
}

/// Hit rect for the suggestion-box button; same vertical band as the tool palette.
pub fn suggestion_button_hit_rect(canvas: Rect) -> Rect {
    let dock = tokens::current().dock.clone();
    let t = tokens::current().readouts.clone();
    let palette = bottom_tool_palette_rect(canvas);
    let size = if t.suggestion_size > 0.0 {
        t.suggestion_size
    } else {
        dock.icon_size
    };
    let chevron = readout_chevron_hit_rect(canvas);
    let mut left = chevron.left() - t.suggestion_gap_from_chevron - size;
    left = left.max(t.suggestion_inset_x);
    Rect::from_min_max(
        Pos2::new(left, palette.top()),
        Pos2::new(left + size, palette.bottom()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas() -> Rect {
        Rect::from_min_size(Pos2::new(0.0, 0.0), eframe::egui::Vec2::new(1440.0, 820.0))
    }

    #[test]
    fn suggestion_does_not_overlap_readout_chevron() {
        let canvas = canvas();
        let chevron = readout_chevron_hit_rect(canvas);
        let suggestion = suggestion_button_hit_rect(canvas);
        assert!(!chevron.intersects(suggestion));
        assert!(
            suggestion.right() + tokens::current().readouts.suggestion_gap_from_chevron
                <= chevron.left() + 0.5,
            "gap before chevron"
        );
    }

    #[test]
    fn suggestion_matches_tool_palette_height() {
        let palette = bottom_tool_palette_rect(canvas());
        let suggestion = suggestion_button_hit_rect(canvas());
        assert!((suggestion.height() - palette.height()).abs() < 0.01);
        assert!((suggestion.bottom() - palette.bottom()).abs() < 0.01);
        assert!((suggestion.top() - palette.top()).abs() < 0.01);
    }
}
