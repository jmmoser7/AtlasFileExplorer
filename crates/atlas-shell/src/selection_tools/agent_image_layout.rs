use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ImageAgentDatumLayout {
    pub(super) model: Rect,
    pub(super) aspect: Rect,
    pub(super) mode: Rect,
    pub(super) settings: Rect,
    pub(super) count: Rect,
    pub(super) play: Rect,
    pub(super) history: Rect,
    pub(super) controls_bottom: f32,
}

/// Control rows depend on the host's board-space width, never its zoom.
pub(super) fn image_agent_datum_rows(width_board_units: f32) -> usize {
    if width_board_units >= IMAGE_AGENT_NARROW_WIDTH + 24.0 {
        1
    } else if width_board_units >= 220.0 {
        2
    } else {
        3
    }
}

pub(super) fn image_agent_datum_layout(inner: Rect, zoom: f32) -> ImageAgentDatumLayout {
    let row_h = 24.0 * zoom;
    let gap = 6.0 * zoom;
    let top = Rect::from_min_size(inner.min, Vec2::new(inner.width(), row_h));
    let rows = image_agent_datum_rows(inner.width() / zoom.max(f32::EPSILON) + 24.0);
    if rows > 1 {
        let model_width = (inner.width() - gap) * 0.58;
        let model = Rect::from_min_size(top.min, Vec2::new(model_width, row_h));
        let aspect = Rect::from_min_size(
            Pos2::new(model.right() + gap, top.top()),
            Vec2::new((inner.right() - model.right() - gap).max(0.0), row_h),
        );
        let second_top = top.bottom() + gap;
        let mode = Rect::from_min_size(
            Pos2::new(inner.left(), second_top),
            Vec2::new(50.0 * zoom, row_h),
        );
        let count_width = 32.0 * zoom;
        let play_width = 20.0 * zoom;
        let third_top = if rows == 2 {
            second_top
        } else {
            second_top + row_h + gap
        };
        let play = Rect::from_min_size(
            Pos2::new(inner.right() - play_width, third_top),
            Vec2::splat(play_width),
        );
        let count_right = play.left() - gap;
        let count_width = count_width.min((count_right - inner.left()).max(0.0));
        let count = Rect::from_min_size(
            Pos2::new(count_right - count_width, third_top),
            Vec2::new(count_width, row_h),
        );
        let settings_right = if rows == 2 {
            count.left() - gap
        } else {
            inner.right()
        };
        let settings_width = (76.0 * zoom).min((settings_right - mode.right() - gap).max(0.0));
        let settings = Rect::from_min_size(
            Pos2::new(mode.right() + gap, second_top),
            Vec2::new(settings_width, row_h),
        );
        ImageAgentDatumLayout {
            model,
            aspect,
            mode,
            settings,
            count,
            play,
            history: Rect::from_min_max(
                Pos2::new(
                    if rows == 2 {
                        settings.right() + gap
                    } else {
                        inner.left()
                    },
                    third_top - zoom,
                ),
                Pos2::new(
                    (count.left() - gap).max(if rows == 2 {
                        settings.right() + gap
                    } else {
                        inner.left()
                    }),
                    third_top + 25.0 * zoom,
                ),
            ),
            controls_bottom: third_top + row_h,
        }
    } else {
        let mut x = top.left();
        let model_width = (inner.width() * 0.27).min(126.0 * zoom);
        let model = Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(model_width, row_h));
        x = model.right() + gap;
        let ratio_width = (inner.width() * 0.18).min(82.0 * zoom);
        let aspect = Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(ratio_width, row_h));
        x = aspect.right() + gap;
        let mode = Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(50.0 * zoom, row_h));
        x = mode.right() + gap;
        let play_width = 20.0 * zoom;
        let count_width = 32.0 * zoom;
        let play = Rect::from_min_size(
            Pos2::new(top.right() - play_width, top.top()),
            Vec2::splat(play_width),
        );
        let count = Rect::from_min_size(
            Pos2::new(play.left() - gap - count_width, top.top()),
            Vec2::new(count_width, row_h),
        );
        let settings_width = (76.0 * zoom).min((count.left() - gap - x).max(0.0));
        let settings =
            Rect::from_min_size(Pos2::new(x, top.top()), Vec2::new(settings_width, row_h));
        ImageAgentDatumLayout {
            model,
            aspect,
            mode,
            settings,
            count,
            play,
            history: Rect::from_min_max(
                Pos2::new(settings.right() + gap, top.top() - zoom),
                Pos2::new(
                    (count.left() - gap).max(settings.right() + gap),
                    top.top() + 25.0 * zoom,
                ),
            ),
            controls_bottom: top.bottom(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_agent_datum_never_overlaps_controls_at_narrow_square_or_wide_sizes() {
        for (panel_width, rows) in [
            (160.0, 3),
            (219.0, 3),
            (220.0, 2),
            (360.0, 2),
            (384.0, 1),
            (900.0, 1),
        ] {
            let panel = Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(panel_width, 220.0));
            let inner = panel.shrink(12.0);
            let row = image_agent_datum_layout(inner, 1.0);
            for r in [
                row.model,
                row.aspect,
                row.mode,
                row.settings,
                row.count,
                row.play,
            ] {
                assert!(inner.contains_rect(r), "width={panel_width}: {r:?}");
            }
            let mut positive = [
                row.model,
                row.aspect,
                row.mode,
                row.settings,
                row.count,
                row.play,
            ]
            .into_iter()
            .filter(|r| r.width() > 0.0);
            let rects: Vec<_> = positive.by_ref().collect();
            for (i, left) in rects.iter().enumerate() {
                for right in rects.iter().skip(i + 1) {
                    assert!(
                        !left.intersects(*right),
                        "width={panel_width}: {left:?} {right:?}"
                    );
                }
            }
            assert_eq!(row.controls_bottom - inner.top(), rows as f32 * 30.0 - 6.0);
        }
    }
}
