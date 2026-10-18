//! Production frame pixels for rotated and mirrored human crop gestures.
use super::*;
use crate::app::{
    board_crop::{self, CropGesture},
    preview::{PreviewEntry, PX_EXACT},
};
use slate_doc::scene::ImageNode;

fn ready_image(h: &mut Harness, item: slate_doc::ItemId, name: &str) {
    let (key, path, _, _) = h.app.resolved_item_preview(item).unwrap();
    let decoded = image::open(path).unwrap().to_rgba8();
    let pixels = egui::ColorImage::from_rgba_unmultiplied(
        [decoded.width() as usize, decoded.height() as usize],
        decoded.as_raw(),
    );
    let tex = h
        .ctx
        .load_texture(name, pixels.clone(), egui::TextureOptions::LINEAR);
    h.app.preview_cache.insert(
        key,
        PreviewEntry {
            tex,
            bytes: pixels.pixels.len() * 4,
            pixels: std::sync::Arc::new(pixels),
            px: PX_EXACT,
            last_used: 0,
        },
    );
}

fn sample(raster: &FrameRaster, screen: Pos2) -> [f32; 3] {
    let (x, y) = (screen.x.round() as usize, screen.y.round() as usize);
    let mut sum = [0.0; 3];
    for dy in 0..3 {
        for dx in 0..3 {
            let pixel = raster.px[(y + dy - 1) * raster.w + x + dx - 1];
            for channel in 0..3 {
                sum[channel] += pixel[channel] / 9.0;
            }
        }
    }
    sum
}

#[test]
fn human_crop_keeps_rotated_mirrored_primary_and_peer_pixels_at_world_anchors() {
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review")
        .join(format!("crop-anchors-{}", atlas_agent::request_id()));
    std::fs::create_dir_all(&output).unwrap();
    let mut evidence = Vec::new();
    for (case, (rotation, flip_x, flip_y)) in [
        (0.0, false, false),
        (30.0, true, false),
        (90.0, false, true),
        (-45.0, true, true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut h = web_board("canonical_crop_anchors");
        h.app.dark_mode = false;
        h.app.tab_mut().cam.z = 1.0;
        h.app.tab_mut().cam.offset = EVec2::ZERO;
        let source = h.base.join("coordinate-colors.png");
        image::RgbaImage::from_fn(120, 80, |x, y| {
            image::Rgba([
                (x * 2) as u8,
                (y * 3) as u8,
                70 + ((x / 15 + y / 10) % 2) as u8 * 100,
                255,
            ])
        })
        .save(&source)
        .unwrap();
        let item = h.app.add_paths(std::slice::from_ref(&source))[0];
        let mut nodes = Vec::new();
        for (rect, angle) in [
            (WorldRect::new(-310.0, -80.0, 240.0, 160.0), rotation),
            (WorldRect::new(90.0, -60.0, 180.0, 120.0), rotation + 20.0),
        ] {
            let mut image = ImageNode::new(item);
            image.flip_x = flip_x;
            image.flip_y = flip_y;
            let mut node = h
                .app
                .doc_mut()
                .scene
                .build_node(rect, NodeKind::Image(image));
            node.rotation_deg = angle;
            nodes.push(node);
        }
        let originals = nodes.clone();
        let ids = h.app.add_nodes(nodes);
        ready_image(&mut h, item, &format!("crop-coordinate-source-{case}"));
        h.app.board_sel = ids.iter().copied().collect();
        h.app.enter_crop_mode(ids[0]);
        let mut raster = FrameRaster::new(1440, 900);
        let _ = capture_frame(&mut h, &mut raster, |_| {});
        let _ = capture_frame(&mut h, &mut raster, |_| {});
        let xf = h.app.board_xf();
        raster.sync_fonts(&h.ctx);
        let before_frame = capture_frame(&mut h, &mut raster, |_| {});
        rasterize(&mut h, &mut raster, before_frame);
        raster.save(&output.join(format!("{case}-before.png")));
        let anchors: Vec<_> = originals
            .iter()
            .flat_map(|node| {
                [[0.35, 0.25], [0.55, 0.65], [0.75, 0.35]]
                    .into_iter()
                    .map(move |point| {
                        let local = [
                            node.rect.x + node.rect.w * point[0],
                            node.rect.y + node.rect.h * point[1],
                        ];
                        let world = node.rect.rotate_point(local, node.rotation_deg);
                        xf.w2s(Pos2::new(world[0], world[1]))
                    })
            })
            .collect();
        let before_pixels: Vec<_> = anchors
            .iter()
            .map(|point| sample(&raster, *point))
            .collect();
        let before = &originals[0];
        let target = before
            .rect
            .rotate_point([before.rect.x + 48.0, before.rect.center().1], rotation);
        let gesture = CropGesture::Handle {
            handle: 7,
            world: (target[0], target[1]),
        };
        let contact = board_crop::gesture_contact(before, gesture).unwrap();
        let world = Pos2::new(contact.0, contact.1);
        h.app.board_drag =
            h.app
                .begin_gesture_for_test(xf.w2s(world), world, egui::Modifiers::NONE);
        assert!(
            matches!(
                h.app.board_drag,
                Some(board::BoardDrag::CropEdge { handle: 7, .. })
            ),
            "actual painted west handle must enter the human crop path"
        );
        let depth = h.app.tab().journal.undo_depth();
        let target = Pos2::new(target[0], target[1]);
        h.app.update_gesture_for_test(target, egui::Modifiers::NONE);
        assert_eq!(
            h.app.doc().scene.node(ids[0]),
            Some(&board_crop::apply_gesture(before, gesture).unwrap())
        );
        h.app
            .end_gesture_for_test(target, Some(xf.w2s(target)), egui::Modifiers::NONE);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        let after_frame = capture_frame(&mut h, &mut raster, |_| {});
        rasterize(&mut h, &mut raster, after_frame);
        raster.save(&output.join(format!("{case}-after.png")));
        for (anchor, old) in anchors.iter().zip(&before_pixels) {
            let new = sample(&raster, *anchor);
            assert!(
                old[2] < 0.8 && (old[0] > 0.03 || old[1] > 0.03),
                "source gradient must actually be painted"
            );
            for channel in 0..3 {
                assert!(
                    (new[channel] - old[channel]).abs() < 0.015,
                    "case {case} anchor {anchor:?}: old {old:?}, new {new:?}"
                );
            }
        }
        let primary = h.app.doc().scene.node(ids[0]).unwrap().clone();
        let NodeKind::Image(image) = &primary.kind else {
            panic!("image")
        };
        let expected_peer =
            board_crop::apply_peer_crop(&originals[1], slate_doc::scene::Crop::full(), image.crop)
                .unwrap();
        assert_eq!(h.app.doc().scene.node(ids[1]), Some(&expected_peer));
        assert_eq!(h.app.tab().cam.z, 1.0);
        assert_eq!(h.app.tab().cam.offset, EVec2::ZERO);
        h.app.board_undo();
        for node in &originals {
            assert_eq!(h.app.doc().scene.node(node.id), Some(node));
        }
        evidence.push(serde_json::json!({"case":case,"rotation":rotation,"mirrorX":flip_x,"mirrorY":flip_y,"primaryAndPeerAnchors":anchors.len(),"singleUndo":true}));
    }
    std::fs::write(output.join("asserted-cases.json"),serde_json::to_vec_pretty(&serde_json::json!({"scope":"Production full-app software frames; no native GPU or human acceptance claim","cases":evidence})).unwrap()).unwrap();
    println!("Retained crop anchor software frames: {}", output.display());
}

fn pan_mask() -> slate_doc::scene::PathData {
    use slate_doc::scene::{PathContour, PathData, PathFillRule, PathSeg};
    PathData {
        start: [0.0, 0.0],
        segs: vec![
            PathSeg::Line { to: [1.0, 0.0] },
            PathSeg::Line { to: [1.0, 1.0] },
            PathSeg::Line { to: [0.0, 1.0] },
        ],
        closed: true,
        extra: vec![PathContour {
            start: [0.45, 0.45],
            segs: vec![
                PathSeg::Line { to: [0.55, 0.45] },
                PathSeg::Line { to: [0.55, 0.55] },
                PathSeg::Line { to: [0.45, 0.55] },
            ],
            closed: true,
        }],
        fill_rule: PathFillRule::EvenOdd,
        ..PathData::default()
    }
}

#[test]
fn real_human_pan_uses_the_same_immutable_start_gesture_and_one_undo() {
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/review")
        .join(format!("crop-pan-anchors-{}", atlas_agent::request_id()));
    std::fs::create_dir_all(&output).unwrap();
    let mut evidence = Vec::new();
    for (case, (rotation, flip_x, flip_y, masked)) in [
        (0.0, false, false, false),
        (37.0, true, false, false),
        (90.0, true, true, false),
        (37.0, true, true, true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut h = web_board("canonical_crop_pan");
        h.app.dark_mode = false;
        h.app.board_show_grid = false;
        h.app.tab_mut().cam.z = 1.0;
        h.app.tab_mut().cam.offset = EVec2::ZERO;
        let source = h.base.join("pan-coordinate-colors.png");
        image::RgbaImage::from_fn(120, 80, |x, y| {
            image::Rgba([
                (x * 2) as u8,
                (y * 3) as u8,
                70 + ((x / 15 + y / 10) % 2) as u8 * 100,
                255,
            ])
        })
        .save(&source)
        .unwrap();
        let item = h.app.add_paths(std::slice::from_ref(&source))[0];
        let mut image = ImageNode::new(item);
        image.flip_x = flip_x;
        image.flip_y = flip_y;
        image.crop = slate_doc::scene::Crop {
            x: 0.2,
            y: 0.1,
            w: 0.6,
            h: 0.8,
        };
        let mut node = h.app.doc_mut().scene.build_node(
            WorldRect::new(-150.0, -100.0, 300.0, 200.0),
            NodeKind::Image(image),
        );
        node.rotation_deg = rotation;
        node.clip = masked.then(pan_mask);
        let id = h.app.add_nodes(vec![node])[0];
        ready_image(&mut h, item, &format!("crop-pan-coordinate-source-{case}"));
        h.app.enter_crop_mode(id);
        let mut raster = FrameRaster::new(1440, 900);
        let _ = capture_frame(&mut h, &mut raster, |_| {});
        let _ = capture_frame(&mut h, &mut raster, |_| {});
        raster.sync_fonts(&h.ctx);
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let camera = (h.app.tab().cam.offset, h.app.tab().cam.z);
        let xf = h.app.board_xf();
        let local = [before.rect.x + 60.0, before.rect.y + 40.0];
        let point = before.rect.rotate_point(local, rotation);
        let start = Pos2::new(point[0], point[1]);
        assert!(slate_doc::geom::point_in_node_outline(
            &before, start.x, start.y, 0.05
        ));
        if masked {
            let center = before.rect.center();
            assert!(
                !slate_doc::geom::point_in_node_outline(&before, center.0, center.1, 0.05),
                "fixture has a real closed-mask hole"
            );
        }
        h.app.board_drag =
            h.app
                .begin_gesture_for_test(xf.w2s(start), start, egui::Modifiers::NONE);
        assert!(
            matches!(h.app.board_drag, Some(board::BoardDrag::CropPan { .. })),
            "a visible interior press must use the real human pixel-pan path"
        );
        // An overlapping style edit is part of BOTH pixel baselines. It must
        // survive crop preview/commit but cannot itself make the pixel test pass.
        h.app.doc_mut().scene.node_mut(id).unwrap().opacity = 0.42;
        let baseline = capture_frame(&mut h, &mut raster, |_| {});
        assert!(matches!(
            h.app.board_drag,
            Some(board::BoardDrag::CropPan { .. })
        ));
        rasterize(&mut h, &mut raster, baseline);
        raster.save(&output.join(format!("{case}-before.png")));
        let delta = EVec2::new(12.0, 3.0);
        let (sin, cos) = rotation.to_radians().sin_cos();
        let local_delta = (
            delta.x * cos + delta.y * sin,
            -delta.x * sin + delta.y * cos,
        );
        let NodeKind::Image(original) = &before.kind else {
            panic!("image")
        };
        let old_crop = original.crop;
        let expected_x = old_crop.x - local_delta.0 / (before.rect.w / old_crop.w);
        let expected_y = old_crop.y - local_delta.1 / (before.rect.h / old_crop.h);
        assert!(
            expected_x > 0.0
                && expected_x < 1.0 - old_crop.w
                && expected_y > 0.0
                && expected_y < 1.0 - old_crop.h,
            "the independent world-translation oracle requires an unclamped pan"
        );
        let world_anchors: Vec<_> = [[0.30, 0.25], [0.70, 0.30], [0.70, 0.75]]
            .into_iter()
            .map(|p| {
                let local = [
                    before.rect.x + before.rect.w * p[0],
                    before.rect.y + before.rect.h * p[1],
                ];
                let point = before.rect.rotate_point(local, rotation);
                Pos2::new(point[0], point[1])
            })
            .collect();
        for anchor in &world_anchors {
            let moved = *anchor + delta;
            assert!(slate_doc::geom::point_in_node_outline(
                &before, anchor.x, anchor.y, 0.05
            ));
            assert!(
                slate_doc::geom::point_in_node_outline(&before, moved.x, moved.y, 0.05),
                "sample and translated sample stay inside the actual mask, clear of its hole"
            );
        }
        let before_pixels: Vec<_> = world_anchors
            .iter()
            .map(|p| sample(&raster, xf.w2s(*p)))
            .collect();
        let target = start + delta;
        // Replay the actual event delta: adding to a rotated f32 point can
        // round the requested translation before the gesture subtracts start.
        let event_delta = target - start;
        let depth = h.app.tab().journal.undo_depth();
        let mut expected = board_crop::apply_gesture(
            &before,
            CropGesture::Pan {
                delta_world: (event_delta.x, event_delta.y),
            },
        )
        .unwrap();
        expected.opacity = 0.42;
        h.app.update_gesture_for_test(target, egui::Modifiers::NONE);
        assert_eq!(h.app.doc().scene.node(id), Some(&expected));
        let NodeKind::Image(actual) = &h.app.doc().scene.node(id).unwrap().kind else {
            panic!("image")
        };
        assert!(
            (actual.crop.x - expected_x).abs() < 1e-6 && (actual.crop.y - expected_y).abs() < 1e-6,
            "independent displayed-source offset oracle"
        );
        h.app
            .end_gesture_for_test(target, Some(xf.w2s(target)), egui::Modifiers::NONE);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        assert_eq!(h.app.doc().scene.node(id), Some(&expected));
        assert_eq!(
            h.app.doc().scene.node(id).unwrap().rect,
            before.rect,
            "Pan keeps the authored mask/frame fixed"
        );
        assert_eq!(h.app.doc().scene.node(id).unwrap().clip, before.clip);
        let frame = capture_frame(&mut h, &mut raster, |_| {});
        rasterize(&mut h, &mut raster, frame);
        raster.save(&output.join(format!("{case}-after.png")));
        for (anchor, old) in world_anchors.iter().zip(&before_pixels) {
            // This pixel oracle depends ONLY on the gesture's world delta,
            // never on apply_gesture's resulting crop or frame.
            let translated = sample(&raster, xf.w2s(*anchor + delta));
            let stationary = sample(&raster, xf.w2s(*anchor));
            for channel in 0..3 {
                assert!((translated[channel]-old[channel]).abs()<0.015,"case {case}: source pixel must travel by the independent world delta; old {old:?}, translated {translated:?}");
            }
            let change = stationary
                .iter()
                .zip(old)
                .map(|(new, old)| (new - old).abs())
                .sum::<f32>();
            assert!(change>0.006,"a stationary image must fail the oracle: case {case}, old {old:?}, stationary {stationary:?}");
        }
        assert_eq!((h.app.tab().cam.offset, h.app.tab().cam.z), camera);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(id), Some(&before));
        evidence.push(serde_json::json!({"case":case,"rotation":rotation,"mirrorX":flip_x,"mirrorY":flip_y,"fixedHoledMask":masked,"worldDelta":[delta.x,delta.y],"unclamped":true,"sourceAnchors":world_anchors.len(),"sameOpacityBaselines":0.42,"independentOracle":"before(P) equals after(P + worldDelta); after(P) must change","singleUndo":true,"cameraUnchanged":true}));
    }
    std::fs::write(output.join("asserted-cases.json"),serde_json::to_vec_pretty(&serde_json::json!({"scope":"Production full-app software frames; no native GPU or human acceptance claim","cases":evidence})).unwrap()).unwrap();
    println!("Retained crop-pan software frames: {}", output.display());
}
