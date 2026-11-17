use super::*;
use crate::app::tests::Harness;

#[test]
fn committing_a_hosted_brush_stroke_keeps_existing_marks_visible_until_raster_replacement() {
    use crate::app::tests::{image_paint_board, layer_marks, press_drag_release_frames};

    let (mut h, id) = image_paint_board(
        "layer-composite-handoff",
        WorldRect::new(0.0, 0.0, 400.0, 300.0),
        0.0,
        &[Pos2::new(30.0, 30.0), Pos2::new(100.0, 80.0)],
    );
    assert_eq!(layer_marks(&h, id)[0].len(), 1);
    let tab = h.app.tab().id;
    let result = h.app.image_result_id(id).unwrap();
    let texture = h.ctx.load_texture(
        "previous-host-brush-composite",
        egui::ColorImage::new([2, 2], Color32::RED),
        egui::TextureOptions::LINEAR,
    );
    h.app.paint_layer_texture_cache.insert(
        id,
        PaintLayerTextureCache {
            tab,
            result,
            key: 0,
            texture,
        },
    );
    let unrelated = h.app.doc_mut().scene.build_node(
        WorldRect::new(600.0, 600.0, 100.0, 100.0),
        NodeKind::Image(ImageNode::new(slate_doc::ItemId::NONE)),
    );
    h.app.add_nodes(vec![unrelated]);
    assert!(
        h.app.paint_layer_texture_cache.contains_key(&id),
        "editing an unrelated card does not blank this image's committed ink"
    );
    press_drag_release_frames(
        &mut h,
        &[Pos2::new(200.0, 110.0), Pos2::new(255.0, 190.0)],
        egui::Modifiers::NONE,
        |_| {},
    );
    assert_eq!(layer_marks(&h, id)[0].len(), 2);
    assert!(h.app.paint_layer_texture_cache.contains_key(&id));
    h.app.board_undo();
    assert_eq!(layer_marks(&h, id)[0].len(), 1);
    assert!(!h.app.paint_layer_texture_cache.contains_key(&id));
}

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
