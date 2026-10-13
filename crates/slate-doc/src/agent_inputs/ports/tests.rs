use super::*;
use std::path::Path;

#[test]
fn ordinary_image_inputs_bind_words_from_text_stickies_and_imported_documents() {
    for source_kind in 0..3 {
        let mut doc = SlateDoc::default();
        let picture = doc.add_item("photo.png".into(), "photo", 0, 0, "photo");
        let words = doc.add_item("brief.docx".into(), "brief", 0, 0, "brief");
        let target = doc.scene.build_node(
            WorldRect::new(400.0, 0.0, 200.0, 120.0),
            NodeKind::Image(ImageNode::new(picture)),
        );
        let target_id = target.id;
        doc.scene.nodes.push(target);
        let kind = if source_kind == 2 {
            NodeKind::Image(ImageNode::new(words))
        } else {
            NodeKind::Text(TextNode {
                text: "warm afternoon".into(),
                style: Default::default(),
                family: Default::default(),
                size: 16.0,
                color: Rgba::BLACK,
                align: Default::default(),
                fill: (source_kind == 1).then_some(Rgba::WHITE),
                stroke: Stroke::none(),
                agent: None,
            })
        };
        let source = doc
            .scene
            .build_node(WorldRect::new(0.0, 0.0, 200.0, 120.0), kind);
        let source_id = source.id;
        doc.scene.nodes.push(source);
        let paths = |id| doc.item(id).map(|item| item.path.as_path());
        let ports = input_ports_with(&doc.scene, target_id, &paths);
        assert_eq!(ports.len(), 3);
        assert_eq!(
            ports.iter().map(|port| port.slot).collect::<Vec<_>>(),
            vec![InputSlot::Media, InputSlot::Prompt, InputSlot::Style]
        );
        assert_eq!(
            default_port(&doc.scene, target_id, source_id, &paths)
                .unwrap()
                .slot,
            InputSlot::Prompt
        );
        let output = ConnectorEnd::Anchored {
            node: source_id,
            side: Side::Right,
            t: OUTPUT_T,
        };
        let input = ConnectorEnd::Anchored {
            node: target_id,
            side: Side::Left,
            t: ports[1].t,
        };
        let binding = infer_binding_with(&doc.scene, &output, &input, &paths).unwrap();
        assert!(binding.input_b);
        assert_eq!(binding.kind, InputKind::Text);
        assert_eq!(binding.slot.as_deref(), Some("prompt"));
        let wrong_input = ConnectorEnd::Anchored {
            node: target_id,
            side: Side::Left,
            t: ports[0].t,
        };
        assert!(infer_binding_with(&doc.scene, &output, &wrong_input, &paths).is_none());
    }
}

#[test]
fn candidate_image_ports_do_not_turn_other_media_into_generators() {
    let mut scene = Scene::default();
    for path in ["report.pdf", "clip.mp4", "brief.txt", "data.xlsx"] {
        let node = scene.build_node(
            WorldRect::new(0.0, 0.0, 100.0, 100.0),
            NodeKind::Image(ImageNode::new(crate::ItemId(1))),
        );
        assert!(!image_generation_target(&node, &|_| Some(Path::new(path))));
        scene.nodes.push(node.clone());
        assert!(input_ports_with(&scene, node.id, &|_| Some(Path::new(path))).is_empty());
    }
}
