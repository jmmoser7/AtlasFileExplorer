use super::*;
use crate::ids::ItemId;

#[test]
fn model_view_port_binds_any_wired_image() {
    let mut doc = SlateDoc::new("views");
    let model_id = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(1))), 0.0);
    let shot = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(2))), 200.0);
    let binding = infer_binding_with(
        &doc.scene,
        &ConnectorEnd::Anchored {
            node: shot,
            side: Side::Right,
            t: 0.5,
        },
        &ConnectorEnd::Anchored {
            node: model_id,
            side: Side::Left,
            t: MODEL_VIEW_PORT_T,
        },
        &|id| match id.0 {
            1 => Some(std::path::Path::new("box.obj")),
            2 => Some(std::path::Path::new("view.png")),
            _ => None,
        },
    )
    .expect("view wire");
    assert_eq!(binding.slot.as_deref(), Some(InputSlot::View.id()));
}

#[test]
fn a_view_wire_lands_on_any_edge_of_a_model() {
    let mut doc = SlateDoc::new("views-any-edge");
    let model_id = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(1))), 0.0);
    let shot = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(2))), 200.0);
    let paths = |id: ItemId| match id.0 {
        1 => Some(std::path::Path::new("box.obj")),
        2 => Some(std::path::Path::new("view.png")),
        _ => None,
    };
    let shot_end = ConnectorEnd::Anchored {
        node: shot,
        side: Side::Right,
        t: 0.5,
    };
    for (side, t) in [(Side::Left, 0.3), (Side::Top, 0.5), (Side::Right, 0.9)] {
        let model_end = ConnectorEnd::Anchored {
            node: model_id,
            side,
            t,
        };
        let binding =
            infer_binding_with(&doc.scene, &shot_end, &model_end, &paths).expect("view wire");
        assert_eq!(binding.slot.as_deref(), Some(InputSlot::View.id()));
    }
}

#[test]
fn bound_slots_includes_view_when_wired() {
    let mut doc = SlateDoc::new("bound-view");
    let model_id = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(1))), 0.0);
    let shot = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(2))), 200.0);
    let binding = infer_binding_with(
        &doc.scene,
        &ConnectorEnd::Anchored {
            node: shot,
            side: Side::Right,
            t: 0.5,
        },
        &ConnectorEnd::Anchored {
            node: model_id,
            side: Side::Left,
            t: MODEL_VIEW_PORT_T,
        },
        &|id| match id.0 {
            1 => Some(std::path::Path::new("box.obj")),
            2 => Some(std::path::Path::new("view.png")),
            _ => None,
        },
    )
    .unwrap();
    add(
        &mut doc,
        NodeKind::Connector(ConnectorNode {
            crosstalk: None,
            a: ConnectorEnd::Anchored {
                node: shot,
                side: Side::Right,
                t: 0.5,
            },
            b: ConnectorEnd::Anchored {
                node: model_id,
                side: Side::Left,
                t: MODEL_VIEW_PORT_T,
            },
            stroke: Stroke::default(),
            color: None,
            routing: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: WireDisplay::Default,
            binding: Some(binding),
        }),
        400.0,
    );
    assert!(bound_slots(&doc.scene, model_id).contains(&InputSlot::View));
}

#[test]
fn view_wire_binding_source_is_screenshot_image() {
    let mut doc = SlateDoc::new("wire-dir");
    let model_id = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(1))), 0.0);
    let shot = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(2))), 200.0);
    let shot_end = ConnectorEnd::Anchored {
        node: shot,
        side: Side::Right,
        t: 0.5,
    };
    let model_port = ConnectorEnd::Anchored {
        node: model_id,
        side: Side::Left,
        t: MODEL_VIEW_PORT_T,
    };
    let binding = infer_binding_with(&doc.scene, &shot_end, &model_port, &|id| {
        if id.0 == 2 {
            Some(std::path::Path::new("screenshot.png"))
        } else {
            Some(std::path::Path::new("model.obj"))
        }
    })
    .expect("view wire");
    let conn = ConnectorNode {
        crosstalk: None,
        a: shot_end,
        b: model_port,
        stroke: Stroke::default(),
        color: None,
        routing: None,
        arrow_a: false,
        arrow_b: false,
        label: None,
        display: WireDisplay::Default,
        binding: Some(binding),
    };
    let b = conn.binding.as_ref().unwrap();
    assert_eq!(endpoint_node(b.source_end(&conn)), Some(shot));
    assert_eq!(endpoint_node(b.target_end(&conn)), Some(model_id));
}

#[test]
fn context_uses_midpoint_input_and_portal_locators_only_when_wired() {
    let mut doc = SlateDoc::new("midpoint");
    let mut web = PortalNode::unbound_agent("reference", "");
    web.kind = PortalKind::Web;
    web.agent = None;
    web.source = Some(SourceUri {
        locator: "https://example.com/research".into(),
    });
    let source = add(&mut doc, NodeKind::Portal(web), 0.0);
    let target = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("chat", "codex")),
        200.0,
    );
    let output = ConnectorEnd::Anchored {
        node: source,
        side: Side::Right,
        t: 0.5,
    };
    assert!(infer_binding(
        &doc.scene,
        &output,
        &ConnectorEnd::Anchored {
            node: target,
            side: Side::Left,
            t: 0.05
        }
    )
    .is_none());
    connect(&mut doc, source, target);
    let input = snapshot(
        &doc,
        target,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert!(input.context.is_empty());
    assert_eq!(input.wired[0].text, "https://example.com/research");
}

#[test]
fn an_image_generator_accepts_a_model_wire_that_missed_the_midpoint() {
    let mut doc = SlateDoc::new("view");
    let item = doc.add_item(
        std::path::PathBuf::from("pavilion.3dm"),
        "pavilion.3dm",
        10,
        0,
        "k",
    );
    let model = add(&mut doc, NodeKind::Image(ImageNode::new(item)), 0.0);
    let mut portal = PortalNode::unbound_agent("Images", "comfy");
    portal.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let target = add(&mut doc, NodeKind::Portal(portal), 400.0);
    let chat = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("chat", "codex")),
        800.0,
    );
    let from = ConnectorEnd::Anchored {
        node: model,
        side: Side::Right,
        t: 0.5,
    };
    let off_midpoint = |node| ConnectorEnd::Anchored {
        node,
        side: Side::Top,
        t: 0.2,
    };
    // New wires bind on any generator edge; chat cards keep the midpoint.
    let bound = infer_binding(&doc.scene, &from, &off_midpoint(target)).unwrap();
    assert_eq!(bound.kind, InputKind::Images);
    assert!(bound.input_b);
    assert!(infer_binding(&doc.scene, &from, &off_midpoint(chat)).is_none());

    let wire = |a: ConnectorEnd, b: ConnectorEnd, display: WireDisplay| {
        NodeKind::Connector(ConnectorNode {
            crosstalk: None,
            routing: None,
            a,
            b,
            binding: None,
            stroke: Stroke::none(),
            color: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display,
        })
    };
    // An unbound wire saved by an older build still feeds the generator.
    add(
        &mut doc,
        wire(from, off_midpoint(target), WireDisplay::Default),
        0.0,
    );
    let input = snapshot(
        &doc,
        target,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(input.wired.len(), 1);
    assert_eq!(input.wired[0].images, vec!["pavilion.3dm".to_string()]);

    // A free end and a faint provenance wire supply nothing.
    let note = add(&mut doc, text("watercolor"), 0.0);
    let near = doc.scene.node(target).unwrap().rect;
    add(
        &mut doc,
        wire(
            ConnectorEnd::Anchored {
                node: note,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Free {
                point: [near.x + 1.0, near.y + 1.0],
            },
            WireDisplay::Default,
        ),
        0.0,
    );
    add(
        &mut doc,
        wire(
            ConnectorEnd::Anchored {
                node: note,
                side: Side::Right,
                t: 0.5,
            },
            off_midpoint(target),
            WireDisplay::Faint,
        ),
        0.0,
    );
    let input = snapshot(
        &doc,
        target,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(input.wired.len(), 1, "free and provenance ends stay inert");
}

fn add(doc: &mut SlateDoc, kind: NodeKind, x: f32) -> NodeId {
    let node = doc
        .scene
        .build_node(WorldRect::new(x, 0.0, 120.0, 100.0), kind);
    let id = node.id;
    assert!(doc.scene.apply(&SceneCmd::Add {
        index: doc.scene.nodes.len(),
        node
    }));
    id
}
fn text(s: &str) -> NodeKind {
    NodeKind::Text(TextNode {
        text: s.into(),
        style: Default::default(),
        family: Default::default(),
        size: 16.0,
        color: Rgba::BLACK,
        align: Default::default(),
        fill: None,
        stroke: Default::default(),
        agent: None,
    })
}
fn connect(doc: &mut SlateDoc, a: NodeId, b: NodeId) -> NodeId {
    let a = ConnectorEnd::Anchored {
        node: a,
        side: Side::Right,
        t: 0.5,
    };
    let b = ConnectorEnd::Anchored {
        node: b,
        side: Side::Left,
        t: 0.5,
    };
    let binding = infer_binding(&doc.scene, &a, &b);
    add(
        doc,
        NodeKind::Connector(ConnectorNode {
            crosstalk: None,
            routing: None,
            a,
            b,
            binding,
            stroke: Stroke::none(),
            color: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: Default::default(),
        }),
        0.0,
    )
}
#[test]
fn only_sent_context_is_pocketed_and_only_while_hidden() {
    let mut doc = SlateDoc::new("pocket");
    let note = add(&mut doc, text("site plan"), 0.0);
    let a = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("a", "local")),
        250.0,
    );
    let b = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("b", "local")),
        500.0,
    );
    let sent = connect(&mut doc, note, a);
    connect(&mut doc, note, b);
    assert!(consumed_by(&doc.scene, note).is_empty(), "unsent wires");
    if let NodeKind::Connector(c) = &mut doc.scene.node_mut(sent).unwrap().kind {
        c.binding.as_mut().unwrap().consumed = true;
    }
    assert_eq!(consumed_by(&doc.scene, note), vec![(sent, a)]);
    assert!(pocketed(&doc.scene, a).is_empty(), "visible context");
    doc.scene.node_mut(note).unwrap().hidden = true;
    assert_eq!(pocketed(&doc.scene, a), vec![note]);
    assert!(pocketed(&doc.scene, b).is_empty(), "b never sent it");
    assert_eq!(pocket_holders(&doc.scene), BTreeSet::from([a]));
}
