//! Images use the same compact datum as every other generator.
use super::*;
pub(super) type ImageAgentDatumLayout = generator::layout::DatumLayout;

pub(super) fn image_agent_datum_layout(
    ctx: &egui::Context,
    inner: Rect,
    zoom: f32,
    model: &str,
) -> ImageAgentDatumLayout {
    generator::layout::datum_layout(ctx, inner, zoom, model, Some("…"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_image_datum_stays_in_one_row_and_keeps_fixed_targets() {
        let ctx = egui::Context::default();
        let _ = ctx.run(Default::default(), |_| {});
        for width in [160.0, 220.0, 360.0, 420.0, 900.0] {
            let panel = Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(width, 100.0));
            let inner = Rect::from_min_size(
                panel.min + Vec2::new(8.0, 2.0),
                Vec2::new(width - 16.0, 0.0),
            );
            for label in [
                "Auto",
                "A deliberately very long model name that cannot fit",
            ] {
                let d = image_agent_datum_layout(&ctx, inner, 1.0, label);
                let boxes = [d.model, d.settings.unwrap(), d.count, d.play, d.fold];
                for (index, a) in boxes.iter().enumerate() {
                    assert!(panel.contains_rect(*a));
                    for b in boxes.iter().skip(index + 1) {
                        assert!(!a.intersects(*b), "width={width}: {a:?} and {b:?}");
                    }
                }
                assert_eq!(d.bottom - inner.top(), 24.0);
                assert_eq!(d.settings.unwrap().left() - d.model.right(), 1.0);
            }
        }
    }
}
