use slate_doc::{
    image_paint::{layer_node_from_world, layer_node_to_world, PaintLayer, PaintLayerId},
    scene::{ImageNode, NodeKind, Scene, SceneCmd, ShapeKind, ShapeNode, Stroke, WorldRect},
    ItemId, SlateDoc,
};

#[test]
fn paint_layer_patch_undo_round_trip() {
    let mut doc = SlateDoc::new("layers");
    let item = doc.add_item("/tmp/x.png".into(), "x.png", 0, 0, "");
    let mut scene = doc.scene.clone();
    let node = scene.build_node(
        WorldRect::new(0.0, 0.0, 200.0, 100.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    let id = node.id;
    scene.apply(&SceneCmd::Add { index: 0, node });
    let before = scene.node(id).unwrap().clone();
    let mut after = before.clone();
    let NodeKind::Image(ref mut img) = after.kind else {
        panic!("image");
    };
    img.paint_layers.push(PaintLayer::new(PaintLayerId(1)));
    scene.apply(&SceneCmd::Patch {
        before: Box::new(before.clone()),
        after: Box::new(after),
    });
    assert_eq!(scene.node(id).unwrap().kind.image_layers_len(), 1);
    scene.apply(&SceneCmd::Patch {
        before: Box::new(scene.node(id).unwrap().clone()),
        after: Box::new(before),
    });
    assert_eq!(scene.node(id).unwrap().kind.image_layers_len(), 0);
}

#[test]
fn layer_stroke_follows_image_move() {
    let mut scene = Scene::default();
    let host = scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(ItemId(1))),
    );
    let id = host.id;
    scene.apply(&SceneCmd::Add {
        index: 0,
        node: host.clone(),
    });
    let stroke = scene.build_node(
        WorldRect::new(10.0, 10.0, 20.0, 20.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            sides: 6,
            fill: None,
            stroke: Stroke::default(),
            corner: Default::default(),
            flip: false,
            path: None,
            text: None,
        }),
    );
    let NodeKind::Image(ref host_img) = host.kind else {
        panic!();
    };
    let local = layer_node_from_world(&host, host_img, &stroke);
    let mut patched = host.clone();
    let NodeKind::Image(ref mut img) = patched.kind else {
        panic!();
    };
    img.paint_layers.push(PaintLayer {
        id: PaintLayerId(1),
        opacity: 1.0,
        visible: true,
        nodes: vec![local],
    });
    scene.apply(&SceneCmd::Patch {
        before: Box::new(host.clone()),
        after: Box::new(patched),
    });
    let mut moved = scene.node(id).unwrap().clone();
    moved.rect.x += 50.0;
    scene.apply(&SceneCmd::Patch {
        before: Box::new(scene.node(id).unwrap().clone()),
        after: Box::new(moved.clone()),
    });
    let host = scene.node(id).unwrap();
    let NodeKind::Image(img) = &host.kind else {
        panic!();
    };
    let world = layer_node_to_world(host, img, &img.paint_layers[0].nodes[0]);
    assert!((world.rect.x - 60.0).abs() < 1e-3);
}

#[test]
fn layer_node_add_undo_round_trip() {
    use slate_doc::image_paint::PaintLayer;
    use slate_doc::scene::{Corner, ShapeKind, ShapeNode, Stroke};
    let mut scene = Scene::default();
    let host = scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(ItemId(1))),
    );
    let host_id = host.id;
    scene.apply(&SceneCmd::Add {
        index: 0,
        node: host,
    });
    let layer_id = PaintLayerId(1);
    let stroke = scene.build_node(
        WorldRect::new(0.1, 0.1, 0.2, 0.2),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            sides: 6,
            fill: None,
            stroke: Stroke::none(),
            corner: Corner::Square,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let mut before = scene.node(host_id).unwrap().clone();
    let NodeKind::Image(ref mut img) = before.kind else {
        panic!();
    };
    img.paint_layers.push(PaintLayer::new(layer_id));
    scene.apply(&SceneCmd::Patch {
        before: Box::new(scene.node(host_id).unwrap().clone()),
        after: Box::new(before),
    });
    assert!(scene.apply(&SceneCmd::LayerNodeAdd {
        host: host_id,
        layer: layer_id,
        index: 0,
        node: stroke,
    }));
    let NodeKind::Image(img) = &scene.node(host_id).unwrap().kind else {
        panic!();
    };
    assert_eq!(img.paint_layers[0].nodes.len(), 1);
    let removed = img.paint_layers[0].nodes[0].clone();
    assert!(scene.apply(&SceneCmd::LayerNodeRemove {
        host: host_id,
        layer: layer_id,
        index: 0,
        node: removed,
    }));
    let NodeKind::Image(img) = &scene.node(host_id).unwrap().kind else {
        panic!();
    };
    assert!(img.paint_layers[0].nodes.is_empty());
}

trait ImageLayersLen {
    fn image_layers_len(&self) -> usize;
}

impl ImageLayersLen for NodeKind {
    fn image_layers_len(&self) -> usize {
        match self {
            NodeKind::Image(i) => i.paint_layers.len(),
            _ => 0,
        }
    }
}
