#[test]
fn text_edit_changes_next_snapshot_but_not_submitted_input() {
    let mut doc = SlateDoc::new("context");
    let note = add(&mut doc, text("soft light"), 0.0);
    let portal = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("agent", "local")),
        250.0,
    );
    let wire = connect(&mut doc, note, portal);
    let before = snapshot(
        &doc,
        portal,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(before.wired[0].text, "soft light");
    if let NodeKind::Text(t) = &mut doc.scene.node_mut(note).unwrap().kind {
        t.text = "hard light".into();
    }
    let after = snapshot(
        &doc,
        portal,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_ne!(before.revision, after.revision);
    assert_eq!(before.wired[0].text, "soft light");
    if let NodeKind::Connector(c) = &mut doc.scene.node_mut(wire).unwrap().kind {
        c.a = ConnectorEnd::Free { point: [0.0, 0.0] };
    }
    assert!(snapshot(
        &doc,
        portal,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new()
    )
    .unwrap()
    .wired
    .is_empty());
}
#[test]
fn decorative_connectors_are_inert_and_inputs_keep_wire_order() {
    let mut doc = SlateDoc::new("order");
    let first = add(&mut doc, text("first"), 400.0);
    let second = add(&mut doc, text("second"), 0.0);
    let portal = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("agent", "local")),
        250.0,
    );
    connect(&mut doc, first, portal);
    let decorative = connect(&mut doc, second, portal);
    let inputs = snapshot(
        &doc,
        portal,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(
        inputs
            .wired
            .iter()
            .map(|i| i.text.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    if let NodeKind::Connector(c) = &mut doc.scene.node_mut(decorative).unwrap().kind {
        c.binding = None;
    }
    assert_eq!(
        snapshot(
            &doc,
            portal,
            AgentContextScope::Selection,
            &[],
            &BTreeMap::new()
        )
        .unwrap()
        .wired
        .len(),
        1
    );
}
#[test]
fn unbundle_keeps_inputs_and_identity_and_undo_redo_reuses_sessions() {
    let mut doc = SlateDoc::new("bundle");
    let note = add(&mut doc, text("garden"), 0.0);
    let mut p = PortalNode::unbound_agent("images", "image-link");
    p.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let portal = add(&mut doc, NodeKind::Portal(p), 250.0);
    connect(&mut doc, note, portal);
    let original = doc.scene.clone();
    let images: Vec<_> = (0..3)
        .map(|i| ImageOutput {
            id: format!("image-{i}"),
            source: format!("{i}.png"),
            request: "run-1".into(),
            prompt: "garden".into(),
            model: String::new(),
            task: String::new(),
            live: String::new(),
        })
        .collect();
    let (commands, ids) = unbundle(&doc.scene, portal, &images, 1).unwrap();
    assert_eq!(ids[1], portal);
    let mut journal = SceneJournal::default();
    assert!(journal.commit(&mut doc.scene, commands));
    let split = doc.scene.clone();
    let mut sessions = BTreeSet::new();
    for id in ids {
        let NodeKind::Portal(p) = &doc.scene.node(id).unwrap().kind else {
            panic!()
        };
        let agent = p.agent.as_ref().unwrap();
        assert_eq!(agent.provider, "image-link");
        assert!(agent.seed.is_some());
        assert!(sessions.insert(agent.session.clone()));
        assert_eq!(
            snapshot(
                &doc,
                id,
                AgentContextScope::Selection,
                &[],
                &BTreeMap::new()
            )
            .unwrap()
            .wired[0]
                .text,
            "garden"
        );
    }
    assert!(journal.undo(&mut doc.scene));
    assert_eq!(doc.scene.nodes, original.nodes);
    assert!(journal.redo(&mut doc.scene));
    assert_eq!(doc.scene.nodes, split.nodes);
}
#[test]
fn active_all_and_pinned_outputs_survive_dedup_and_unbundle() {
    let mut doc = SlateDoc::new("outputs");
    let mut p = PortalNode::unbound_agent("images", "image-link");
    p.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let source = add(&mut doc, NodeKind::Portal(p), 0.0);
    let target = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("target", "codex")),
        500.0,
    );
    let active_wire = connect(&mut doc, source, target);
    let all_wire = connect(&mut doc, source, target);
    let pinned_wire = connect(&mut doc, source, target);
    if let NodeKind::Connector(c) = &mut doc.scene.node_mut(all_wire).unwrap().kind {
        c.binding.as_mut().unwrap().all_images = true;
    }
    if let NodeKind::Connector(c) = &mut doc.scene.node_mut(pinned_wire).unwrap().kind {
        c.binding.as_mut().unwrap().output = Some("image-0".into());
    }
    let images: Vec<_> = (0..3)
        .map(|i| ImageOutput {
            id: format!("image-{i}"),
            source: format!("{i}.png"),
            request: "run".into(),
            prompt: "test".into(),
            model: String::new(),
            task: String::new(),
            live: String::new(),
        })
        .collect();
    let outputs = BTreeMap::from([(
        source,
        ContextItem {
            node: source.0,
            text: String::new(),
            images: images.iter().map(|i| i.source.clone()).collect(),
            outputs: images
                .iter()
                .map(|i| (i.id.clone(), i.source.clone()))
                .collect(),
            active: Some("image-1".into()),
            depth: None,
            slot: None,
        },
    )]);
    let before = snapshot(&doc, target, AgentContextScope::Selection, &[], &outputs).unwrap();
    assert_eq!(
        before
            .wired
            .iter()
            .map(|v| v.images.clone())
            .collect::<Vec<_>>(),
        vec![
            vec!["1.png"],
            vec!["0.png", "1.png", "2.png"],
            vec!["0.png"]
        ]
    );
    let (cmds, ids) = unbundle(&doc.scene, source, &images, 1).unwrap();
    assert!(doc.scene.apply_all(&cmds));
    let NodeKind::Connector(c) = &doc.scene.node(active_wire).unwrap().kind else {
        panic!()
    };
    assert_eq!(endpoint_node(&c.a), Some(ids[1]));
    assert_eq!(c.binding.as_ref().unwrap().output, None);
    assert!(!c.binding.as_ref().unwrap().all_images);
    let NodeKind::Connector(c) = &doc.scene.node(pinned_wire).unwrap().kind else {
        panic!()
    };
    assert_eq!(endpoint_node(&c.a), Some(ids[0]));
    assert_eq!(
        c.binding.as_ref().unwrap().output.as_deref(),
        Some("image-0")
    );
    let mut expanded = doc
        .scene
        .nodes
        .iter()
        .filter_map(|n| {
            let NodeKind::Connector(c) = &n.kind else {
                return None;
            };
            let b = c.binding.as_ref()?;
            (b.order.first() == Some(&all_wire.0)).then_some((
                b.order.clone(),
                endpoint_node(&c.a),
                b,
            ))
        })
        .collect::<Vec<_>>();
    expanded.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(expanded.len(), 3);
    for (i, (_, node, b)) in expanded.iter().enumerate() {
        assert_eq!(*node, Some(ids[i]));
        assert_eq!(b.output.as_deref(), Some(images[i].id.as_str()));
        assert!(!b.all_images);
    }
}

#[test]
fn reattach_preserves_reverse_direction_and_source_output_policy() {
    let mut doc = SlateDoc::new("rewire");
    let a = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("A", "codex")),
        500.0,
    );
    let b = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("B", "codex")),
        600.0,
    );
    let mut p = PortalNode::unbound_agent("images", "image-link");
    p.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let image1 = add(&mut doc, NodeKind::Portal(p.clone()), 0.0);
    let image2 = add(&mut doc, NodeKind::Portal(p), 100.0);
    let wire = connect(&mut doc, image1, a);
    let NodeKind::Connector(mut before) = doc.scene.node(wire).unwrap().kind.clone() else {
        panic!()
    };
    std::mem::swap(&mut before.a, &mut before.b);
    let binding = before.binding.as_mut().unwrap();
    binding.input_b = false;
    binding.output = Some("chosen".into());
    binding.order = vec![wire.0];
    let mut after = before.clone();
    after.b = ConnectorEnd::Anchored {
        node: image2,
        side: Side::Right,
        t: 0.5,
    };
    let rebound = rebind(&doc.scene, &before, &after).unwrap();
    assert!(!rebound.input_b);
    assert_eq!(rebound.kind, InputKind::Images);
    assert_eq!(rebound.output, None);
    after = before.clone();
    after.a = ConnectorEnd::Anchored {
        node: b,
        side: Side::Left,
        t: 0.5,
    };
    let rebound = rebind(&doc.scene, &before, &after).unwrap();
    assert!(!rebound.input_b);
    assert_eq!(rebound.output.as_deref(), Some("chosen"));
    assert_eq!(rebound.order, vec![wire.0]);
    before.binding.as_mut().unwrap().output = None;
    before.binding.as_mut().unwrap().all_images = true;
    assert!(rebind(&doc.scene, &before, &after).unwrap().all_images);
}

#[test]
fn a_spreadsheet_wire_into_a_web_portal_is_a_table_link() {
    use crate::ids::ItemId;
    use std::path::Path;
    let mut doc = SlateDoc::new("link");
    let sheet = add(&mut doc, NodeKind::Image(ImageNode::new(ItemId(7))), 0.0);
    let page = add(
        &mut doc,
        NodeKind::Portal(PortalNode::bound_web("Chart", "dash.html")),
        400.0,
    );
    let a = ConnectorEnd::Anchored {
        node: sheet,
        side: Side::Right,
        t: 0.5,
    };
    let b = ConnectorEnd::Anchored {
        node: page,
        side: Side::Left,
        t: 0.5,
    };
    let binding = infer_binding_with(&doc.scene, &a, &b, &|id| {
        (id == ItemId(7)).then_some(Path::new("budget.xlsx"))
    })
    .unwrap();
    assert_eq!(binding.kind, InputKind::Table);
    assert!(binding.input_b);
    assert!(infer_binding(&doc.scene, &a, &b).is_none());
}

#[test]
fn a_wired_sketch_is_text_context() {
    let mut doc = SlateDoc::new("sketch");
    let sketch = add(
        &mut doc,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: None,
            stroke: Stroke::default(),
            corner: Default::default(),
            sides: crate::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: Some(ShapeText {
                body: "north wing".into(),
                style: Default::default(),
                family: Default::default(),
                size: 18.0,
                color: Rgba::BLACK,
                align: Default::default(),
            }),
        }),
        0.0,
    );
    let chat = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("chat", "codex")),
        400.0,
    );
    let binding = infer_binding(
        &doc.scene,
        &ConnectorEnd::Anchored {
            node: sketch,
            side: Side::Right,
            t: 0.5,
        },
        &ConnectorEnd::Anchored {
            node: chat,
            side: Side::Left,
            t: 0.5,
        },
    )
    .unwrap();
    assert_eq!(binding.kind, InputKind::Text);
    connect(&mut doc, sketch, chat);
    let snap = snapshot(
        &doc,
        chat,
        AgentContextScope::Selection,
        &[],
        &Default::default(),
    )
    .unwrap();
    assert!(snap.wired[0].text.contains("north wing"));
    assert!(snap.wired[0].text.contains("rectangle"));

    // The same sketch wired to an image generator supplies its words only.
    let mut portal = PortalNode::unbound_agent("Images", "comfy");
    portal.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let generator = add(&mut doc, NodeKind::Portal(portal), 800.0);
    connect(&mut doc, sketch, generator);
    let snap = snapshot(
        &doc,
        generator,
        AgentContextScope::Selection,
        &[],
        &Default::default(),
    )
    .unwrap();
    assert_eq!(snap.wired[0].text, "north wing");
}
