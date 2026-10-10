//! Deterministic input scripts: mask inference is injected at the worker seam.
use super::*;
use crate::app::board::BoardTool;
use crate::app::tests::{capture_frame, rasterize, FrameRaster, Harness};
use slate_doc::scene::ImageNode;

#[test]
fn segment_dispatch_shares_resident_pixels_without_copying_them() {
    let (mut h, id) = fixture("segment_shared_pixels");
    let item = h.app.image_item(id).unwrap();
    let key = h.app.resolved_item_preview(item).unwrap().0;
    h.app.preview_cache.remove(&key);
    let pixels = std::sync::Arc::new(egui::ColorImage::new([1024, 512], egui::Color32::RED));
    h.app.thumb_pixels.insert(key, pixels.clone());
    let request = h.app.segment_request(id, [0.25, 0.5]).unwrap();
    assert!(std::sync::Arc::ptr_eq(&pixels, &request.pixels));
    assert_eq!(
        request.pixels.size,
        [1024, 512],
        "dispatch leaves sampling to the worker"
    );
    assert_eq!(request.point, [0.25, 0.5]);
}

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

pub(super) fn fixture(name: &str) -> (Harness, NodeId) {
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
    h.app
        .pin_pack_for_test("sam", atlas_ai::packs::PackHealth::Ok);
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

pub(super) fn inject(h: &mut Harness, id: NodeId) -> Pos2 {
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

pub(super) fn move_to(h: &mut Harness, p: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
}

#[test]
fn segment_highlight_click_opens_a_tag_and_create_layer_does_not_move_the_photo() {
    let (mut h, id) = fixture("segment_action");
    let p = inject(&mut h, id);
    move_to(&mut h, p);
    assert!(
        h.app.image_segments.tag_at.is_none(),
        "hover draws the highlight without a button"
    );
    let before = h.app.doc().scene.node(id).unwrap().rect;
    for pressed in [true, false] {
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            })
        });
    }
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before);
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(img.paint_layers.is_empty(), "the click only opens the tag");
    let target = h.app.image_segments.tag_rects[0].center();
    assert!(target.y < p.y, "the tag sits above the cursor");
    for pressed in [true, false] {
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerMoved(target));
            i.events.push(egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            })
        });
    }
    let node = h.app.doc().scene.node(id).unwrap();
    assert_eq!(node.rect, before, "the tag never moves the photo");
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
    assert_eq!(h.app.image_segments.layers_menu, Some(id));
    assert!(
        super::tag::layer_rows(img)
            .iter()
            .any(|row| row == "Layer 1"),
        "the new layer is listed on the layer squircle"
    );
    h.app.board_undo();
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(img.paint_layers.is_empty());
}

#[test]
fn a_machine_without_the_highlight_pack_is_never_offered_the_dwell() {
    let (mut h, id) = fixture("segment_no_pack");
    h.app
        .pin_pack_for_test("sam", atlas_ai::packs::PackHealth::Missing);
    let p = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    move_to(&mut h, p);
    assert!(h.app.image_segments.hover.is_none());
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
    press_escape(&mut h);
    assert!(h.app.image_segments.hover.is_none());
}

fn press_escape(h: &mut Harness) {
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
}

#[test]
fn escape_over_a_bare_dwell_still_reaches_the_selection() {
    let (mut h, id) = fixture("segment_escape_falls_through");
    h.app.board_sel = [id].into();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let center = h.app.board_xf().rect_w2s(rect).center();
    move_to(&mut h, center);
    assert!(h.app.image_segments.hover.is_some());
    press_escape(&mut h);
    assert!(h.app.image_segments.hover.is_none());
    assert!(
        h.app.board_sel.is_empty(),
        "a bare dwell is not a cancel layer"
    );
    // A visible offer is one: Esc dismisses it and keeps the selection.
    h.app.board_sel = [id].into();
    let p = inject(&mut h, id);
    move_to(&mut h, p);
    assert!(
        h.app.image_segments.result.is_some(),
        "the highlight is the offer"
    );
    press_escape(&mut h);
    assert!(h.app.image_segments.result.is_none());
    assert!(h.app.board_sel.contains(&id));
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

#[test]
fn segment_grow_is_idle_until_the_debug_flag_and_then_retreats() {
    let (mut h, id) = fixture("segment_grow");
    let p = inject(&mut h, id);
    let width = |h: &Harness| h.app.image_segments.result.as_ref().unwrap().mask[0][1][0];
    let before = width(&h);
    h.app.segment_grow_step(p, 1.0);
    h.app.segment_grow_step(p + egui::vec2(1.5, 0.0), 1.0);
    assert_eq!(width(&h), before, "no flag, no growth");
    h.app.image_segments.grow_debug = true;
    h.app.image_segments.grow_last = None;
    h.app.segment_grow_step(p, 1.0);
    h.app.segment_grow_step(p + egui::vec2(1.5, 0.0), 1.0);
    assert!(width(&h) > before, "slow drift grows the outline");
    h.app.segment_grow_step(p + egui::vec2(40.0, 0.0), 1.0);
    assert!(
        (width(&h) - before).abs() < 1e-4,
        "a quick retreat undoes it"
    );
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
