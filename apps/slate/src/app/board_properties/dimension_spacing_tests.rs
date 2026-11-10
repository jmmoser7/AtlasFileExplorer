//! Check the production paint output, including the image-specific strip route.
use super::*;
use crate::app::tests::{capture_frame, rasterize, FrameRaster, Harness};

fn board(name: &str, dark: bool) -> Harness {
    let mut h = Harness::new(name);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.dark_mode = dark;
    h.app.tab_mut().cam.offset = Vec2::ZERO;
    h
}

fn media(h: &mut Harness, name: &str, size: Vec2) -> NodeId {
    let id = crate::app::board_flow::tests::place(h, name, 0.0);
    h.app.patch_nodes(&[id], |node| {
        node.rect = WorldRect::new(-size.x * 0.5, -size.y * 0.5, size.x, size.y);
    });
    id
}

fn native_text(h: &mut Harness, size: Vec2) -> NodeId {
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-size.x * 0.5, -size.y * 0.5, size.x, size.y),
        NodeKind::Text(slate_doc::scene::TextNode {
            answers: Default::default(),
            text: "Generated text fixture\n\nA local dimension audit. No model request is sent."
                .into(),
            style: Default::default(),
            family: Default::default(),
            size: 10.0,
            color: Rgba::BLACK,
            align: Default::default(),
            fill: Some(Rgba::WHITE),
            stroke: scene::Stroke::none(),
            agent: None,
        }),
    );
    h.app.add_nodes(vec![node])[0]
}

fn frame(h: &mut Harness, time: f64) -> egui::FullOutput {
    h.frame_output(|input| {
        input.time = Some(time);
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(2200.0, 1500.0)));
    })
}

fn endpoint(shape: &egui::Shape, center: Pos2, radius: f32) -> bool {
    match shape {
        egui::Shape::Circle(circle) => {
            circle.center.distance(center) < 0.02 && (circle.radius - radius).abs() < 0.02
        }
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| endpoint(shape, center, radius)),
        _ => false,
    }
}

fn assert_spacing(h: &Harness, output: &egui::FullOutput, external: bool) {
    let state = h.app.shape_properties.last_chrome.as_ref().unwrap();
    assert_eq!(
        !state.image && !state.blister && !state.items.is_empty(),
        external,
        "the expected presentation must be exercised"
    );
    let xf = h.app.board_xf();
    assert_eq!(h.app.shape_properties.dimensions.len(), 2);
    for dimension in &h.app.shape_properties.dimensions {
        let normal = dimension.offset.normalized();
        let downward = normal.y > 0.0 && normal.y.abs() + 0.0001 >= normal.x.abs();
        let gap = if external && downward { 68.0 } else { 24.0 };
        for end in dimension.ends {
            let expected = xf.w2s(end + normal * gap);
            assert!(
                output
                    .shapes
                    .iter()
                    .any(|entry| endpoint(&entry.shape, expected, 1.8 * xf.z)),
                "painted {:?} endpoint must be {gap} board units outside its host: \
                 external={external}, zoom={}, expected={expected:?}",
                dimension.kind,
                xf.z
            );
        }
    }
}

#[test]
fn dimension_spacing_tracks_media_presentation_in_both_themes_and_at_every_zoom() {
    for dark in [false, true] {
        for kind in [
            "document.txt",
            "document.pdf",
            "photo.png",
            "movie.mp4",
            "native-text",
        ] {
            for (size, attached) in [
                (Vec2::new(680.0, 380.0), true),
                (Vec2::new(180.0, 135.0), false),
            ] {
                let mut h = board(&format!("dimension-spacing-{kind}-{dark}-{attached}"), dark);
                let id = if kind == "native-text" {
                    native_text(&mut h, size)
                } else {
                    media(&mut h, kind, size)
                };
                h.app.board_sel = [id].into_iter().collect();
                for (step, zoom) in [0.5, 1.0, 2.0].into_iter().enumerate() {
                    h.app.tab_mut().cam.z = zoom;
                    frame(&mut h, step as f64 + 1.0);
                    let output = frame(&mut h, step as f64 + 1.5);
                    // Ordinary Text now shares the generic media blister:
                    // large hosts attach, while small hosts use the external
                    // strip. Both routes retain their board-space offsets.
                    assert_spacing(&h, &output, !attached);
                }
            }
        }
    }
}

#[test]
fn dimension_spacing_refreshes_after_resize_rotation_and_selection_changes() {
    let mut h = board("dimension-spacing-live-transitions", false);
    let id = media(&mut h, "photo.png", Vec2::new(900.0, 600.0));
    let other = media(&mut h, "document.txt", Vec2::new(180.0, 135.0));
    h.app.tab_mut().cam.z = 0.7;
    h.app.board_sel = [id].into_iter().collect();
    for (step, (rotation, external)) in [(0.0, false), (45.0, true), (90.0, false)]
        .into_iter()
        .enumerate()
    {
        h.app
            .patch_nodes(&[id], |node| node.rotation_deg = rotation);
        frame(&mut h, step as f64 + 1.0);
        let output = frame(&mut h, step as f64 + 1.5);
        assert_spacing(&h, &output, external);
    }
    h.app.patch_nodes(&[id], |node| {
        node.rotation_deg = 0.0;
        node.rect = WorldRect::new(-90.0, -67.5, 180.0, 135.0);
    });
    frame(&mut h, 5.0);
    let output = frame(&mut h, 5.5);
    assert_spacing(&h, &output, true);
    h.app.patch_nodes(&[id], |node| {
        node.rect = WorldRect::new(-450.0, -300.0, 900.0, 600.0);
    });
    for (step, (selected, external)) in [(id, false), (other, true), (id, false)]
        .into_iter()
        .enumerate()
    {
        h.app.board_sel = [selected].into_iter().collect();
        frame(&mut h, step as f64 + 7.0);
        let output = frame(&mut h, step as f64 + 7.5);
        assert_spacing(&h, &output, external);
    }
}

fn click(h: &mut Harness, point: Pos2, time: f64) {
    // Inline-number hit regions follow the pointer; egui routes the next press
    // through the widget geometry registered by this hover frame.
    h.frame_with(|input| {
        input.time = Some(time - 0.05);
        input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(2200.0, 1500.0)));
        input.events.push(egui::Event::PointerMoved(point));
    });
    for (step, pressed) in [true, false].into_iter().enumerate() {
        h.frame_with(|input| {
            input.time = Some(time + step as f64 * 0.05);
            input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(2200.0, 1500.0)));
            input.events.extend([
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        });
    }
}

#[test]
fn compact_dimension_spacing_keeps_numeric_editing_attached_after_pan_and_zoom() {
    for dark in [false, true] {
        let mut h = board(&format!("dimension-spacing-edit-{dark}"), dark);
        let id = media(&mut h, "document.txt", Vec2::new(384.4, 288.3));
        h.app.board_sel = [id].into_iter().collect();
        h.app.tab_mut().cam.z = 1.0;
        frame(&mut h, 1.0);
        let output = frame(&mut h, 1.5);
        assert_spacing(&h, &output, false);
        let dimension = h.app.shape_properties.dimensions[0].clone();
        let midpoint = dimension.ends[0].lerp(dimension.ends[1], 0.5);
        let normal = dimension.offset.normalized();
        let old_point = h.app.board_xf().w2s(midpoint + normal * 68.0);
        click(&mut h, old_point, 2.0);
        assert!(
            h.app.shape_properties.number.is_none(),
            "the retired 68-unit lane has no editor"
        );
        h.app.board_sel = [id].into_iter().collect();
        frame(&mut h, 3.0);
        frame(&mut h, 3.5);
        let undo_depth = h.app.tab().journal.undo_depth();
        let point = h.app.board_xf().w2s(midpoint + normal * 24.0);
        click(&mut h, point, 4.0);
        assert!(
            h.app.shape_properties.number.is_some(),
            "the compact lane accepts real clicks"
        );
        h.app.tab_mut().cam.z = 1.7;
        h.app.tab_mut().cam.offset += Vec2::new(30.0, 15.0);
        h.frame_with(|input| {
            input.time = Some(5.0);
            input.events.push(egui::Event::Text("500".into()));
        });
        h.frame_with(|input| {
            input.time = Some(5.5);
            input.events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        });
        assert!(h.app.shape_properties.number.is_none());
        let node = h.app.doc().scene.node(id).unwrap();
        assert_eq!(node.rect.w, 500.0);
        assert!((node.rect.h - 288.3).abs() < 0.001);
        assert_eq!(node.rect.center(), (0.0, 0.0));
        assert_eq!(h.app.tab().journal.undo_depth(), undo_depth + 1);
    }
}

#[test]
fn dimension_spacing_keeps_mixed_selection_outside_the_external_strip() {
    let mut h = board("dimension-spacing-mixed-selection", false);
    let photo = media(&mut h, "photo.png", Vec2::new(680.0, 380.0));
    let document = media(&mut h, "document.txt", Vec2::new(180.0, 135.0));
    h.app.patch_nodes(&[document], |node| node.rect.x = 400.0);
    h.app.board_sel = [photo, document].into_iter().collect();
    h.app.tab_mut().cam.z = 0.7;
    frame(&mut h, 1.0);
    let output = frame(&mut h, 1.5);
    assert_spacing(&h, &output, true);
}

#[test]
#[ignore = "writes full production Slate frames for visual review"]
fn capture_dimension_spacing_in_both_themes() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/dimension-spacing");
    std::fs::create_dir_all(&dir).unwrap();
    for (theme, dark) in [("light", false), ("dark", true)] {
        for (presentation, size, zoom) in [
            ("blister", Vec2::new(384.4, 288.3), 2.0),
            ("external", Vec2::new(226.285, 169.714), 3.0),
        ] {
            let mut h = board(
                &format!("dimension-spacing-capture-{theme}-{presentation}"),
                dark,
            );
            let id = media(&mut h, "dimension-fixture.txt", size);
            std::fs::write(
                h.base.join("dimension-fixture.txt"),
                "Generated text fixture\n\nA local dimension audit. No model request is sent.",
            )
            .unwrap();
            h.app.board_sel = [id].into_iter().collect();
            h.app.tab_mut().cam.z = zoom;
            if presentation == "external" {
                h.app.tab_mut().cam.offset.y = 40.0;
            }
            let mut raster = FrameRaster::new(1440, 900);
            capture_frame(&mut h, &mut raster, |input| input.time = Some(1.0));
            capture_frame(&mut h, &mut raster, |input| input.time = Some(1.25));
            let output = capture_frame(&mut h, &mut raster, |input| input.time = Some(1.5));
            assert_spacing(&h, &output, presentation == "external");
            raster.sync_fonts(&h.ctx);
            rasterize(&mut h, &mut raster, output);
            raster.save(&dir.join(format!("{theme}-{presentation}.png")));
        }
    }
}
