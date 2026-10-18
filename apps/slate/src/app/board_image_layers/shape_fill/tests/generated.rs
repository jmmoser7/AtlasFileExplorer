//! Generated output intake uses the same move/Alt and Replace/Add journals.
use super::*;
use crate::app::board_image_layers::{ImageDropChoice, ImageDropOffer, ImageDropSource};

fn generated(h: &mut Harness, id: NodeId, name: &str) -> std::path::PathBuf {
    let path = h.base.join(format!("{name}.png"));
    image::RgbaImage::from_pixel(200, 100, image::Rgba([220, 40, 70, 255]))
        .save(&path)
        .unwrap();
    h.app.patch_nodes(&[id], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.item = slate_doc::ItemId::NONE;
        image.agent = Some(Box::new(
            serde_json::from_value(serde_json::json!({
                "provider": "comfy", "session": "drop-fixture", "view": "images"
            }))
            .unwrap(),
        ));
        image.result_layers[0].result = ImageResultId::Output(name.into());
    });
    h.app.agents.sessions.insert(
        id,
        std::sync::Arc::new(atlas_ai::agent::AgentSession {
            activity: Vec::new(),
            usage: None,
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: atlas_ai::agent::AgentStatus::Idle,
            provider: "comfy".into(),
            turns: vec![],
            updated_at: 0,
            bundle: atlas_ai::agent::ImageBundle {
                images: vec![atlas_ai::agent::ImageOutput {
                    id: name.into(),
                    source: path.to_string_lossy().into_owned(),
                    ..Default::default()
                }],
            },
            request: String::new(),
        }),
    );
    h.app.note_scene_change();
    path
}

#[test]
fn generated_to_placed_replace_and_add_support_move_and_alt() {
    for alt in [false, true] {
        for choice in [ImageDropChoice::Replace, ImageDropChoice::AddLayer] {
            let (mut h, source, target) = fixture("generated_drop", ShapeKind::Rect);
            let original = image(&h, source).clone();
            h.app.patch_nodes(&[target], |node| {
                node.kind = NodeKind::Image(original.clone())
            });
            let output = generated(&mut h, source, "new-output");
            let before = h.app.doc().scene.nodes.clone();
            let depth = h.app.tab().journal.undo_depth();
            let mods = egui::Modifiers {
                alt,
                ..Default::default()
            };
            h.app.alt_down = alt;
            h.app.board_sel = std::iter::once(source).collect();
            let start = Pos2::new(100.0, 50.0);
            let end = Pos2::new(450.0, 50.0);
            let xf = h.app.board_xf();
            h.app.board_drag = h.app.begin_gesture_for_test(xf.w2s(start), start, mods);
            h.app.update_gesture_for_test(end, mods);
            let offer = h
                .app
                .image_drop
                .as_mut()
                .expect("generated source is eligible");
            assert!(matches!(
                offer.source,
                ImageDropSource::GeneratedNode { .. }
            ));
            offer.highlight = Some(choice);
            h.app.end_gesture_for_test(end, Some(xf.w2s(end)), mods);
            let base = image(&h, target);
            let placed = match choice {
                ImageDropChoice::Replace => base.item,
                ImageDropChoice::AddLayer => {
                    let result = h.app.image_result_id(target).unwrap();
                    let layer = &slate_doc::image_paint::layers_for_result(base, &result)[0];
                    match &layer.nodes[0].kind {
                        NodeKind::Image(child) => child.item,
                        _ => unreachable!(),
                    }
                }
                _ => unreachable!(),
            };
            assert_eq!(h.app.item_path(placed), Some(output));
            assert_eq!(h.app.doc().scene.node(source).is_some(), alt);
            assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
            assert_eq!(base.adjust, original.adjust);
            assert_eq!(base.crop, original.crop);
            undo(&mut h);
            assert_eq!(h.app.doc().scene.nodes, before);
        }
    }
}

#[test]
fn replace_generated_target_adopts_new_base_and_preserves_result_stacks() {
    let (mut h, source, target) = fixture("generated_target", ShapeKind::Rect);
    let original = image(&h, source).clone();
    h.app.patch_nodes(&[target], |node| {
        node.kind = NodeKind::Image(original.clone())
    });
    generated(&mut h, target, "old-output");
    h.app.patch_nodes(&[target], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        let mut inactive = ImageResultLayers::new(ImageResultId::Output("inactive".into()));
        inactive.add_layer(PaintLayer::new(PaintLayerId(80)));
        image.result_layers.push(inactive);
    });
    let old = image(&h, target).clone();
    let before = h.app.doc().scene.nodes.clone();
    assert_eq!(
        h.app.image_drop_target_at(Pos2::new(450.0, 50.0)),
        Some(target)
    );
    h.app.image_drop = Some(ImageDropOffer {
        target,
        source: ImageDropSource::Node(source),
        highlight: Some(ImageDropChoice::Replace),
        row: None,
    });
    let item = image(&h, source).item;
    assert!(h.app.commit_image_drop(Some(item), false));
    let replaced = image(&h, target);
    assert!(replaced.agent.is_none());
    assert_eq!(h.app.image_item_path(target), h.app.item_path(item));
    assert_eq!(
        h.app.image_result_id(target),
        Some(ImageResultId::Item(item))
    );
    let shown =
        slate_doc::image_paint::result_layers(replaced, &ImageResultId::Item(item)).unwrap();
    assert_eq!(shown.layers, old.result_layers[0].layers);
    assert_eq!(shown.metadata, old.result_layers[0].metadata);
    assert_eq!(replaced.result_layers[1], old.result_layers[1]);
    assert_eq!(replaced.adjust, old.adjust);
    assert_eq!(replaced.crop, old.crop);
    undo(&mut h);
    assert_eq!(h.app.doc().scene.nodes, before);
    assert!(
        h.app.agent_shown_path(target).is_some(),
        "undo recovers runtime output"
    );
}

#[test]
fn generated_target_accepts_add_layer_in_its_active_result_only() {
    let (mut h, source, target) = fixture("generated_add_target", ShapeKind::Rect);
    let original = image(&h, source).clone();
    h.app.patch_nodes(&[target], |node| {
        node.kind = NodeKind::Image(original.clone())
    });
    let path = generated(&mut h, target, "target-output");
    let before = h.app.doc().scene.nodes.clone();
    let active = h.app.image_result_id(target).unwrap();
    h.app.image_drop = Some(ImageDropOffer {
        target,
        source: ImageDropSource::Node(source),
        highlight: Some(ImageDropChoice::AddLayer),
        row: None,
    });
    assert!(h.app.commit_image_drop(Some(original.item), false));
    assert_eq!(h.app.image_item_path(target), Some(path));
    assert_eq!(h.app.image_result_id(target), Some(active.clone()));
    assert_eq!(
        slate_doc::image_paint::layers_for_result(image(&h, target), &active)[0]
            .nodes
            .len(),
        1
    );
    undo(&mut h);
    assert_eq!(h.app.doc().scene.nodes, before);
}

#[test]
fn alt_generated_shape_fill_carries_runtime_output_and_full_stack_durably() {
    let (mut h, source, target) = fixture("generated_shape", ShapeKind::Ellipse);
    let item = image(&h, source).item;
    let output = generated(&mut h, source, "shape-output");
    let child = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.1, 0.1, 0.5, 0.5),
        NodeKind::Image(ImageNode::new(item)),
    );
    h.app.patch_nodes(&[source], |node| {
        let NodeKind::Image(image) = &mut node.kind else {
            unreachable!()
        };
        image.result_layers[0].layers[0].nodes.push(child.clone());
    });
    h.app.alt_down = true;
    h.app.board_sel = std::iter::once(source).collect();
    let mods = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let start = Pos2::new(100.0, 50.0);
    let end = Pos2::new(450.0, 50.0);
    let xf = h.app.board_xf();
    h.app.board_drag = h.app.begin_gesture_for_test(xf.w2s(start), start, mods);
    h.app.update_gesture_for_test(end, mods);
    assert!(h.app.image_drop_armed());
    h.app.end_gesture_for_test(end, Some(xf.w2s(end)), mods);
    assert_eq!(h.app.image_item_path(target), Some(output));
    let filled = &image(&h, target).result_layers[0];
    let original = &image(&h, source).result_layers[0];
    assert_eq!(filled.result, original.result);
    assert_ne!(
        filled.layers[0].id, original.layers[0].id,
        "an independent copy owns fresh layer identities"
    );
    assert_eq!(filled.metadata.len(), original.metadata.len());
    for (copy, source) in filled.metadata.iter().zip(&original.metadata) {
        assert_eq!((&copy.name, copy.order), (&source.name, source.order));
        let index = original
            .layers
            .iter()
            .position(|layer| layer.id == source.layer)
            .unwrap();
        assert_eq!(
            copy.layer, filled.layers[index].id,
            "metadata follows the copied layer identity"
        );
    }
    assert_eq!(
        filled.layers[0].nodes[0].kind,
        original.layers[0].nodes[0].kind
    );
    assert_ne!(
        filled.layers[0].nodes[0].id, original.layers[0].nodes[0].id,
        "Alt copy owns fresh nested IDs"
    );
    h.app.agents.sessions.clear();
    h.app.note_scene_change();
    assert!(
        h.app.image_item_path(target).is_some(),
        "saved seed owns the shown output"
    );
}
