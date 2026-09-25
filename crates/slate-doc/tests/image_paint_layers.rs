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
