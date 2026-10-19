use super::*;
use slate_doc::scene::{
    Corner, FrameNode, NodeKind, Rgba, ShapeKind, ShapeNode, ShapeText, Stroke, TextAlign,
    TextNode, TextStyle, Typeface,
};
use slate_doc::{GroupId, TagId};
use std::collections::BTreeMap;

#[test]
fn participant_descriptors_include_slide_order_and_basic_authored_styles() {
    let mut h = host();
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: "Dot".into(),
        hat: "none".into(),
        provider: "local".into(),
        retired: false,
    };
    h.app.join_presence(profile, None).unwrap();
    let frame = FrameNode {
        title: "Slide 3".into(),
        order: 3,
        fill: Rgba([12, 24, 36, 200]),
        fill_authored: true,
        assignments: BTreeMap::from([(GroupId(7), TagId(11))]),
        stroke: Stroke::default(),
        corner: Corner::Rounded { radius: 9.0 },
    };
    let mut shape = ShapeNode {
        shape: ShapeKind::Rect,
        fill: Some(Rgba([1, 2, 3, 255])),
        stroke: Stroke::default(),
        corner: Corner::Chamfer { cut: 4.0 },
        sides: 4,
        phase_deg: 0.0,
        flip: false,
        path: None,
        text: Some(ShapeText {
            body: "Label".into(),
            style: TextStyle {
                bold: true,
                ..Default::default()
            },
            family: Typeface::Serif,
            size: 18.0,
            color: Rgba([220, 210, 200, 255]),
            align: TextAlign::Right,
        }),
    };
    shape.stroke.width = 2.0;
    let text = TextNode {
        text: "Body".into(),
        style: TextStyle::default(),
        family: Typeface::Mono,
        size: 22.0,
        color: Rgba([40, 50, 60, 255]),
        align: TextAlign::Center,
        fill: None,
        stroke: Stroke::default(),
        agent: None,
    };
    let frame = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Frame(frame),
    );
    let shape = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 30.0, 30.0),
        NodeKind::Shape(shape),
    );
    let text = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 30.0, 30.0),
        NodeKind::Text(text),
    );
    let mut shape = shape;
    shape.opacity = 0.45;
    let frame_id = frame.id;
    let shape_id = shape.id;
    let text_id = text.id;
    h.app.add_nodes(vec![frame, shape, text]);
    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();

    let frame = h
        .app
        .presence_object_descriptor(h.app.doc().scene.node(frame_id).unwrap(), grant);
    assert_eq!(frame["frame"]["order"], 3);
    assert_eq!(frame["frame"]["assignments"][0]["group_id"], 7);
    assert_eq!(frame["frame"]["assignments"][0]["tag_id"], 11);
    assert_eq!(frame["frame"]["corner"]["rounded"]["radius"], 9.0);

    let shape = h
        .app
        .presence_object_descriptor(h.app.doc().scene.node(shape_id).unwrap(), grant);
    assert_eq!(shape["z_index"], 1);
    assert!((shape["opacity"].as_f64().unwrap() - 0.45).abs() < 1e-6);
    assert_eq!(shape["style"]["fill"][0], 1);
    assert_eq!(shape["style"]["corner"]["chamfer"]["cut"], 4.0);
    assert_eq!(shape["text_style"]["font"], "Serif");
    assert_eq!(shape["text_style"]["size"], 18.0);
    assert_eq!(shape["text_style"]["alignment"], "right");

    let text = h
        .app
        .presence_object_descriptor(h.app.doc().scene.node(text_id).unwrap(), grant);
    assert_eq!(text["style"]["font"], "Mono");
    assert_eq!(text["style"]["size"], 22.0);
    assert_eq!(text["style"]["alignment"], "center");
}

#[test]
fn linked_image_descriptors_expose_current_bounded_tag_assignment_ids() {
    let mut h = host();
    let profile = StoredAgent {
        id: atlas_agent::request_id(),
        name: "Dot".into(),
        hat: "none".into(),
        provider: "local".into(),
        retired: false,
    };
    h.app.join_presence(profile, None).unwrap();
    let group = h.app.doc_mut().add_group("Subject");
    let tag = h
        .app
        .doc_mut()
        .add_tag(group, "Portrait", [12, 34, 56])
        .unwrap();
    let item = h.app.doc_mut().add_item(
        h.base.join("portrait.png"),
        "portrait.png",
        123,
        456,
        "portrait-cache-key",
    );
    assert!(h.app.doc_mut().assign(item, tag));
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::WorldRect::new(0.0, 0.0, 80.0, 60.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let node_id = node.id;
    h.app.add_nodes(vec![node]);

    let grant = h.app.presence.agents[0].grant.as_ref().unwrap();
    let descriptor = h
        .app
        .presence_object_descriptor(h.app.doc().scene.node(node_id).unwrap(), grant);
    let tags = &descriptor["item"]["tag_assignments"];
    assert_eq!(tags["assignment_count"], 1);
    assert_eq!(tags["assignments"][0]["group_id"], group.0);
    assert_eq!(tags["assignments"][0]["tag_id"], tag.0);
    assert_eq!(tags["assignments_truncated"], false);
}
