use super::*;

fn contains_circle(host: Rect, center: Pos2, radius: f32) {
    assert!(
        host.contains_rect(Rect::from_center_size(center, Vec2::splat(radius * 2.0))),
        "host={host:?}, center={center:?}, radius={radius}"
    );
}

#[test]
fn image_panels_without_color_controls_reserve_no_empty_color_datum() {
    for width in [160.0, 360.0, 900.0] {
        let image = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 300.0));
        for kind in [
            ImagePanelKind::Filters,
            ImagePanelKind::Custom { main_height: 150.0 },
        ] {
            let layout = image_presentation_layout(image, 1.0, kind);
            assert_eq!(layout.main.top(), image.bottom());
            assert_eq!(layout.overall.height(), layout.main.height());
        }
    }
}

#[test]
fn image_color_fields_match_approved_aspects_across_host_shapes() {
    for (width, height, expected_wide, expected_aspect) in [
        (220.0, 300.0, false, 6.0),
        (360.0, 360.0, false, 6.0),
        (680.0, 320.0, true, 5.0),
        (900.0, 900.0, false, 6.0),
    ] {
        let host = Rect::from_min_size(Pos2::new(30.0, 40.0), Vec2::new(width, height));
        let layout = image_presentation_layout(host, 1.0, ImagePanelKind::Color { stroke: false });
        let color = layout.color.expect("color field geometry");
        assert_eq!(color.wide, expected_wide, "host={width}x{height}");
        let aspect = color.field.width() / color.field.height();
        assert!(
            (aspect - expected_aspect).abs() < 0.01,
            "host={width}x{height}: {aspect}"
        );
        assert_eq!(color.rail_count, 3);
        if expected_wide {
            assert_eq!(color.field.right(), layout.main.right() - IMAGE_PANEL_INSET);
            assert_eq!(
                color.rails[0].left(),
                layout.main.left() + IMAGE_PANEL_INSET
            );
        } else {
            assert_eq!(color.field.left(), color.rails[0].left());
            assert_eq!(color.field.right(), color.rails[0].right());
        }
    }
}

#[test]
fn wide_color_columns_are_equal_with_a_twelve_unit_gap() {
    let host = Rect::from_min_size(Pos2::ZERO, Vec2::new(680.0, 320.0));
    let layout = image_presentation_layout(host, 1.0, ImagePanelKind::Color { stroke: true });
    let color = layout.color.unwrap();
    assert!(color.wide);
    assert!((color.rails[0].width() - color.field.width()).abs() < 0.01);
    assert!((color.field.left() - color.rails[0].right() - 12.0).abs() < 0.01);
    assert!((color.field.width() / color.field.height() - 5.0).abs() < 0.01);
    for rail in color.rails.iter().take(4) {
        assert!(layout.main.contains_rect(*rail));
    }
}

#[test]
fn responsive_color_layout_is_stable_when_zoom_changes() {
    for zoom in [0.5, 1.0, 2.0] {
        let host =
            Rect::from_min_size(Pos2::new(40.0, 60.0) * zoom, Vec2::new(680.0, 320.0) * zoom);
        let layout = image_presentation_layout(host, zoom, ImagePanelKind::Color { stroke: false });
        let color = layout.color.unwrap();
        assert!(color.wide);
        assert!((color.field.width() / zoom - 322.0).abs() < 0.01);
        assert!((color.field.height() / zoom - 64.4).abs() < 0.01);
    }
    let square = Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 900.0));
    assert!(
        !image_presentation_layout(square, 1.0, ImagePanelKind::Color { stroke: false })
            .color
            .unwrap()
            .wide
    );
}

#[test]
fn brush_tools_and_color_controls_share_one_datum_above_three_rails() {
    for width in [220.0, 360.0, 680.0] {
        let host = Rect::from_min_size(Pos2::new(30.0, 40.0), Vec2::new(width, 300.0));
        let layout = image_presentation_layout(host, 1.0, ImagePanelKind::Brush);
        let header = layout.brush_header.expect("brush parameter strip");
        let color = layout.color.expect("brush shares the color editor");
        assert_eq!(header.top(), host.bottom());
        assert_eq!(layout.technical.top(), header.top());
        assert_eq!(layout.technical.bottom(), header.bottom());
        assert_eq!(layout.overall.top(), host.bottom());
        assert_eq!(color.rail_count, 3);
        assert_eq!(
            layout.technical_controls.eyedropper.left(),
            header.right() + 6.0
        );
        if color.wide {
            assert_eq!(color.field.right(), layout.main.right() - IMAGE_PANEL_INSET);
        } else {
            assert_eq!(color.field.left(), layout.main.left() + IMAGE_PANEL_INSET);
        }
    }
}

#[test]
fn brush_datum_controls_never_overlap_or_leave_their_host_at_any_review_zoom() {
    for width in [220.0, 360.0, 680.0] {
        for zoom in [0.5, 1.0, 2.0] {
            let host =
                Rect::from_min_size(Pos2::new(30.0, 40.0) * zoom, Vec2::new(width, 300.0) * zoom);
            let layout = image_presentation_layout(host, zoom, ImagePanelKind::Brush);
            let header = layout.brush_header.unwrap();
            assert_eq!(layout.overall.left(), host.left());
            assert_eq!(layout.overall.right(), host.right());
            assert!((layout.technical.height() / zoom - IMAGE_DATUM_HEIGHT).abs() < 0.001);
            let controls = &layout.technical_controls;
            let mut hits = super::image_brush::image_brush_header_groups(header, zoom).to_vec();
            hits.push(controls.eyedropper);
            hits.extend(
                controls
                    .recent_colors
                    .iter()
                    .copied()
                    .filter(|rect| rect.is_positive()),
            );
            hits.extend(controls.rgb_values);
            for (index, hit) in hits.iter().enumerate() {
                assert!(hit.is_positive(), "width={width}, zoom={zoom}: {hit:?}");
                assert!(
                    layout.technical.contains_rect(*hit),
                    "width={width}, zoom={zoom}: {hit:?}"
                );
                for other in hits.iter().skip(index + 1) {
                    assert!(
                        !hit.intersects(*other),
                        "width={width}, zoom={zoom}: {hit:?} overlaps {other:?}"
                    );
                }
            }
            assert!((controls.eyedropper.left() - header.right() - 6.0 * zoom).abs() < 0.001);
        }
    }
}

#[test]
fn image_filter_geometry_stays_inside_narrow_square_and_wide_hosts() {
    for (width, height) in [(160.0, 120.0), (360.0, 360.0), (900.0, 600.0)] {
        let host = Rect::from_min_size(Pos2::new(30.0, 40.0), Vec2::new(width, height));
        let (centers, radius, track) =
            filter_capsule_geometry(host, 6, 1.0, FilterCapsuleStyle::ImageWithIntensity);
        assert_eq!(centers.len(), 6);
        for (index, center) in centers.iter().copied().enumerate() {
            contains_circle(host, center, radius);
            if let Some(next) = centers.get(index + 1) {
                assert!(!Rect::from_center_size(center, Vec2::splat(radius * 2.0))
                    .intersects(Rect::from_center_size(*next, Vec2::splat(radius * 2.0))));
            }
        }
        let track = track.expect("intensity track");
        assert!(host.contains_rect(track));
        assert!(centers.iter().all(|center| {
            !Rect::from_center_size(*center, Vec2::splat(radius * 2.0)).intersects(track)
        }));
    }
}

#[test]
fn image_layer_controls_page_and_fit_inside_narrow_square_and_wide_hosts() {
    for (width, height) in [(160.0, 140.0), (360.0, 360.0), (900.0, 600.0)] {
        let image = Rect::from_min_size(Pos2::new(25.0, 35.0), Vec2::new(width, height));
        let presentation = image_presentation_layout(image, 1.0, ImagePanelKind::Layers);
        let layout = image_layer_palette_layout(&presentation, 30, 6, 1.0, Some(17));
        let header = presentation.layer_header.expect("Layers has a top datum");
        let host = presentation.overall;
        assert_eq!(header.top(), image.bottom());
        assert_eq!(header.height(), 64.0);
        assert_eq!(presentation.main.top(), header.bottom());
        assert_eq!(presentation.main.height(), 32.0);
        assert!(
            layout.colors.is_empty(),
            "image Layers excludes recent colors"
        );
        assert!(host.contains_rect(layout.capsule));
        assert!(layout.radius >= 14.0, "width={width}: {}", layout.radius);
        assert!((layout.add.width() - 2.0 * layout.radius).abs() < 0.01);
        assert!((layout.add.height() - layout.add.width()).abs() < 0.01);
        for center in &layout.layers {
            contains_circle(header, *center, layout.radius);
        }
        for center in &layout.colors {
            contains_circle(host, *center, layout.color_radius * 1.2);
        }
        for control in [layout.track, layout.opacity_value, layout.add] {
            assert!(host.contains_rect(control), "width={width}: {control:?}");
        }
        assert!(header.contains_rect(layout.add));
        assert!(presentation.main.contains_rect(layout.track));
        assert!(presentation.main.contains_rect(layout.opacity_value));
        let mut hit_rects: Vec<Rect> = layout
            .layers
            .iter()
            .map(|center| Rect::from_center_size(*center, Vec2::splat(layout.radius * 2.0)))
            .chain(layout.colors.iter().map(|center| {
                Rect::from_center_size(*center, Vec2::splat(layout.color_radius * 2.4))
            }))
            .chain([layout.track, layout.opacity_value, layout.add])
            .collect();
        if layout.page_count > 1 {
            assert!(host.contains_rect(layout.page_prev));
            assert!(host.contains_rect(layout.page_next));
            assert!(layout.layer_indices.contains(&17));
            hit_rects.extend([layout.page_prev, layout.page_next]);
        }
        for (index, rect) in hit_rects.iter().enumerate() {
            for other in hit_rects.iter().skip(index + 1) {
                assert!(
                    !rect.intersects(*other),
                    "width={width}: {rect:?} {other:?}"
                );
            }
        }
    }
}
