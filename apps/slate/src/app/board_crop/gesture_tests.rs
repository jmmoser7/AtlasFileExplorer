//! Canonical world gestures preserve authored payloads and the human path.
use super::*;
use slate_doc::scene::{ImageNode, PathContour, PathData, PathFillRule, PathSeg, Scene, SceneCmd};

fn node() -> Node {
    let mut scene = Scene::default();
    let mut image = ImageNode::new(slate_doc::ItemId(1));
    image.crop = Crop {
        x: 0.2,
        y: 0.1,
        w: 0.6,
        h: 0.8,
    };
    image.flip_x = true;
    image.adjust.brightness = 1.15;
    image
        .paint_layers
        .push(slate_doc::PaintLayer::new(slate_doc::PaintLayerId(7)));
    let mut node = scene.build_node(
        WorldRect::new(20.0, 40.0, 120.0, 80.0),
        NodeKind::Image(image),
    );
    node.opacity = 0.7;
    node.group = Some(slate_doc::scene::GroupKey(3));
    node
}

fn crop(node: &Node) -> Crop {
    let NodeKind::Image(image) = &node.kind else {
        panic!("image")
    };
    image.crop
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.002, "{a} != {b}");
}

fn source_anchor(node: &Node, displayed_uv: [f32; 2]) -> [f32; 2] {
    let content = content_rect(node.rect, crop(node));
    node.rect.rotate_point(
        [
            content.x + content.w * displayed_uv[0],
            content.y + content.h * displayed_uv[1],
        ],
        node.rotation_deg,
    )
}

#[test]
fn world_gesture_matches_existing_unrotated_handle_and_pan_math() {
    let before = node();
    for handle in 0..8 {
        let target = (85.0, 75.0);
        let after = apply_gesture(
            &before,
            CropGesture::Handle {
                handle,
                world: target,
            },
        )
        .unwrap();
        let (rect, wanted) = edge_drag_node(&before, handle, target);
        assert_eq!((after.rect, crop(&after)), (rect, wanted));
        let mut payload = after.clone();
        payload.rect = before.rect;
        if let NodeKind::Image(image) = &mut payload.kind {
            image.crop = crop(&before);
        }
        assert_eq!(payload, before, "all non-crop authored fields stay exact");
    }
    let after = apply_gesture(
        &before,
        CropGesture::Pan {
            delta_world: (15.0, -12.0),
        },
    )
    .unwrap();
    assert_eq!(after.rect, before.rect);
    let mut live = before.clone();
    live.opacity = 0.42;
    assert!(copy_crop_geometry(&mut live, &after));
    let mut expected_live = after.clone();
    expected_live.opacity = 0.42;
    assert_eq!(live, expected_live);
    let mut wrong_id = after.clone();
    wrong_id.id = slate_doc::NodeId(after.id.0 + 1);
    assert!(!copy_crop_geometry(&mut live, &wrong_id));
    assert_eq!(live, expected_live);
    assert_eq!(
        crop(&after),
        pan_drag(before.rect, crop(&before), (15.0, -12.0))
    );
    assert!(apply_gesture(
        &before,
        CropGesture::Handle {
            handle: 8,
            world: (0.0, 0.0)
        }
    )
    .is_none());
    assert!(apply_gesture(
        &before,
        CropGesture::Pan {
            delta_world: (f32::NAN, 0.0)
        }
    )
    .is_none());
}

#[test]
fn rotated_primary_and_peers_keep_surviving_displayed_source_anchors() {
    for rotation in [0.0, 30.0, 90.0, -135.0] {
        let mut before = node();
        before.rotation_deg = rotation;
        let target = before
            .rect
            .rotate_point([before.rect.x + 30.0, before.rect.center().1], rotation);
        let after = apply_gesture(
            &before,
            CropGesture::Handle {
                handle: 7,
                world: (target[0], target[1]),
            },
        )
        .unwrap();
        for point in [[0.5, 0.4], [0.75, 0.8]] {
            let a = source_anchor(&before, point);
            let b = source_anchor(&after, point);
            close(a[0], b[0]);
            close(a[1], b[1]);
        }
        let mut peer = before.clone();
        peer.rect.x += 300.0;
        peer.rotation_deg = rotation + 20.0;
        let applied = apply_peer_crop(&peer, crop(&before), crop(&after)).unwrap();
        for point in [[0.5, 0.4], [0.75, 0.8]] {
            let a = source_anchor(&peer, point);
            let b = source_anchor(&applied, point);
            close(a[0], b[0]);
            close(a[1], b[1]);
        }
    }
}

fn hole_mask() -> PathData {
    PathData {
        start: [0.0, 0.0],
        segs: vec![
            PathSeg::Line { to: [1.0, 0.0] },
            PathSeg::Line { to: [1.0, 1.0] },
            PathSeg::Line { to: [0.0, 1.0] },
        ],
        closed: true,
        extra: vec![PathContour {
            start: [0.4, 0.4],
            segs: vec![
                PathSeg::Line { to: [0.6, 0.4] },
                PathSeg::Line { to: [0.6, 0.6] },
                PathSeg::Line { to: [0.4, 0.6] },
            ],
            closed: true,
        }],
        fill_rule: PathFillRule::EvenOdd,
        ..PathData::default()
    }
}

#[test]
fn fixed_mask_grips_keep_holes_frame_and_uniform_source_cover() {
    let mut before = node();
    before.clip = Some(hole_mask());
    before.rotation_deg = 37.0;
    for handle in 0..8 {
        for destination in [(80.0, 80.0), (-400.0, -300.0), (1200.0, 900.0)] {
            let after = apply_gesture(
                &before,
                CropGesture::Handle {
                    handle,
                    world: destination,
                },
            )
            .unwrap();
            assert_eq!(after.rect, before.rect);
            assert_eq!(after.clip, before.clip);
            let content = content_rect(after.rect, crop(&after));
            let source = content_rect(before.rect, crop(&before));
            close(content.w / content.h, source.w / source.h);
            assert!(content.x <= after.rect.x + 0.002 && content.y <= after.rect.y + 0.002);
            assert!(content.x + content.w >= after.rect.x + after.rect.w - 0.002);
            assert!(content.y + content.h >= after.rect.y + after.rect.h - 0.002);
            let contact = gesture_contact(
                &before,
                CropGesture::Handle {
                    handle,
                    world: destination,
                },
            )
            .unwrap();
            let handle_rect = handle_rect(&before);
            let corners = handle_rect.corners_rotated(before.rotation_deg);
            let a = corners[handle as usize / 2];
            let b = corners[(handle as usize / 2 + 1) % 4];
            let wanted = if handle % 2 == 0 {
                a
            } else {
                ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
            };
            close(contact.0, wanted.0);
            close(contact.1, wanted.1);
        }
    }
    assert!(gesture_contact(
        &before,
        CropGesture::Pan {
            delta_world: (1.0, 2.0)
        }
    )
    .is_none());
    let center = before.rect.center();
    assert!(
        !slate_doc::geom::point_in_node_outline(&before, center.0, center.1, 0.05),
        "body contact must not assume a mask's center is visible"
    );
}

#[test]
fn complete_crop_patch_inverse_and_serialized_export_keep_the_same_model() {
    let mut doc = slate_doc::SlateDoc::new("Crop gesture");
    let path = std::path::PathBuf::from("crop-source.png");
    let item = doc.add_item(path.clone(), "crop-source.png", 0, 0, "");
    let mut before = node();
    before.rotation_deg = 30.0;
    if let NodeKind::Image(image) = &mut before.kind {
        image.item = item;
    }
    doc.scene.nodes.push(before.clone());
    let after = apply_gesture(
        &before,
        CropGesture::Handle {
            handle: 7,
            world: (80.0, 80.0),
        },
    )
    .unwrap();
    let patch = SceneCmd::Patch {
        before: Box::new(before.clone()),
        after: Box::new(after.clone()),
    };
    assert!(doc.scene.apply(&patch));
    assert_eq!(doc.scene.node(after.id), Some(&after));
    let mut assets = slate_artifact::AssetMap::default();
    assets.insert(path, "assets/crop-source.png".into());
    let html = slate_artifact::render_html(&doc, &assets);
    let encoded = serde_json::to_vec(&doc).unwrap();
    let roundtrip: slate_doc::SlateDoc = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(slate_artifact::render_html(&roundtrip, &assets), html);
    assert!(html.contains("transform:rotate(30.000deg)"));
    let c = crop(&after);
    assert!(html.contains(&format!("width:{:.4}%", 100.0 / c.w)));
    assert!(html.contains(&format!("left:{:.4}%", -c.x / c.w * 100.0)));
    assert!(doc.scene.apply(&patch.inverted()));
    assert_eq!(doc.scene.node(before.id), Some(&before));
}

#[test]
fn effective_pan_follows_clamped_displayed_source_travel_across_rotations_and_mirrors() {
    for rotation in [0.0, 37.0, 90.0, -135.0] {
        for masked in [false, true] {
            let mut before = node();
            before.rotation_deg = rotation;
            before.clip = masked.then(hole_mask);
            if let NodeKind::Image(image) = &mut before.kind {
                image.flip_y = true;
            }
            let immutable = before.clone();
            let requested = CropGesture::Pan {
                delta_world: (10000.0, -7000.0),
            };
            let exact_prepared = apply_gesture(&before, requested).unwrap();
            let effective = effective_gesture(&before, requested).unwrap();
            let CropGesture::Pan {
                delta_world: (dx, dy),
            } = effective
            else {
                panic!("Pan")
            };
            let a = source_anchor(&before, [0.5, 0.5]);
            let b = source_anchor(&exact_prepared, [0.5, 0.5]);
            close(dx, b[0] - a[0]);
            close(dy, b[1] - a[1]);
            assert!(
                dx.abs() < 1000.0 && dy.abs() < 1000.0,
                "bounded source movement cannot follow the raw overlimit pointer"
            );
            let endpoint_replay = apply_gesture(&before, effective).unwrap();
            let replay_anchor = source_anchor(&endpoint_replay, [0.5, 0.5]);
            close(replay_anchor[0], b[0]);
            close(replay_anchor[1], b[1]);
            // This is a visual endpoint roundtrip only. Exact final Node identity
            // belongs to the witness test retaining the originally prepared Node.
            assert_eq!(before, immutable);
            assert_eq!(exact_prepared.rect, before.rect);
            assert_eq!(exact_prepared.clip, before.clip);
        }
    }
}

#[test]
fn effective_handles_end_on_the_painted_clamped_window_or_uniform_source_grip() {
    for rotation in [0.0, 37.0, 90.0] {
        for masked in [false, true] {
            let mut before = node();
            before.rotation_deg = rotation;
            before.clip = masked.then(hole_mask);
            for handle in 0..8 {
                let requested = CropGesture::Handle {
                    handle,
                    world: (-900.0, 1200.0),
                };
                let exact_prepared = apply_gesture(&before, requested).unwrap();
                let effective = effective_gesture(&before, requested).unwrap();
                let CropGesture::Handle {
                    handle: resolved,
                    world,
                } = effective
                else {
                    panic!("Handle")
                };
                assert_eq!(resolved, handle);
                assert_ne!(world, (-900.0, 1200.0));
                assert_eq!(Some(world), gesture_contact(&exact_prepared, requested));
                let endpoint_replay = apply_gesture(&before, effective).unwrap();
                let replay = gesture_contact(&endpoint_replay, effective).unwrap();
                close(replay.0, world.0);
                close(replay.1, world.1);
                if masked {
                    assert_eq!(exact_prepared.rect, before.rect);
                    assert_eq!(exact_prepared.clip, before.clip);
                }
            }
        }
    }
}

#[test]
fn effective_pan_uses_the_same_clamped_display_coordinates_as_existing_paint() {
    let mut before = node();
    before.rotation_deg = 45.0;
    if let NodeKind::Image(image) = &mut before.kind {
        image.crop.x = -0.25;
        image.crop.y = 2.0;
    }
    let requested = CropGesture::Pan {
        delta_world: (1000.0, 1000.0),
    };
    let after = apply_gesture(&before, requested).unwrap();
    let CropGesture::Pan {
        delta_world: (dx, dy),
    } = effective_gesture(&before, requested).unwrap()
    else {
        panic!("Pan")
    };
    let start = source_anchor(&before, [0.5, 0.5]);
    let end = source_anchor(&after, [0.5, 0.5]);
    close(dx, end[0] - start[0]);
    close(dy, end[1] - start[1]);
}
