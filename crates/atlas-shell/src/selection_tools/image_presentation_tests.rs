use super::*;

fn image() -> Rect {
    Rect::from_min_size(Pos2::new(80.0, 120.0), Vec2::new(640.0, 320.0))
}

#[test]
fn image_datum_touches_image_and_main_field_starts_after_datum_gap() {
    let layout = image_presentation_layout(image(), 1.0, ImagePanelKind::Color { stroke: true });
    assert_eq!(layout.technical.top(), image().bottom());
    assert_eq!(
        layout.main.top() - layout.technical.bottom(),
        IMAGE_PANEL_GAP
    );
    assert_eq!(layout.overall.left(), image().left());
    assert_eq!(layout.overall.right(), image().right());
}

#[test]
fn narrow_technical_datum_wraps_rgb_fields_into_a_nonoverlapping_second_row() {
    let image = Rect::from_min_size(Pos2::new(20.0, 40.0), Vec2::new(160.0, 100.0));
    let layout = image_presentation_layout(image, 1.0, ImagePanelKind::Color { stroke: false });
    assert_eq!(layout.technical.height(), IMAGE_DATUM_NARROW_HEIGHT);
    let fields = layout.technical_controls.rgb_values;
    for field in fields {
        assert!(layout.technical.contains_rect(field));
    }
    assert!(fields[0].right() < fields[1].left());
    assert!(fields[1].right() < fields[2].left());
    assert!(fields[0].center().y > layout.technical_controls.recent_colors[0].center().y);
}

#[test]
fn narrow_color_field_stacks_rails_below_the_gradient_without_overlap() {
    let layout = image_presentation_layout_with_choice(
        image(),
        1.0,
        ImagePanelKind::Color { stroke: true },
        ImageLayoutChoice::Narrow,
    );
    let color = layout.color.unwrap();
    assert!(!color.wide);
    assert_eq!(color.rail_count, 4);
    assert_eq!(color.field.width(), color.rails[0].width());
    assert!(color.field.bottom() <= color.rails[0].top());
    assert!(color.rails[0].bottom() <= color.rails[1].top());
    assert!(color.rails[1].bottom() <= color.rails[2].top());
    assert!(color.rails[2].bottom() <= color.rails[3].top());
}

#[test]
fn wide_color_columns_have_equal_rails_and_a_five_to_one_field() {
    let layout = image_presentation_layout_with_choice(
        image(),
        1.0,
        ImagePanelKind::Color { stroke: true },
        ImageLayoutChoice::Wide,
    );
    let color = layout.color.unwrap();
    assert!(color.wide);
    assert!((color.rails[0].width() - color.field.width()).abs() < 0.01);
    assert!((color.field.width() / color.field.height() - 5.0).abs() < 0.01);
    assert!(color.rails[0].right() < color.field.left());
    assert!(color.rails[0].top() >= layout.main.top());
    assert!(color.rails[3].bottom() <= layout.main.bottom());
}

#[test]
fn datum_and_field_geometry_scale_in_canvas_space() {
    for (width, height) in [(220.0, 300.0), (360.0, 360.0), (680.0, 320.0)] {
        let base_host = Rect::from_min_size(Pos2::new(80.0, 120.0), Vec2::new(width, height));
        let base =
            image_presentation_layout(base_host, 1.0, ImagePanelKind::Color { stroke: false });
        for zoom in [0.5, 2.0] {
            let scaled_host = Rect::from_min_size(base_host.min * zoom, base_host.size() * zoom);
            let scaled = image_presentation_layout(
                scaled_host,
                zoom,
                ImagePanelKind::Color { stroke: false },
            );
            assert_eq!(scaled.color.unwrap().wide, base.color.unwrap().wide);
            assert_eq!(scaled.technical.height() / zoom, base.technical.height());
            let scaled_color = scaled.color.unwrap();
            let base_color = base.color.unwrap();
            assert_eq!(scaled_color.field.min / zoom, base_color.field.min);
            assert_eq!(scaled_color.field.size() / zoom, base_color.field.size());
            for (scaled_rail, base_rail) in scaled_color.rails.iter().zip(base_color.rails) {
                assert_eq!(scaled_rail.min / zoom, base_rail.min);
                assert_eq!(scaled_rail.size() / zoom, base_rail.size());
            }
            for (scaled, original) in scaled
                .technical_controls
                .panel_buttons
                .iter()
                .zip(base.technical_controls.panel_buttons)
            {
                assert_eq!(scaled.size() / zoom, original.size());
                assert_eq!(scaled.center() / zoom, original.center());
            }
            for (scaled, original) in scaled
                .technical_controls
                .recent_colors
                .iter()
                .zip(base.technical_controls.recent_colors)
            {
                assert_eq!(scaled.size() / zoom, original.size());
                assert_eq!(scaled.center() / zoom, original.center());
            }
            for (scaled, original) in scaled
                .technical_controls
                .rgb_values
                .iter()
                .zip(base.technical_controls.rgb_values)
            {
                assert_eq!(scaled.size() / zoom, original.size());
                assert_eq!(scaled.center() / zoom, original.center());
            }
        }
    }
}

#[test]
fn corner_editor_keeps_equal_content_insets_at_each_canvas_scale() {
    for width in [220.0, 360.0, 680.0] {
        for zoom in [0.5, 1.0, 2.0] {
            let host = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 320.0) * zoom);
            let layout = image_presentation_layout(host, zoom, ImagePanelKind::Corners);
            let editor = layout.editor_rect(zoom);
            assert_eq!(
                (editor.left() - layout.main.left()) / zoom,
                IMAGE_PANEL_INSET
            );
            assert_eq!(
                (layout.main.right() - editor.right()) / zoom,
                IMAGE_PANEL_INSET
            );
            assert_eq!(
                (layout.main.bottom() - editor.bottom()) / zoom,
                IMAGE_PANEL_INSET
            );
            assert!((editor.height() / zoom - super::super::CORNER_HEIGHT).abs() < 0.01);
        }
    }
}
