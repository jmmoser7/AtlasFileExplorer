use super::*;

#[test]
fn small_panoramic_and_round_images_route_to_external_squircles_at_every_zoom() {
    let cases = [
        ("small", 180.0, 180.0, 0.0, false),
        ("panorama", 520.0, 60.0, 0.0, false),
        ("round", 360.0, 360.0, 180.0, false),
        ("blister", 360.0, 240.0, 0.0, true),
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
            frame_with_events(&mut h, &mut time, vec![egui::Event::PointerMoved(point)]);
            frame_with_events(
                &mut h,
                &mut time,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            if let Some(other) = other {
                assert!(
                    h.app.shape_properties.panel.is_none(),
                    "the topmost object is not the image host"
                );
                assert_eq!(h.app.board_sel, std::iter::once(other).collect());
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
