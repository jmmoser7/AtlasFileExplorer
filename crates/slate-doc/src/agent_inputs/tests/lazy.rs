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
