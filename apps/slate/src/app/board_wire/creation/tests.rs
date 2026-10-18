use super::*;
use crate::app::board_flow::tests::{generator_with_output, place};
use slate_doc::scene::TextNode;

#[test]
fn words_wired_to_an_ordinary_image_enable_generation_in_one_undo() {
    for sticky in [false, true] {
        let (mut h, _) = generator_with_output(if sticky {
            "sticky-image-wire"
        } else {
            "text-image-wire"
        });
        let image = place(&mut h, "target.png", 800.0);
        let mut text = TextNode {
            text: "a warm sunset".into(),
            style: Default::default(),
            family: Default::default(),
            size: 16.0,
            color: slate_doc::scene::Rgba::BLACK,
            align: Default::default(),
            fill: None,
            stroke: Stroke::none(),
            agent: None,
        };
        if sticky {
            text.fill = Some(slate_doc::scene::Rgba::WHITE);
        }
        let node = h
            .app
            .doc_mut()
            .scene
            .build_node(WorldRect::new(0.0, 0.0, 100.0, 80.0), NodeKind::Text(text));
        let source = node.id;
        h.app.add_nodes(vec![node]);
        let before = h.app.doc().scene.node(image).unwrap().clone();
        let depth = h.app.tab().journal.undo_depth();
        let wire = h
            .app
            .add_connector(
                ConnectorEnd::Anchored {
                    node: source,
                    side: Side::Right,
                    t: 0.5,
                },
                ConnectorEnd::Anchored {
                    node: image,
                    side: Side::Left,
                    t: 0.5,
                },
            )
            .unwrap();
        let NodeKind::Connector(connector) = &h.app.doc().scene.node(wire).unwrap().kind else {
            unreachable!()
        };
        let binding = connector.binding.as_ref().unwrap();
        assert_eq!(binding.kind, slate_doc::agent_inputs::InputKind::Text);
        assert_eq!(binding.slot.as_deref(), Some("prompt"));
        let after = h.app.doc().scene.node(image).unwrap();
        assert_eq!(after.rect, before.rect);
        let agent = slate_doc::agent_chat::agent(after).unwrap();
        assert_eq!(agent.view, atlas_agent::PortalView::Images);
        assert!(agent.seed.as_ref().unwrap().source.ends_with("target.png"));
        assert!(
            h.app.agents.dispatched.is_empty(),
            "connecting alone never runs a model"
        );
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        let tab = h.app.tab_mut();
        assert!(tab.journal.undo(&mut tab.doc.scene));
        assert!(h.app.doc().scene.node(wire).is_none());
        assert_eq!(h.app.doc().scene.node(image), Some(&before));
        let tab = h.app.tab_mut();
        assert!(tab.journal.redo(&mut tab.doc.scene));
        assert!(slate_doc::agent_inputs::is_image_generator(
            &h.app.doc().scene,
            image
        ));
    }
}

#[test]
fn imported_words_snap_to_the_ordinary_images_text_input() {
    let (mut h, _) = generator_with_output("document-image-wire");
    let source = place(&mut h, "brief.txt", 0.0);
    let target = place(&mut h, "target.png", 800.0);
    let rect = h.app.doc().scene.node(target).unwrap().rect;
    let snap = h.app.wire_snap_target(
        Pos2::new(rect.x + 60.0, rect.y + 60.0),
        Some((source, Side::Right, 0.5)),
    );
    assert_eq!(snap, Some((target, Side::Left, 0.5)));
    let id = h
        .app
        .add_connector(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: target,
                side: Side::Left,
                t: 0.5,
            },
        )
        .unwrap();
    let NodeKind::Connector(connector) = &h.app.doc().scene.node(id).unwrap().kind else {
        unreachable!()
    };
    assert_eq!(
        connector.binding.as_ref().unwrap().slot.as_deref(),
        Some("prompt")
    );
}
