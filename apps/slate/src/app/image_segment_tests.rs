//! Deterministic input scripts: mask inference is injected at the worker seam.
use super::*;
use crate::app::board::BoardTool;
use crate::app::tests::{capture_frame, rasterize, FrameRaster, Harness};
use slate_doc::scene::ImageNode;

#[test]
fn committed_segment_becomes_a_prompted_image_layer_region() {
    let (mut h, id) = fixture("segment_prompt_layer");
    let region = h
        .app
        .commit_image_segment(
            id,
            region_node(
                slate_doc::image_paint::region_path(vec![vec![
                    [0.2, 0.2],
                    [0.6, 0.2],
                    [0.6, 0.7],
                    [0.2, 0.7],
                ]])
                .unwrap(),
            ),
            Some("give this person a hat"),
        )
        .expect("segment committed");
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(img.paint_layers.len(), 1);
    assert_eq!(img.paint_layers[0].nodes[0].id, region);
    assert_eq!(img.paint_layers[0].prompts[0].node, region);
    assert_eq!(
        h.app.image_region_prompts(id),
        vec!["give this person a hat"]
    );
    h.app.board_undo();
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert!(img.paint_layers.is_empty());
}

fn fixture(name: &str) -> (Harness, NodeId) {
    let (mut h, id) = fixture_unpainted(name);
    h.frame();
    (h, id)
}

fn fixture_unpainted(name: &str) -> (Harness, NodeId) {
    let mut h = Harness::new(name);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.set_board_tool(BoardTool::Select);
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.tab_mut().cam.z = 1.0;
    let path = h.base.join("photo.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([100, 130, 160, 255]))
        .save(&path)
        .unwrap();
    let item = h.app.add_paths(&[path])[0];
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(-200.0, -160.0, 400.0, 320.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    (h, id)
}

fn contours() -> Vec<Vec<[f32; 2]>> {
    // Concave silhouette with an interior hole, impossible to render as a box.
    vec![
        vec![
            [0.15, 0.15],
            [0.8, 0.15],
            [0.8, 0.4],
            [0.5, 0.4],
            [0.5, 0.85],
            [0.15, 0.85],
        ],
        vec![[0.2, 0.3], [0.3, 0.3], [0.3, 0.4], [0.2, 0.4]],
    ]
}

fn inject(h: &mut Harness, id: NodeId) -> Pos2 {
    let world = Pos2::new(-50.0, -70.0);
    let screen = h.app.board_xf().w2s(world);
    let mask = contours();
    h.app.image_segments.hover = Some(Hover {
        tab: h.app.tab().id,
        host: id,
        gen: h.app.scene_gen,
        point_norm: [0.375, 0.28125],
        screen,
        since: Instant::now() - LINGER,
    });
    h.app.image_segments.result = Some(SegmentResult {
        host: id,
        gen: h.app.scene_gen,
        local: region_node(slate_doc::image_paint::region_path(mask.clone()).unwrap()),
        mask: std::sync::Arc::new(mask),
    });
    screen
}

fn move_to(h: &mut Harness, p: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
}

#[test]
fn segment_highlight_action_is_reachable_and_does_not_drag_the_photo() {
    let (mut h, id) = fixture("segment_action");
    let p = inject(&mut h, id);
    move_to(&mut h, p);
    let before = h.app.doc().scene.node(id).unwrap().rect;
    let target = h.app.image_segments.action_rect.unwrap().center();
    // Travel across the intervening empty canvas before pressing the capsule.
    for n in 1..=6 {
        move_to(&mut h, p.lerp(target, n as f32 / 6.0));
    }
    assert!(h.app.image_segments.result.is_some());
    for pressed in [true, false] {
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            })
        });
    }
    let node = h.app.doc().scene.node(id).unwrap();
    assert_eq!(node.rect, before, "capsule press never moves the photo");
    let NodeKind::Image(img) = &node.kind else {
        unreachable!()
    };
    assert_eq!(img.paint_layers.len(), 1);
    let NodeKind::Shape(shape) = &img.paint_layers[0].nodes[0].kind else {
        unreachable!()
    };
    assert_eq!(shape.fill, Some(HIGHLIGHT));
    assert!(
        shape.stroke.is_none(),
        "a highlight has no bounding-box stroke"
    );
    assert_eq!(
        shape.path.as_ref().unwrap().extra.len(),
        1,
        "hole survives commit"
    );
    h.app.board_undo();
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(img.paint_layers.is_empty());
}

#[test]
fn segment_dwell_tolerates_small_pointer_jitter_and_escape_stays_dismissed() {
    let (mut h, id) = fixture("segment_jitter");
    let p = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    move_to(&mut h, p);
    let since = h.app.image_segments.hover.as_ref().unwrap().since;
    for dx in [1.0, -2.0, 3.0] {
        move_to(&mut h, p + egui::vec2(dx, 0.0));
    }
    assert_eq!(h.app.image_segments.hover.as_ref().unwrap().since, since);
    h.frame_with(|i| {
        i.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        })
    });
    h.frame();
    assert!(h.app.image_segments.hover.is_none());
}

#[test]
fn segment_worker_results_cannot_resurrect_a_dismissed_or_replaced_hover() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut runtime = ImageSegmentRuntime {
        serial: 4,
        pending: Some(4),
        done: Some(rx),
        ..Default::default()
    };
    runtime.clear();
    tx.send((4, Ok(contours()))).unwrap();
    runtime.receive();
    assert!(runtime.result.is_none());
    runtime.pending = Some(6);
    runtime.serial = 6;
    tx.send((5, Err("stale failure".into()))).unwrap();
    runtime.receive();
    assert_eq!(runtime.pending, Some(6));
    assert!(runtime.error.is_none());
}

#[test]
fn segment_command_refuses_a_preview_from_another_tab() {
    let (mut h, id) = fixture("segment_stale_tab");
    inject(&mut h, id);
    h.app.image_segments.hover.as_mut().unwrap().tab ^= 1;
    assert!(h.app.commit_hover_segment(None).is_none());
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(img.paint_layers.is_empty());
}

#[test]
fn segment_preview_and_export_keep_the_silhouette_and_hole() {
    let (mut h, id) = fixture("segment_export");
    inject(&mut h, id);
    h.app.commit_hover_segment(None).unwrap();
    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    assert!(html.contains("evenodd"), "export keeps the mask hole");
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    let NodeKind::Shape(shape) = &img.paint_layers[0].nodes[0].kind else {
        unreachable!()
    };
    let path = shape.path.as_ref().unwrap();
    assert_eq!(
        path.segs.len(),
        5,
        "the concave silhouette was not replaced by a rectangle"
    );
}

#[test]
fn segment_visible_window_tracks_rotated_crop_coordinates() {
    let mut img = ImageNode::new(slate_doc::ItemId::NONE);
    img.crop = slate_doc::scene::Crop {
        x: 0.2,
        y: 0.1,
        w: 0.5,
        h: 0.6,
    };
    let mut host = region_node(slate_doc::image_paint::region_path(contours()).unwrap());
    host.rect = WorldRect::new(120.0, -90.0, 300.0, 200.0);
    host.rotation_deg = 37.0;
    host.kind = NodeKind::Image(img.clone());
    let window = slate_doc::image_paint::visible_window_norm(&host, &img);
    let points = &window[0];
    let min_x = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_y = points
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((min_x - 0.2).abs() < 0.0001);
    assert!((max_y - 0.7).abs() < 0.0001);
}

/// Optional real production-widget captures; no model or network required.
#[test]
#[ignore]
fn segment_capture_production_widgets() {
    for dark in [true, false] {
        let (mut h, id) = fixture_unpainted(if dark {
            "segment_dark"
        } else {
            "segment_light"
        });
        h.app.dark_mode = dark;
        h.ctx.set_visuals(if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        let mut raster = FrameRaster::new(1440, 900);
        capture_frame(&mut h, &mut raster, |_| {});
        let p = inject(&mut h, id);
        let out = capture_frame(&mut h, &mut raster, |i| {
            i.events.push(egui::Event::PointerMoved(p))
        });
        rasterize(&mut h, &mut raster, out);
        let mut image = image::RgbaImage::new(1440, 900);
        for (pixel, color) in image.pixels_mut().zip(&raster.px) {
            *pixel = image::Rgba(color.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8));
        }
        let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        image
            .save(output.join(if dark {
                "segment-dark.png"
            } else {
                "segment-light.png"
            }))
            .unwrap();
    }
}
