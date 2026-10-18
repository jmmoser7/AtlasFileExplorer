use super::*;

#[test]
fn small_panoramic_and_round_images_route_to_external_squircles_at_every_zoom() {
    let cases = [
        ("small", 180.0, 180.0, 0.0, false),
        ("panorama", 520.0, 60.0, 0.0, false),
        ("round", 360.0, 360.0, 180.0, false),
        ("insufficient-width", 360.0, 240.0, 0.0, false),
        ("blister", 680.0, 380.0, 0.0, true),
    ];
    for (name, width, height, radius, attached) in cases {
        let mut h = Harness::new(&format!("image-palette-route-{name}"));
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.set_board_tool(BoardTool::Select);
        let path = h.base.join("photo.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([100, 120, 140, 255]))
            .save(&path)
            .unwrap();
        let item = h.app.add_paths(&[path])[0];
        let mut image = scene::ImageNode::new(item);
        image.corner = scene::Corner::Rounded { radius };
        let node = h.app.doc_mut().scene.build_node(
            WorldRect::new(-width * 0.5, -height * 0.5, width, height),
            NodeKind::Image(image),
        );
        let id = h.app.add_nodes(vec![node])[0];
        h.app.board_sel = std::iter::once(id).collect();
        for zoom in [0.25, 0.5, 1.0, 2.0] {
            h.app.tab_mut().cam.z = zoom;
            h.frame();
            h.frame();
            let chrome = h
                .app
                .shape_properties
                .last_chrome
                .as_ref()
                .expect("selected host has a property strip");
            assert_eq!(chrome.image, attached, "{name} at zoom {zoom}");
            if !attached {
                assert!(
                    !chrome.blister,
                    "fallback must not use the generic portal blister"
                );
            }
        }
    }
}

#[test]
fn fallback_image_body_inspection_retains_the_editor_including_locked_hosts() {
    for locked in [false, true] {
        for covered in [false, true] {
            let mut h = Harness::new(&format!("image-fallback-body-{locked}-{covered}"));
            h.app.leave_home();
            h.app.ensure_work_tab();
            h.app.set_board_tool(BoardTool::Select);
            h.frame();
            let id = crate::app::board_flow::tests::place(&mut h, "photo.png", 0.0);
            h.app.patch_nodes(&[id], |node| {
                node.rect = WorldRect::new(-90.0, -90.0, 180.0, 180.0);
                node.locked = locked;
            });
            let other = covered.then(|| {
                crate::app::board_properties::tests::rectangle(
                    &mut h,
                    WorldRect::new(-20.0, -20.0, 40.0, 40.0),
                    0.0,
                )
            });
            // Force selection also permits inspection of a locked host.
            h.app.board_sel = std::iter::once(id).collect();
            let mut time = h.ctx.input(|i| i.time);
            frame(&mut h, &mut time);
            frame(&mut h, &mut time);
            h.app.shape_properties.panel = Some(Panel::Fill);
            for _ in 0..12 {
                frame(&mut h, &mut time);
            }
            let chrome = h.app.shape_properties.last_chrome.as_ref().unwrap();
            assert!(
                !chrome.image && !chrome.blister,
                "exercise external image controls"
            );
            let host = h
                .app
                .board_xf()
                .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
            let point = host.center();
            assert!(h.app.canvas_rect.contains(point));
            assert!(
                !h.app
                    .shape_properties
                    .chrome_hits
                    .iter()
                    .any(|hit| hit.contains(point)),
                "the press is on the picture body, not its controls"
            );
            crate::app::tests::click_board(&mut h, point);
            if let Some(other) = other {
                h.frame();
                assert_eq!(h.app.board_sel, std::iter::once(other).collect());
                assert!(
                    h.app.shape_properties.panel.is_none(),
                    "the synchronized selection is no longer the image host"
                );
                assert_eq!(
                    h.app
                        .shape_properties
                        .last_chrome
                        .as_ref()
                        .and_then(|chrome| chrome.host),
                    Some((h.app.tab().id, other)),
                    "the synchronized property strip belongs to the topmost object"
                );
            } else {
                assert_eq!(
                    h.app.shape_properties.panel,
                    Some(Panel::Fill),
                    "body inspection retains the local editor, locked={locked}"
                );
                assert_eq!(h.app.board_sel, std::iter::once(id).collect());
            }
        }
    }
}

#[test]
fn fallback_images_keep_external_brush_and_layers_controls_at_full_width() {
    for (name, width, height, rotation) in [
        ("360", 360.0, 100.0, 0.0),
        ("400", 400.0, 100.0, 0.0),
        ("rotated", 360.0, 240.0, 24.0),
    ] {
        let mut h = Harness::new(&format!("fallback-external-editors-{name}"));
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.set_board_tool(BoardTool::Select);
        let id = crate::app::board_flow::tests::place(&mut h, "photo.png", 0.0);
        h.app.patch_nodes(&[id], |node| {
            node.rect = WorldRect::new(-width * 0.5, -height * 0.5, width, height);
            node.rotation_deg = rotation;
        });
        h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
        h.app.tab_mut().cam.z = 1.0;
        h.app.board_sel = std::iter::once(id).collect();
        h.frame();

        let mut time = h.ctx.input(|input| input.time);
        for _ in 0..12 {
            frame(&mut h, &mut time);
        }
        let state = h.app.shape_properties.last_chrome.as_ref().unwrap();
        assert!(
            !state.image && !state.blister,
            "{name} stays on external chrome"
        );
        assert!(state.items.contains(&StripItem::Panel(Panel::Brush)));
        assert!(state.items.contains(&StripItem::Panel(Panel::Layers)));

        click_external_image_panel(&mut h, &mut time, Panel::Brush);
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Brush), "{name}");
        for _ in 0..12 {
            frame(&mut h, &mut time);
        }
        let (attached, brush_layout, _) =
            h.app
                .observed_image_palette_layout_for_test(&h.ctx, id, chrome::ImagePanelKind::Brush);
        assert!(!attached, "{name} Brush remains external");
        let brush_header = brush_layout
            .brush_header
            .expect("Brush header in shared layout");
        assert!(
            h.app
                .shape_properties
                .chrome_hits
                .iter()
                .any(|hit| hit.intersects(brush_header)),
            "the external Brush body paints its visible header, {name}"
        );

        click_external_image_panel(&mut h, &mut time, Panel::Layers);
        assert_eq!(h.app.shape_properties.panel, Some(Panel::Layers), "{name}");
        for _ in 0..12 {
            frame(&mut h, &mut time);
        }
        let (attached, layers_layout, _) = h.app.observed_image_palette_layout_for_test(
            &h.ctx,
            id,
            chrome::ImagePanelKind::Layers,
        );
        assert!(!attached, "{name} Layers remains external");
        let layer_layout = chrome::image_layer_palette_layout(
            &layers_layout,
            h.app.image_paint_layer_choices(id).len(),
            0,
            h.app.board_xf().z,
            Some(0),
        );
        assert!(
            h.app
                .shape_properties
                .chrome_hits
                .iter()
                .any(|hit| hit.contains(layer_layout.add.center())),
            "the external Layers body paints its Add control, {name}"
        );
        let before = h.app.image_paint_layer_choices(id).len();
        let undo_before = h.app.tab().journal.undo_depth();
        click_point(&mut h, &mut time, layer_layout.add.center());
        assert_eq!(
            h.app.image_paint_layer_choices(id).len(),
            before + 1,
            "the visible Add control creates a layer, {name}"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), undo_before + 1, "{name}");
        h.app.board_undo();
        assert_eq!(
            h.app.image_paint_layer_choices(id).len(),
            before,
            "undo removes the layer created by the visible control, {name}"
        );
        if name == "360" {
            h.app.patch_nodes(&[id], |node| node.locked = true);
            for _ in 0..2 {
                frame(&mut h, &mut time);
            }
            let (attached, locked_layout, _) = h.app.observed_image_palette_layout_for_test(
                &h.ctx,
                id,
                chrome::ImagePanelKind::Layers,
            );
            assert!(!attached, "{name} locked Layers remains external");
            let add = chrome::image_layer_palette_layout(
                &locked_layout,
                before,
                0,
                h.app.board_xf().z,
                Some(0),
            )
            .add;
            let undo_locked = h.app.tab().journal.undo_depth();
            click_point(&mut h, &mut time, add.center());
            assert_eq!(h.app.image_paint_layer_choices(id).len(), before);
            assert_eq!(h.app.tab().journal.undo_depth(), undo_locked);
        }
    }
}

fn click_external_image_panel(h: &mut Harness, time: &mut f64, panel: Panel) {
    let xf = h.app.board_xf();
    let state = h.app.shape_properties.last_chrome.as_ref().unwrap();
    let index = state
        .items
        .iter()
        .position(|item| *item == StripItem::Panel(panel))
        .expect("external strip includes the requested image control");
    let (attached, _, strip) = h.app.observed_image_palette_layout_for_test(
        &h.ctx,
        state.host.expect("single image host").1,
        image_palette::panel_kind(h.app.shape_properties.panel),
    );
    assert!(
        !attached,
        "requested control is on the observed external strip"
    );
    let rect = chrome::strip_button_rect(strip, index, xf.z);
    click_point(h, time, rect.center());
}

fn click_point(h: &mut Harness, time: &mut f64, point: Pos2) {
    frame_with_events(h, time, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] {
        frame_with_events(
            h,
            time,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
}
