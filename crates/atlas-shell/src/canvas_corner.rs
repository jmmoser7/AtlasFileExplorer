//! Lower-left canvas chrome layout: readout chevron, suggestion box, zoom
//! cluster, and the bottom tool palette band they align to.
//! Window chrome is screen-sized (P0.9 exception).

use crate::tokens;
use eframe::egui::{Pos2, Rect, Vec2};

/// Screen-space band of the bottom-center tool palette icons (full canvas width).
pub fn bottom_tool_palette_rect(canvas: Rect) -> Rect {
    let dock = tokens::current().dock.clone();
    let bottom = canvas.bottom() - dock.bottom_margin;
    Rect::from_min_max(
        Pos2::new(canvas.left(), bottom - dock.icon_size),
        Pos2::new(canvas.right(), bottom),
    )
}

/// Hit rect for the readout expand/collapse chevron: the canvas's lower-left
/// corner, just above the readout strip.
pub fn readout_chevron_hit_rect(canvas: Rect) -> Rect {
    let t = tokens::current().readouts.clone();
    let hit = t.chevron_hit.max(t.chevron_size);
    let left_bottom = canvas.left_bottom() + Vec2::new(t.chevron_inset_x, -t.chevron_inset_y);
    Rect::from_min_max(
        Pos2::new(left_bottom.x, left_bottom.y - hit),
        Pos2::new(left_bottom.x + hit, left_bottom.y),
    )
}

/// Hit rect for the suggestion-box button: right of the chevron, a palette
/// icon tall, on the palette's baseline.
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
    let left =
        (canvas.left() + t.suggestion_inset_x).max(chevron.right() + t.suggestion_gap_from_chevron);
    Rect::from_min_max(
        Pos2::new(left, palette.bottom() - size),
        Pos2::new(left + size, palette.bottom()),
    )
}

/// Left-center anchor of the zoom cluster, past the suggestion button and
/// centered on the palette band.
pub fn zoom_cluster_anchor(canvas: Rect) -> Pos2 {
    let gap = tokens::current().dock.icon_gap;
    let button = suggestion_button_hit_rect(canvas);
    Pos2::new(button.right() + gap, button.center().y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvases() -> [Rect; 2] {
        // Readouts open (canvas stops above the strip) and closed, both
        // offset by a left tools rail.
        [
            Rect::from_min_max(Pos2::new(48.0, 40.0), Pos2::new(1440.0, 820.0)),
            Rect::from_min_max(Pos2::new(48.0, 40.0), Pos2::new(1440.0, 900.0)),
        ]
    }

    #[test]
    fn suggestion_never_overlaps_the_readout_chevron() {
        for canvas in canvases() {
            let chevron = readout_chevron_hit_rect(canvas);
            let suggestion = suggestion_button_hit_rect(canvas);
            assert!(
                !chevron.intersects(suggestion),
                "{chevron:?} vs {suggestion:?}"
            );
            assert!(
                suggestion.left() - chevron.right()
                    >= tokens::current().readouts.suggestion_gap_from_chevron - 0.01,
                "gap after chevron"
            );
            assert!(canvas.contains_rect(chevron) && canvas.contains_rect(suggestion));
        }
    }

    #[test]
    fn suggestion_matches_tool_palette_height_and_baseline() {
        for canvas in canvases() {
            let palette = bottom_tool_palette_rect(canvas);
            let suggestion = suggestion_button_hit_rect(canvas);
            assert!((suggestion.height() - palette.height()).abs() < 0.01);
            assert!((suggestion.bottom() - palette.bottom()).abs() < 0.01);
            assert!((suggestion.top() - palette.top()).abs() < 0.01);
        }
    }

    #[test]
    fn suggestion_sits_in_from_the_corner() {
        for canvas in canvases() {
            let suggestion = suggestion_button_hit_rect(canvas);
            assert!(
                suggestion.left() - canvas.left() >= tokens::current().readouts.suggestion_inset_x
            );
            assert!(
                canvas.bottom() - suggestion.bottom()
                    >= tokens::current().dock.bottom_margin - 0.01
            );
        }
    }

    #[test]
    fn zoom_cluster_starts_past_the_suggestion_button() {
        for canvas in canvases() {
            let anchor = zoom_cluster_anchor(canvas);
            assert!(anchor.x > suggestion_button_hit_rect(canvas).right());
        }
    }

    /// The painted areas (File Atlas shows all three), not just the layout math.
    #[test]
    fn painted_corner_widgets_stay_apart() {
        use crate::widgets::{canvas_mini_menu, MiniMenuModel};
        use eframe::egui::{self, Id};
        let ctx = egui::Context::default();
        let palette = crate::theme::Palette::for_mode(true);
        let mut hub = crate::feedback::FeedbackHub::default();
        for canvas in canvases() {
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                    ..Default::default()
                };
                let _ = ctx.run(input, |ctx| {
                    crate::feedback::suggestion_button(ctx, &palette, "t", canvas, &mut hub);
                    let model = MiniMenuModel {
                        zoom_pct: Some(100.0),
                        fullscreen: false,
                    };
                    let _ = canvas_mini_menu(ctx, &palette, "t", canvas, model);
                });
            }
            let area = |id: Id| ctx.memory(|m| m.area_rect(id)).expect("area painted");
            let button = area(Id::new(("suggestion_box", "t")));
            let chevron = area(Id::new(("readout_chevron", "t")));
            let zoom = area(Id::new(("canvas_zoom_cluster", "t")));
            assert!(
                !button.intersects(chevron),
                "{button:?} vs chevron {chevron:?}"
            );
            assert!(!button.intersects(zoom), "{button:?} vs zoom {zoom:?}");
            assert!((button.bottom() - bottom_tool_palette_rect(canvas).bottom()).abs() < 0.5);
        }
    }
}
