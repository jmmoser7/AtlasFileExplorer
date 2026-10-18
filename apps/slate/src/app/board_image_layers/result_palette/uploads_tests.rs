use super::*;
use crate::app::tests::Harness;

#[test]
fn stale_or_deleted_host_composite_cannot_replace_current_layers() {
    let mut h = Harness::new("stale-layer-composite");
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(slate_doc::ItemId::NONE)),
    );
    let id = h.app.add_nodes(vec![node])[0];
    let node = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Image(image) = &node.kind else {
        panic!("image fixture")
    };
    let result = h.app.image_result_id(id).unwrap();
    let key = paint_layer_cache_key(node, image, &result, h.app.scene_gen, 256);
    assert!(h.app.host_raster_current(id, key, 256));
    h.app.scene_gen += 1;
    assert!(!h.app.host_raster_current(id, key, 256));
    assert!(!h.app.host_raster_current(NodeId(u64::MAX), key, 256));
}
