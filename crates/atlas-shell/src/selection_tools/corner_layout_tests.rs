use super::*;

#[test]
fn narrow_corner_rows_keep_every_control_positive_and_inside_the_apron() {
    for width in [120.0, 216.0, 420.0] {
        for zoom in [0.5, 1.0, 2.5] {
            let rect = Rect::from_min_size(
                Pos2::new(30.0, 40.0),
                Vec2::new(width, CORNER_HEIGHT) * zoom,
            );
            for crop in [false, true] {
                let layout = corner_layout(rect, crop, zoom);
                let parts: Vec<_> = layout
                    .crop
                    .into_iter()
                    .chain([layout.treatment, layout.track, layout.units])
                    .collect();
                for part in &parts {
                    assert!(
                        part.is_positive() && rect.contains_rect(*part),
                        "{part:?} in {rect:?}, crop={crop}"
                    );
                }
                for pair in parts.windows(2) {
                    assert!(pair[0].right() < pair[1].left(), "{pair:?}");
                }
            }
        }
    }
}
