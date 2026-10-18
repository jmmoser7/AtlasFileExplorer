//! Deterministic input scripts: mask inference is injected at the worker seam.
use super::*;
use crate::app::board::BoardTool;
use crate::app::tests::{capture_frame, rasterize, FrameRaster, Harness};
use slate_doc::scene::ImageNode;
#[path = "image_segment_tests/additive.rs"]
mod additive;

#[path = "image_segment_tests/interaction.rs"]
mod interaction;
#[path = "image_segment_tests/preview.rs"]
mod preview;
#[path = "image_segment_tests/selection.rs"]
mod selection;

#[test]
fn segment_dispatch_shares_resident_pixels_without_copying_them() {
    let (mut h, id) = fixture("segment_shared_pixels");
    let item = h.app.image_item(id).unwrap();
    let key = h.app.resolved_item_preview(item).unwrap().0;
    h.app.preview_cache.remove(&key);
    let pixels = std::sync::Arc::new(egui::ColorImage::new([1024, 512], egui::Color32::RED));
    h.app.thumb_pixels.insert(key.clone(), pixels.clone());
    let request = h
        .app
        .segment_request(
            id,
            &[atlas_segment::PointPrompt {
                point: [0.25, 0.5],
                positive: true,
            }],
        )
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&pixels, &request.pixels));
    assert_eq!(
        request.pixels.size,
        [1024, 512],
        "dispatch leaves sampling to the worker"
    );
    assert_eq!(request.key, format!("{key}:false:false"));
    assert_eq!(request.source_key, h.app.segment_source_key(id).unwrap());
    assert_eq!(request.prompts[0].point, [0.25, 0.5]);

    let model_key = request.key.clone();
    let source_key = request.source_key.clone();
    let node = h.app.doc_mut().scene.node_mut(id).unwrap();
    let NodeKind::Image(img) = &mut node.kind else {
        unreachable!()
    };
    img.crop = slate_doc::scene::Crop {
        x: 0.1,
        y: 0.2,
        w: 0.7,
        h: 0.6,
    };
    let cropped = h
        .app
        .segment_request(
            id,
            &[atlas_segment::PointPrompt {
                point: [0.25, 0.5],
                positive: true,
            }],
        )
        .unwrap();
    assert_eq!(cropped.key, model_key, "crop reuses encoded source pixels");
    assert_ne!(cropped.source_key, source_key, "crop invalidates the mask");
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
    assert!(img.paint_layers.is_empty());
    let stack = slate_doc::image_paint::result_layers(
        img,
        &slate_doc::image_paint::ImageResultId::Item(img.item),
    )
    .unwrap();
    assert_eq!(stack.layers.len(), 1);
    assert_eq!(stack.layers[0].nodes[0].id, region);
    assert_eq!(stack.layers[0].prompts[0].node, region);
    assert_eq!(
        h.app.image_region_prompts(id),
        vec!["give this person a hat"]
    );
    h.app.board_undo();
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert!(img.paint_layers.is_empty());
    assert!(img.result_layers.is_empty());
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
    let source_key = h.app.segment_source_key(id).unwrap();
    h.app.image_segments.hover = Some(Hover {
        tab: h.app.tab().id,
        host: id,
        gen: h.app.scene_gen,
        source_key: source_key.clone(),
        point_norm: [0.375, 0.28125],
        prompts: vec![atlas_segment::PointPrompt {
            point: [0.375, 0.28125],
            positive: true,
        }],
        screen,
        since: Instant::now() - LINGER,
    });
    h.app.image_segments.result = Some(SegmentResult {
        host: id,
        gen: h.app.scene_gen,
        source_key,
        local: region_node(slate_doc::image_paint::region_path(mask.clone()).unwrap()),
        mask: std::sync::Arc::new(mask),
        candidates: std::sync::Arc::new(vec![atlas_segment::Candidate {
            quality: 1.0,
            contours: contours(),
        }]),
        selected: 0,
    });
    screen
}

fn move_to(h: &mut Harness, p: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
}

#[test]
fn segment_highlight_action_pins_selection_without_dragging_or_committing() {
    let (mut h, id) = fixture("segment_action");
    let p = inject(&mut h, id);
    move_to(&mut h, p);
    let before = h.app.doc().scene.node(id).unwrap().rect;
    assert!(
        h.app.image_segments.action_rect.is_none(),
        "hover has no Pin capsule"
    );
    let target = p;
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
    assert_eq!(node.rect, before, "mask press never moves the photo");
    let NodeKind::Image(img) = &node.kind else {
        unreachable!()
    };
    assert!(img.paint_layers.is_empty(), "pinning is ephemeral");
    assert!(h.app.image_selection_clip(id).is_some());
    assert!(h.app.image_segments.pinned.is_some());
}

#[test]
fn pinned_mask_creates_a_hosted_cutout_layer_and_undoes_as_one_host_edit() {
    let (mut h, id) = fixture("segment_selection_layer");
    inject(&mut h, id);
    h.app.pin_hover_selection();
    assert!(h.app.image_selection_clip(id).is_some());
    h.app.scene_gen = h.app.scene_gen.wrapping_add(1);
    assert!(h.app.image_selection_clip(id).is_some());

    let top_level_count = h.app.doc().scene.nodes.len();
    let item = h.app.image_item(id).unwrap();
    assert!(h.app.create_image_layer_from_selection(id));
    assert!(
        h.app.image_selection_clip(id).is_none(),
        "creation clears selection"
    );
    assert_eq!(h.app.board_sel, [id].into(), "host stays selected");
    assert_eq!(h.app.doc().scene.nodes.len(), top_level_count);
    let NodeKind::Image(host) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    let result = h.app.image_result_id(id).unwrap();
    let stack = slate_doc::image_paint::result_layers(host, &result).unwrap();
    assert_eq!(stack.layers.len(), 1);
    assert_eq!(stack.layers[0].nodes.len(), 1);
    let child = &stack.layers[0].nodes[0];
    assert!(
        child.clip.is_some(),
        "the hosted image stores the selection clip"
    );
    assert_eq!(child.rect, WorldRect::new(0.0, 0.0, 1.0, 1.0));
    let NodeKind::Image(child_img) = &child.kind else {
        panic!()
    };
    assert_eq!(child_img.item, item);
    assert!(child_img.agent.is_none());
    assert!(child_img.paint_layers.is_empty());
    assert!(child_img.result_layers.is_empty());
    h.app.board_undo();
    let NodeKind::Image(host) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!()
    };
    assert!(slate_doc::image_paint::result_layers(host, &result).is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), top_level_count);
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
    assert!(h.app.image_segments.result.is_some());
    assert!(h.app.image_segments.action_rect.is_none());
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
    tx.send((
        4,
        Ok(atlas_segment::Prediction {
            candidates: vec![atlas_segment::Candidate {
                quality: 0.9,
                contours: contours(),
            }],
        }),
    ))
    .unwrap();
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
fn legacy_segment_commit_command_pins_the_mask_without_journaling_a_highlight() {
    let (mut h, id) = fixture("segment_export");
    inject(&mut h, id);
    h.app.commit_hover_segment(None).unwrap();
    let NodeKind::Image(img) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert!(img.paint_layers.is_empty());
    assert!(h.app.image_selection_clip(id).is_some());
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
