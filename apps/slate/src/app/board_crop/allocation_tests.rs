//! Repeated crop transfers retain existing authored payload storage.
//! Pointer identities prove Node storage reuse, not total allocator activity.
use super::*;
use slate_doc::{
    image_paint::{ImageResultId, ImageResultLayers},
    scene::{ImageNode, PathContour, PathData, PathSeg, Scene},
};

fn path() -> PathData {
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
        fill_rule: slate_doc::scene::PathFillRule::EvenOdd,
        ..PathData::default()
    }
}

fn fixture(masked: bool) -> Node {
    let mut scene = Scene::default();
    let mut layer = slate_doc::PaintLayer::new(slate_doc::PaintLayerId(7));
    for index in 0..32 {
        let mut child = scene.build_node(
            WorldRect::new(index as f32 / 32.0, 0.0, 0.02, 1.0),
            NodeKind::Image(ImageNode::new(slate_doc::ItemId(2))),
        );
        child.clip = Some(path());
        layer.nodes.push(child);
    }
    let mut image = ImageNode::new(slate_doc::ItemId(1));
    image.crop = Crop {
        x: 0.2,
        y: 0.1,
        w: 0.6,
        h: 0.8,
    };
    image.flip_x = true;
    image.flip_y = true;
    image.adjust.brightness = 1.15;
    image.paint_layers.push(layer.clone());
    let mut result = ImageResultLayers::new(ImageResultId::Item(image.item));
    result.add_layer(layer);
    image.result_layers.push(result);
    let mut node = scene.build_node(
        WorldRect::new(20.0, 40.0, 120.0, 80.0),
        NodeKind::Image(image),
    );
    node.rotation_deg = 37.0;
    node.opacity = 0.7;
    node.clip = masked.then(path);
    node
}

fn storage(node: &Node) -> [usize; 10] {
    let NodeKind::Image(image) = &node.kind else {
        panic!("image")
    };
    let result = &image.result_layers[0];
    let clip = node.clip.as_ref();
    [
        clip.map_or(0, |clip| clip.segs.as_ptr() as usize),
        clip.map_or(0, |clip| clip.extra.as_ptr() as usize),
        clip.map_or(0, |clip| clip.extra[0].segs.as_ptr() as usize),
        image.paint_layers.as_ptr() as usize,
        image.paint_layers[0].nodes.as_ptr() as usize,
        image.paint_layers[0].nodes[0]
            .clip
            .as_ref()
            .unwrap()
            .segs
            .as_ptr() as usize,
        image.result_layers.as_ptr() as usize,
        result.layers.as_ptr() as usize,
        result.layers[0].nodes.as_ptr() as usize,
        result.metadata.as_ptr() as usize,
    ]
}

#[test]
fn copy_geometry_reuses_clip_and_paint_result_layer_storage_across_repeated_samples() {
    fn assert_copy<T: Copy>() {}
    assert_copy::<CropGeometry>();
    for masked in [false, true] {
        for pan in [false, true] {
            let before = fixture(masked);
            let source_storage = storage(&before);
            let mut live = before.clone();
            live.opacity = 0.42;
            let live_storage = storage(&live);
            let mut peer_before = fixture(masked);
            peer_before.id = slate_doc::NodeId(before.id.0 + 1);
            peer_before.rect.x += 300.0;
            peer_before.rotation_deg = -23.0;
            let mut peer = peer_before.clone();
            let peer_storage = storage(&peer);
            let NodeKind::Image(image) = &before.kind else {
                panic!("image")
            };
            let primary_crop = image.crop;
            let mut last = None;
            for index in 0..64 {
                let t = (index + 1) as f32 / 64.0;
                let input = if pan {
                    CropGesture::Pan {
                        delta_world: (24.0 * t, -12.0 * t),
                    }
                } else {
                    let target = before.rect.rotate_point(
                        [40.0 + 60.0 * t, before.rect.center().1],
                        before.rotation_deg,
                    );
                    CropGesture::Handle {
                        handle: 7,
                        world: (target[0], target[1]),
                    }
                };
                let geometry = gesture_geometry(&before, input).unwrap();
                assert!(geometry.apply_to(&mut live));
                let peer_geometry =
                    peer_crop_geometry(&peer_before, primary_crop, geometry.crop).unwrap();
                assert!(peer_geometry.apply_to(&mut peer));
                assert_eq!(
                    storage(&before),
                    source_storage,
                    "immutable authored start storage"
                );
                assert_eq!(
                    storage(&live),
                    live_storage,
                    "primary preview retains its nonempty clip/layer vectors"
                );
                assert_eq!(
                    storage(&peer),
                    peer_storage,
                    "peer preview retains its nonempty clip/layer vectors"
                );
                assert_eq!(live.opacity, 0.42, "unrelated live style survives");
                last = Some(input);
            }
            // Full endpoint wrappers remain one-time operations; the repeated
            // loop above consumes only Copy geometry and reused live Nodes.
            let mut exact = apply_gesture(&before, last.unwrap()).unwrap();
            exact.opacity = 0.42;
            assert_eq!(live, exact, "complete endpoint payload and geometry parity");
            let NodeKind::Image(image) = &exact.kind else {
                panic!("image")
            };
            assert_eq!(
                peer,
                apply_peer_crop(&peer_before, primary_crop, image.crop).unwrap()
            );
            let geometry = gesture_geometry(&before, last.unwrap()).unwrap();
            let mut wrong = live.clone();
            wrong.id = slate_doc::NodeId(before.id.0 + 100);
            let unchanged = wrong.clone();
            assert!(!geometry.apply_to(&mut wrong));
            assert_eq!(
                wrong, unchanged,
                "a Copy result cannot update a different identity"
            );
        }
    }
}
