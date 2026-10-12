use crate::image_paint::*;
use crate::scene::{Corner, Crop, NodeKind, ShapeKind, ShapeNode, Stroke};

fn path_shape() -> ShapeNode {
    ShapeNode {
        shape: ShapeKind::Path,
        sides: 6,
        phase_deg: 0.0,
        fill: None,
        stroke: Stroke::default(),
        corner: Corner::Square,
        flip: false,
        path: None,
        text: None,
    }
}

fn host_with_crop(crop: Crop) -> Node {
    let mut scene = crate::scene::Scene::default();
    scene.build_node(
        WorldRect::new(10.0, 20.0, 100.0, 50.0),
        NodeKind::Image(ImageNode {
            item: crate::ItemId(1),
            crop,
            ..ImageNode::new(crate::ItemId(1))
        }),
    )
}

fn stroke_local(u: f32, v: f32) -> Node {
    let mut scene = crate::scene::Scene::default();
    scene.build_node(
        WorldRect::new(u, v, 0.2, 0.1),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            sides: 6,
            phase_deg: 0.0,
            fill: None,
            stroke: Stroke::default(),
            corner: Corner::Square,
            flip: false,
            path: None,
            text: None,
        }),
    )
}

#[test]
fn layer_norm_is_content_not_crop_window() {
    let crop = Crop {
        x: 0.25,
        y: 0.25,
        w: 0.5,
        h: 0.5,
    };
    let host = host_with_crop(crop);
    let img = match &host.kind {
        NodeKind::Image(i) => i,
        _ => unreachable!(),
    };
    let (cx, cy) = host.rect.center();
    let (u, v) = world_to_host_norm(&host, img, cx, cy);
    assert!((u - 0.5).abs() < 1e-3 && (v - 0.5).abs() < 1e-3);
    let mut local = stroke_local(0.0, 0.0);
    local.rect = WorldRect::new(0.4, 0.4, 0.2, 0.2);
    let world = layer_node_to_world(&host, img, &local);
    let content = image_content_rect(&host, img);
    let (ccx, ccy) = content.center();
    let (wx, wy) = world.rect.center();
    assert!((wx - ccx).abs() < 1e-2 && (wy - ccy).abs() < 1e-2);
}

#[test]
fn layer_coords_round_trip_unrotated() {
    let host = host_with_crop(Crop::full());
    let img = match &host.kind {
        NodeKind::Image(i) => i,
        _ => unreachable!(),
    };
    let local = stroke_local(0.2, 0.2);
    let back = layer_node_to_world(&host, img, &local);
    let again = layer_node_from_world(&host, img, &back);
    assert!((again.rect.x - local.rect.x).abs() < 1e-4);
    assert!((again.rect.y - local.rect.y).abs() < 1e-4);
}

#[test]
fn layer_coords_survive_host_rotation() {
    for deg in [37.0, 90.0, 180.0] {
        let mut host = host_with_crop(Crop::full());
        host.rotation_deg = deg;
        let img = match &host.kind {
            NodeKind::Image(i) => i.clone(),
            _ => unreachable!(),
        };
        let local = stroke_local(0.3, 0.4);
        let world = layer_node_to_world(&host, &img, &local);
        let back = layer_node_from_world(&host, &img, &world);
        assert!((back.rect.x - local.rect.x).abs() < 1e-3, "deg={deg} x");
        assert!((back.rect.y - local.rect.y).abs() < 1e-3, "deg={deg} y");
        assert!(
            (back.rotation_deg - local.rotation_deg).abs() < 1e-3,
            "deg={deg} rot"
        );
    }
}

#[test]
fn selection_mask_maps_into_child_clip_coordinates_and_keeps_holes() {
    let mut child = stroke_local(0.25, 0.25);
    child.rect = WorldRect::new(0.25, 0.25, 0.5, 0.5);
    let outer = vec![[0.25, 0.25], [0.75, 0.25], [0.75, 0.75], [0.25, 0.75]];
    let hole = vec![[0.4, 0.4], [0.4, 0.6], [0.6, 0.6], [0.6, 0.4]];
    let clip = clip_path_for_layer_node(&child, &[outer, hole]).unwrap();
    assert_eq!(clip.start, [0.0, 0.0]);
    assert_eq!(clip.extra.len(), 1);
    assert_eq!(clip.extra[0].start, [0.3, 0.3]);
}

#[test]
fn host_local_mapping_does_not_orbit_with_rotated_host() {
    let mut host = host_with_crop(Crop {
        x: 0.25,
        y: 0.0,
        w: 0.5,
        h: 1.0,
    });
    host.rotation_deg = 90.0;
    let img = match &host.kind {
        NodeKind::Image(i) => i,
        _ => unreachable!(),
    };
    let local = stroke_local(0.0, 0.0);
    let mapped = layer_node_to_host_local(&host, img, &local);
    assert!((mapped.rect.x + 50.0).abs() < 1e-3);
    assert!((mapped.rect.y - 0.0).abs() < 1e-3);
    assert!((mapped.rect.w - 40.0).abs() < 1e-3);
    assert_eq!(mapped.rotation_deg, local.rotation_deg);
}

#[test]
fn rotate_after_draw_round_trip() {
    let mut host = host_with_crop(Crop::full());
    let img = match &host.kind {
        NodeKind::Image(i) => i.clone(),
        _ => unreachable!(),
    };
    let local = stroke_local(0.25, 0.35);
    let world_before = layer_node_to_world(&host, &img, &local);
    host.rotation_deg = 45.0;
    let world_after = layer_node_to_world(&host, &img, &local);
    assert!(world_before.rect.x != world_after.rect.x);
    let back = layer_node_from_world(&host, &img, &world_after);
    assert!((back.rect.x - local.rect.x).abs() < 1e-3);
}

#[test]
fn old_image_without_layers_field_defaults_empty() {
    let json = r#"{"item":1,"crop":{"x":0,"y":0,"w":1,"h":1}}"#;
    let img: ImageNode = serde_json::from_str(json).unwrap();
    assert!(img.paint_layers.is_empty());
    assert!(img.result_layers.is_empty());
}

#[test]
fn layer_node_kind_allows_drawing_tools_and_layer_images() {
    use crate::scene::{FrameNode, Rgba, TextNode};
    assert!(layer_node_kind_allowed(&NodeKind::Text(TextNode {
        text: String::new(),
        family: Default::default(),
        size: 14.0,
        color: Rgba([0, 0, 0, 255]),
        align: Default::default(),
        fill: None,
        stroke: Default::default(),
        agent: None,
    })));
    for shape in [
        ShapeKind::Path,
        ShapeKind::Line,
        ShapeKind::Rect,
        ShapeKind::Ellipse,
    ] {
        let mut s = path_shape();
        s.shape = shape;
        assert!(layer_node_kind_allowed(&NodeKind::Shape(s)));
    }
    assert!(layer_node_kind_allowed(&NodeKind::Image(ImageNode::new(
        crate::ItemId(1)
    ))));
    assert!(!layer_node_kind_allowed(&NodeKind::Frame(FrameNode {
        title: String::new(),
        order: 0,
        fill: Rgba([255, 255, 255, 255]),
        fill_authored: false,
        assignments: Default::default(),
        stroke: Stroke::none(),
        corner: Corner::Square,
    })));
}

#[test]
fn find_layer_node_locates_nested_stroke() {
    let mut scene = crate::scene::Scene::default();
    let host = scene.build_node(
        WorldRect::new(10.0, 20.0, 100.0, 50.0),
        NodeKind::Image(ImageNode::new(crate::ItemId(1))),
    );
    let host_id = host.id;
    scene.apply(&crate::scene::SceneCmd::Add {
        index: 0,
        node: host,
    });
    let local = scene.build_node(
        WorldRect::new(0.1, 0.1, 0.2, 0.2),
        NodeKind::Shape(path_shape()),
    );
    let stroke_id = local.id;
    let host = scene.node_mut(host_id).unwrap();
    let NodeKind::Image(ref mut img) = host.kind else {
        unreachable!()
    };
    img.paint_layers.push(PaintLayer::new(PaintLayerId(1)));
    img.paint_layers[0].nodes.push(local);
    let loc = find_layer_node(&scene, stroke_id).unwrap();
    assert_eq!(loc.image, host_id);
    assert_eq!(loc.layer_index, 0);
    assert_eq!(loc.node_index, 0);
}

#[test]
fn find_layer_node_keeps_album_result_identity() {
    let mut scene = crate::scene::Scene::default();
    let host = scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(crate::ItemId::NONE)),
    );
    let host_id = host.id;
    scene.apply(&crate::scene::SceneCmd::Add {
        index: 0,
        node: host,
    });
    let child = stroke_local(0.1, 0.1);
    let child_id = child.id;
    let host = scene.node_mut(host_id).unwrap();
    let NodeKind::Image(img) = &mut host.kind else {
        unreachable!();
    };
    let result = ImageResultId::Output("run-4-1".into());
    let stack = ensure_result_layers_mut(img, result.clone());
    let mut layer = PaintLayer::new(PaintLayerId(12));
    layer.nodes.push(child);
    stack.add_layer(layer);
    let location = find_layer_node(&scene, child_id).unwrap();
    assert_eq!(location.image, host_id);
    assert_eq!(location.result, Some(result));
    assert_eq!(location.layer_index, 0);
}

#[test]
fn result_stacks_round_trip_and_isolate_album_outputs() {
    let mut img = ImageNode::new(crate::ItemId::NONE);
    let first = ImageResultId::Output("request-a-0".into());
    let second = ImageResultId::Output("request-b-0".into());
    ensure_result_layers_mut(&mut img, first.clone())
        .layers
        .push(PaintLayer::new(PaintLayerId(4)));
    ensure_result_layers_mut(&mut img, second.clone())
        .layers
        .push(PaintLayer::new(PaintLayerId(4)));
    let encoded = serde_json::to_string(&img).unwrap();
    let decoded: ImageNode = serde_json::from_str(&encoded).unwrap();
    assert_eq!(layers_for_result(&decoded, &first).len(), 1);
    assert_eq!(layers_for_result(&decoded, &second).len(), 1);
    assert!(layers_for_result(&decoded, &ImageResultId::Output("later".into())).is_empty());
}

#[test]
fn legacy_layers_resolve_only_for_the_matching_ordinary_item() {
    let mut img = ImageNode::new(crate::ItemId(8));
    img.paint_layers.push(PaintLayer::new(PaintLayerId(3)));
    img.paint_layers.push(PaintLayer::new(PaintLayerId(9)));
    assert_eq!(
        layers_for_result(&img, &ImageResultId::Item(crate::ItemId(8))).len(),
        2
    );
    assert!(layers_for_result(&img, &ImageResultId::Item(crate::ItemId(9))).is_empty());
    assert!(layers_for_result(&img, &ImageResultId::Output("new".into())).is_empty());
    let stack = seed_result_from_legacy(&mut img, ImageResultId::Output("picked".into()));
    assert_eq!(stack.layers.len(), 2);
    assert_eq!(stack.layers[0].id, PaintLayerId(9));
    assert_eq!(stack.layers[1].id, PaintLayerId(3));
    assert_eq!(stack.metadata[0].order, 0);
    assert_eq!(stack.metadata[1].order, 1);
    assert!(img.paint_layers.is_empty());
}

#[test]
fn layer_names_and_order_follow_stable_ids() {
    let mut stack = ImageResultLayers::new(ImageResultId::Output("run-2-0".into()));
    let first = PaintLayerId(11);
    let second = PaintLayerId(29);
    stack.add_layer(PaintLayer::new(first));
    stack.add_layer(PaintLayer::new(second));
    stack.layer_meta_mut(first).unwrap().name = "Ink".into();
    assert_eq!(stack.layers[0].id, second, "new layers are topmost");
    assert!(stack.move_layer(1, 0));
    assert_eq!(stack.layers[0].id, first);
    assert_eq!(stack.layer_meta(first).unwrap().name, "Ink");
    assert_eq!(stack.layer_meta(first).unwrap().order, 0);
    assert_eq!(stack.layer_meta(second).unwrap().order, 1);
    assert_eq!(stack.remove_layer(1).unwrap().id, second);
    assert!(stack.layer_meta(second).is_none());
    assert_eq!(stack.layer_meta(first).unwrap().name, "Ink");
}

#[test]
fn clone_to_item_remaps_nested_ids_and_metadata() {
    let mut img = ImageNode::new(crate::ItemId::NONE);
    let result = ImageResultId::Output("inactive-result".into());
    let child = stroke_local(0.1, 0.2);
    let child_id = child.id;
    let mut layer = PaintLayer::new(PaintLayerId(7));
    layer.nodes.push(child);
    layer.prompts.push(PaintLayerPrompt {
        node: child_id,
        prompt: "keep region".into(),
    });
    let stack = ensure_result_layers_mut(&mut img, result.clone());
    stack.layers.push(layer);
    stack.metadata.push(PaintLayerMeta {
        layer: PaintLayerId(7),
        name: "Ink".into(),
        order: 12,
    });
    let mut next = 100;
    let copied = clone_result_layers_to_item(&img, &result, crate::ItemId(22), || {
        let id = crate::scene::NodeId(next);
        next += 1;
        id
    })
    .unwrap();
    assert_eq!(copied.result, ImageResultId::Item(crate::ItemId(22)));
    assert_ne!(copied.layers[0].id, PaintLayerId(7));
    assert_ne!(copied.layers[0].nodes[0].id, child_id);
    assert_eq!(
        copied.layers[0].prompts[0].node,
        copied.layers[0].nodes[0].id
    );
    assert_eq!(copied.metadata[0].name, "Ink");
    assert_eq!(copied.metadata[0].order, 12);
}

#[test]
fn duplicate_remaps_inactive_stacks_too() {
    let mut img = ImageNode::new(crate::ItemId::NONE);
    for output in ["shown", "inactive"] {
        let mut layer = PaintLayer::new(PaintLayerId(7));
        let child = stroke_local(0.2, 0.3);
        let child_id = child.id;
        layer.nodes.push(child);
        layer.prompts.push(PaintLayerPrompt {
            node: child_id,
            prompt: output.into(),
        });
        let stack = ensure_result_layers_mut(&mut img, ImageResultId::Output(output.into()));
        stack.layers.push(layer);
        stack.metadata.push(PaintLayerMeta {
            layer: PaintLayerId(7),
            name: output.into(),
            order: 2,
        });
    }
    let mut copy = img.clone();
    let mut next = 200;
    fresh_layer_node_ids(&mut copy, || {
        let id = crate::scene::NodeId(next);
        next += 1;
        id
    });
    for (old, new) in img.result_layers.iter().zip(&copy.result_layers) {
        assert_ne!(old.layers[0].id, new.layers[0].id);
        assert_ne!(old.layers[0].nodes[0].id, new.layers[0].nodes[0].id);
        assert_eq!(new.layers[0].prompts[0].node, new.layers[0].nodes[0].id);
        assert_eq!(new.metadata[0].layer, new.layers[0].id);
    }
}
