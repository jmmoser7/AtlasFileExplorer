#[test]
fn a_placed_text_document_carries_words_to_a_prompt_port() {
    use crate::ids::ItemId;
    use std::path::Path;
    let mut doc = SlateDoc::new("doc words");
    let item = doc.add_item(std::path::PathBuf::from("brief.md"), "brief.md", 10, 0, "k");
    let brief = add(&mut doc, NodeKind::Image(ImageNode::new(item)), 0.0);
    let mut portal = PortalNode::unbound_agent("Images", "comfy");
    portal.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let generator = add(&mut doc, NodeKind::Portal(portal), 400.0);
    let paths = |id: ItemId| (id == item).then_some(Path::new("brief.md"));
    assert_eq!(wire_kind(&doc.scene, brief, &paths), Some(InputKind::Text));
    assert_eq!(
        wire_kind(&doc.scene, brief, &|_| None),
        Some(InputKind::Images)
    );
    assert_eq!(
        default_port(&doc.scene, generator, brief, &paths)
            .unwrap()
            .slot,
        InputSlot::Prompt
    );
    let a = ConnectorEnd::Anchored {
        node: brief,
        side: Side::Right,
        t: 0.5,
    };
    let b = ConnectorEnd::Anchored {
        node: generator,
        side: Side::Left,
        t: 0.5,
    };
    let binding = infer_binding_with(&doc.scene, &a, &b, &paths).unwrap();
    assert_eq!(binding.kind, InputKind::Text);
    assert_eq!(binding.slot.as_deref(), Some("prompt"));
    add(
        &mut doc,
        NodeKind::Connector(ConnectorNode {
            crosstalk: None,
            routing: None,
            a,
            b,
            binding: Some(binding),
            stroke: Stroke::none(),
            color: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: Default::default(),
        }),
        0.0,
    );
    // The caller reads the document; the snapshot only carries its words.
    let words = BTreeMap::from([(
        brief,
        ContextItem {
            node: brief.0,
            text: "a courtyard in morning light".into(),
            images: vec![],
            outputs: Default::default(),
            active: None,
            depth: None,
            slot: None,
        },
    )]);
    let snap = snapshot(&doc, generator, AgentContextScope::Selection, &[], &words).unwrap();
    assert_eq!(snap.wired[0].text, "a courtyard in morning light");
    assert_eq!(snap.wired[0].port(), InputSlot::Prompt);
}

#[test]
fn flow_ports_bind_by_slot_and_refuse_the_wrong_kind() {
    let mut doc = SlateDoc::new("ports");
    let note = add(&mut doc, text("watercolor"), 0.0);
    let item = doc.add_item(
        std::path::PathBuf::from("style.jpg"),
        "style.jpg",
        10,
        0,
        "k",
    );
    let picture = add(&mut doc, NodeKind::Image(ImageNode::new(item)), 0.0);
    let mut portal = PortalNode::unbound_agent("Images", "comfy");
    portal.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let generator = add(&mut doc, NodeKind::Portal(portal), 400.0);
    let mut block = PortalNode::unbound_agent("Text", "ollama");
    block.agent.as_mut().unwrap().view = atlas_agent::PortalView::Text;
    let block = add(&mut doc, NodeKind::Portal(block), 800.0);
    let ports = input_ports(&doc.scene, generator);
    assert_eq!(
        ports.iter().map(|p| p.slot).collect::<Vec<_>>(),
        [InputSlot::Media, InputSlot::Prompt, InputSlot::Style]
    );
    assert_eq!(input_ports(&doc.scene, block).len(), 2);
    assert!(is_flow_node(&doc.scene, block));
    let out = |node| ConnectorEnd::Anchored {
        node,
        side: Side::Right,
        t: 0.5,
    };
    let port = |node, slot: InputSlot| ConnectorEnd::Anchored {
        node,
        side: Side::Left,
        t: if node == block {
            &TEXT_PORTS[..]
        } else {
            &GENERATOR_PORTS[..]
        }
        .iter()
        .find(|p| p.slot == slot)
        .unwrap()
        .t,
    };
    let style = infer_binding(
        &doc.scene,
        &out(picture),
        &port(generator, InputSlot::Style),
    )
    .unwrap();
    assert_eq!(style.kind, InputKind::Images);
    assert_eq!(style.slot.as_deref(), Some("style"));
    // A picture cannot land on Prompt, and nothing lands on the output port.
    assert!(infer_binding(
        &doc.scene,
        &out(picture),
        &port(generator, InputSlot::Prompt)
    )
    .is_none());
    assert!(infer_binding(&doc.scene, &out(note), &out(generator)).is_none());
    // Drawn backward from the port, the wire still feeds the generator.
    let backward =
        infer_binding(&doc.scene, &port(generator, InputSlot::Prompt), &out(note)).unwrap();
    assert!(!backward.input_b);
    assert_eq!(backward.slot.as_deref(), Some("prompt"));
    // A generator's output feeds a text block's picture port.
    let described =
        infer_binding(&doc.scene, &out(generator), &port(block, InputSlot::Media)).unwrap();
    assert_eq!(described.kind, InputKind::Images);

    assert_eq!(
        default_port(&doc.scene, generator, picture, &|_| None)
            .unwrap()
            .slot,
        InputSlot::Media
    );
    assert_eq!(
        default_port(&doc.scene, generator, note, &|_| None)
            .unwrap()
            .slot,
        InputSlot::Prompt
    );
    let wire = |scene: &Scene, a, b| {
        let binding = infer_binding(scene, &a, &b);
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
        })
    };
    let media = wire(&doc.scene, out(picture), port(generator, InputSlot::Media));
    add(&mut doc, media, 0.0);
    assert_eq!(
        default_port(&doc.scene, generator, picture, &|_| None)
            .unwrap()
            .slot,
        InputSlot::Style,
        "a second picture takes the free Style port"
    );
    let prompt = wire(&doc.scene, out(note), port(generator, InputSlot::Prompt));
    add(&mut doc, prompt, 0.0);
    let snap = snapshot(
        &doc,
        generator,
        AgentContextScope::Selection,
        &[],
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        snap.wired.iter().map(|w| w.slot).collect::<Vec<_>>(),
        [Some(InputSlot::Media), Some(InputSlot::Prompt)]
    );
}

/// Generators and text blocks open as the media they make, keeping their
/// id, binding and wires; an agent picture feeds its newest result until a
/// person picks one, and an agent note its reply until they edit it.
#[test]
fn agent_cards_become_the_media_they_make() {
    let mut doc = SlateDoc::new("t");
    let note = add(&mut doc, text("a harbour at dusk"), 0.0);
    let mut portal = PortalNode::unbound_agent("Images", "comfy");
    portal.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    portal.agent.as_mut().unwrap().instruction = "watercolor".into();
    let generator = add(&mut doc, NodeKind::Portal(portal), 400.0);
    let mut block = PortalNode::unbound_agent("Text", "ollama");
    block.agent.as_mut().unwrap().view = atlas_agent::PortalView::Text;
    let block = add(&mut doc, NodeKind::Portal(block), 800.0);
    let prompt_t = GENERATOR_PORTS[1].t;
    let wire = {
        let a = ConnectorEnd::Anchored {
            node: note,
            side: Side::Right,
            t: 0.5,
        };
        let b = ConnectorEnd::Anchored {
            node: generator,
            side: Side::Left,
            t: prompt_t,
        };
        let binding = infer_binding(&doc.scene, &a, &b);
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
        })
    };
    add(&mut doc, wire, 0.0);

    doc.scene.migrate_agent_cards();
    let NodeKind::Image(picture) = &doc.scene.node(generator).unwrap().kind else {
        panic!("a generator opens as a picture");
    };
    assert!(picture.item.is_none(), "nothing picked yet");
    assert_eq!(picture.agent.as_ref().unwrap().instruction, "watercolor");
    let NodeKind::Image(written) = &doc.scene.node(block).unwrap().kind else {
        panic!("a text block opens as a text document window");
    };
    assert!(written.item.is_none() && written.agent.is_some());
    assert_eq!(input_ports(&doc.scene, generator).len(), 3);
    assert_eq!(input_ports(&doc.scene, block).len(), 2);
    let snap = snapshot(
        &doc,
        generator,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        snap.wired[0].slot,
        Some(InputSlot::Prompt),
        "the wire still binds"
    );

    // Downstream, the newest result stands for the picture, and the
    // reply for the note, until a person picks or edits.
    let reader = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("r", "codex")),
        1200.0,
    );
    for source in [generator, block] {
        let wire = NodeKind::Connector(ConnectorNode {
            crosstalk: None,
            routing: None,
            color: None,
            a: ConnectorEnd::Anchored {
                node: source,
                side: Side::Right,
                t: 0.5,
            },
            b: ConnectorEnd::Anchored {
                node: reader,
                side: Side::Left,
                t: 0.5,
            },
            binding: infer_binding(
                &doc.scene,
                &ConnectorEnd::Anchored {
                    node: source,
                    side: Side::Right,
                    t: 0.5,
                },
                &ConnectorEnd::Anchored {
                    node: reader,
                    side: Side::Left,
                    t: 0.5,
                },
            ),
            stroke: Stroke::none(),
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: Default::default(),
        });
        add(&mut doc, wire, 0.0);
    }
    let derived = |node: NodeId, text: &str, image: &str| ContextItem {
        node: node.0,
        text: text.into(),
        images: if image.is_empty() {
            vec![]
        } else {
            vec![image.into()]
        },
        outputs: Default::default(),
        active: None,
        depth: None,
        slot: None,
    };
    let outputs = BTreeMap::from([
        (generator, derived(generator, "", "run-2.png")),
        (block, derived(block, "A calm harbour.", "")),
    ]);
    let snap = snapshot(&doc, reader, AgentContextScope::Selection, &[], &outputs).unwrap();
    assert_eq!(snap.wired.len(), 2);
    assert!(snap.wired.iter().any(|w| w.images == ["run-2.png"]));
    assert!(snap.wired.iter().any(|w| w.text == "A calm harbour."));

    let picked = doc.add_item("C:/out/run-1.png".into(), "run-1.png", 1, 1, "k");
    let (g, _b) = (
        doc.scene.index_of(generator).unwrap(),
        doc.scene.index_of(block).unwrap(),
    );
    if let NodeKind::Image(i) = &mut doc.scene.nodes[g].kind {
        i.item = picked;
    }
    let snap = snapshot(&doc, reader, AgentContextScope::Selection, &[], &outputs).unwrap();
    assert!(snap
        .wired
        .iter()
        .any(|w| w.images.iter().any(|i| i.ends_with("run-1.png"))));
    assert!(snap.wired.iter().any(|w| w.text == "A calm harbour."));

    let images = [
        ImageOutput {
            id: "a".into(),
            source: "run-1.png".into(),
            ..Default::default()
        },
        ImageOutput {
            id: "b".into(),
            source: "run-2.png".into(),
            ..Default::default()
        },
    ];
    assert_eq!(newest_image(&images).unwrap().id, "b");
}
