use super::*;

#[test]
fn lazy_value_source_observes_only_connected_sources_and_matches_map_api() {
    let mut doc = SlateDoc::new("lazy-inputs");
    let source = add(&mut doc, text("Read this"), 0.0);
    let target = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("Target", "codex")),
        400.0,
    );
    let unused = add(&mut doc, text("Do not read this"), 900.0);
    connect(&mut doc, source, target);
    let mut projected = Vec::new();
    let lazy = snapshot_with_value_source(
        &doc,
        target,
        AgentContextScope::Selection,
        &[],
        |id| {
            projected.push(id);
            assert_ne!(id, unused);
            None
        },
        |_| Ok(()),
    )
    .unwrap();
    let eager = snapshot_with_source_check(
        &doc,
        target,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(lazy).unwrap(),
        serde_json::to_value(eager).unwrap()
    );
    assert_eq!(projected, vec![source]);
}

#[test]
fn pending_active_source_fails_before_projection() {
    let mut doc = SlateDoc::new("lazy-readiness");
    let source = add(&mut doc, text("Pending source"), 0.0);
    let target = add(
        &mut doc,
        NodeKind::Portal(PortalNode::unbound_agent("Target", "codex")),
        400.0,
    );
    connect(&mut doc, source, target);
    let error = snapshot_with_value_source(
        &doc,
        target,
        AgentContextScope::Selection,
        &[],
        |_| panic!("a pending source must not be read"),
        |id| {
            assert_eq!(id, source);
            Err("Source is pending".into())
        },
    )
    .unwrap_err();
    assert_eq!(error, "Source is pending");
}

#[test]
fn indexed_candidates_preserve_wire_order_direction_and_live_values() {
    let mut doc = SlateDoc::new("incoming-candidates");
    let legacy = add(&mut doc, text("Legacy prompt"), 0.0);
    let typed = add(&mut doc, text("Typed prompt"), 120.0);
    let mut portal = PortalNode::unbound_agent("Generator", "image-link");
    portal.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let target = add(&mut doc, NodeKind::Portal(portal), 400.0);
    let legacy_wire = connect(&mut doc, legacy, target);
    let typed_wire = connect(&mut doc, typed, target);
    for wire in [legacy_wire, typed_wire] {
        let NodeKind::Connector(conn) = &mut doc.scene.node_mut(wire).unwrap().kind else {
            unreachable!();
        };
        std::mem::swap(&mut conn.a, &mut conn.b);
        if wire == legacy_wire {
            conn.binding = None;
        } else {
            let binding = conn.binding.as_mut().unwrap();
            binding.input_b = false;
            binding.order = vec![0];
        }
    }

    let mut incoming = IncomingWires::default();
    for node in &doc.scene.nodes {
        incoming.record(node);
    }
    let capture = |doc: &SlateDoc| {
        incoming
            .snapshot_with_value_source(
                doc,
                target,
                AgentContextScope::Selection,
                &[],
                |_| None,
                |_| Ok(()),
            )
            .unwrap()
    };
    let first = capture(&doc);
    assert_eq!(
        first.wired.iter().map(|item| item.node).collect::<Vec<_>>(),
        [typed.0, legacy.0],
    );
    assert_eq!(
        first,
        snapshot(
            &doc,
            target,
            AgentContextScope::Selection,
            &[],
            &BTreeMap::new()
        )
        .unwrap(),
    );

    let NodeKind::Text(note) = &mut doc.scene.node_mut(typed).unwrap().kind else {
        unreachable!();
    };
    note.text = "Updated prompt".into();
    let updated = capture(&doc);
    assert_ne!(first.revision, updated.revision);
    assert_eq!(updated.wired[0].text, "Updated prompt");
}
