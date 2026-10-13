use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ImageAgentDatumLayout {
    pub(super) model: Rect,
    pub(super) aspect: Rect,
    pub(super) mode: Rect,
    pub(super) settings: Rect,
    pub(super) count: Rect,
    pub(super) play: Rect,
    pub(super) history_y: f32,
    pub(super) controls_bottom: f32,
}

pub(super) fn image_agent_datum_layout(inner: Rect, zoom: f32) -> ImageAgentDatumLayout {
    let row_h = 24.0 * zoom;
    let gap = 6.0 * zoom;
    let top = Rect::from_min_size(inner.min, Vec2::new(inner.width(), row_h));
    if inner.width() / zoom.max(f32::EPSILON) < IMAGE_AGENT_NARROW_WIDTH {
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
        let third_top = second_top + row_h + gap;
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
        let settings_width = (inner.width() - mode.width() - gap).max(0.0);
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
            history_y: third_top + row_h * 0.5,
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
            history_y: top.center().y,
            controls_bottom: top.bottom(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_agent_datum_never_overlaps_controls_at_narrow_square_or_wide_sizes() {
        for panel_width in [160.0, 360.0, 900.0] {
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
            if panel_width < IMAGE_AGENT_NARROW_WIDTH + 24.0 {
                assert!(row.controls_bottom > inner.top() + 3.0 * 24.0);
            } else {
                assert_eq!(row.controls_bottom, inner.top() + 24.0);
            }
        }
    }
}
