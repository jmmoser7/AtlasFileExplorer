//! The real attached editor, with a followed 17-event overflowing transcript.
use super::*;
use generator::activity::{ActivityState, CardFold};

#[test]
fn attached_activity_chevrons_remain_live_after_following_an_overflowing_transcript() {
    for order in [CANVAS_CHROME_ORDER, POPUP_ORDER] {
        for zoom in [0.5, 1.14, 2.0] {
            let mut editor = AttachedGenerator::new(order, zoom);
            editor.origin = Some(Pos2::new(300.0, 806.0 - 74.0 * zoom));
            editor.activity = Some(ActivityState {
                fold: CardFold::Partial,
                ..Default::default()
            });
            for _ in 0..8 {
                editor.frame(vec![]);
            }
            let state = editor.activity.unwrap();
            assert!(
                state.scroll.offset > 0.0,
                "follow has scrolled the 17-event transcript"
            );
            assert!(state.content_height > 480.0);
            for (index, expected) in [
                (1, CardFold::Open),
                (1, CardFold::Partial),
                (0, CardFold::Collapsed),
            ] {
                let output = editor.frame(vec![]).1;
                // Continuous dialogue keeps the existing fold controls in the
                // datum, with no duplicate Ready/Activity summary label.
                assert_eq!(editor.fold_hits.len(), 2);
                assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Text(text) if ["Ready", "Activity"].contains(&text.galley.text()))));
                let hit = editor.fold_hits[index];
                assert!(editor.bounds().contains_rect(hit));
                assert!((hit.width() / zoom - 14.0).abs() < 0.01);
                let point = hit.center();
                assert!(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0)).contains(point));
                let response = editor.click(point).0;
                assert_eq!(
                    response.activity_fold,
                    Some(expected),
                    "first click {index} at {zoom}, {order:?}, owner={:?}",
                    editor.ctx.layer_id_at(point)
                );
                assert_eq!(response.presentation.activity.unwrap().fold, expected);
                assert_eq!(
                    editor.activity.unwrap().fold,
                    expected,
                    "the app snapshot preserves the chosen fold"
                );
                for _ in 0..4 {
                    editor.frame(vec![]);
                }
            }
        }
    }
}
