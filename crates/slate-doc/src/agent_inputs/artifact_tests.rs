use super::*;

fn artifact_node(doc: &mut SlateDoc, x: f32, revision: &str) -> NodeId {
    let mut portal = PortalNode::bound_web("dashboard", format!("{revision}.html"));
    portal.web.as_mut().unwrap().artifact = Some(Box::new(ArtifactGeneratorRef {
        revision: Some(revision.into()),
        revisions: vec![revision.into()],
        checkpoint: 4,
        ..Default::default()
    }));
    portal.agent = PortalNode::unbound_agent("dashboard", "cursor").agent;
    let node = doc.scene.build_node(
        WorldRect::new(x, 0.0, 320.0, 200.0),
        NodeKind::Portal(portal),
    );
    let id = node.id;
    doc.scene.nodes.push(node);
    id
}

fn wire(doc: &mut SlateDoc, source: NodeId, target: NodeId, memory: bool) -> NodeId {
    let a = ConnectorEnd::Anchored {
        node: source,
        side: Side::Right,
        t: 0.5,
    };
    let b = ConnectorEnd::Anchored {
        node: target,
        side: Side::Left,
        t: 0.5,
    };
    let mut binding = infer_binding(&doc.scene, &a, &b).unwrap();
    if memory {
        binding.kind = InputKind::Memory;
        binding.slot = Some("memory".into());
        binding.output = Some(
            serde_json::to_string(&ArtifactMemoryPin {
                revision: artifact(doc.scene.node(source).unwrap())
                    .unwrap()
                    .revision
                    .clone()
                    .unwrap(),
                through: 4,
            })
            .unwrap(),
        );
    }
    let node = doc.scene.build_node(
        WorldRect::default(),
        NodeKind::Connector(ConnectorNode {
            a,
            b,
            binding: Some(binding),
            stroke: Stroke::none(),
            color: None,
            routing: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: Default::default(),
            crosstalk: None,
        }),
    );
    let id = node.id;
    doc.scene.nodes.push(node);
    id
}

#[test]
fn deleting_and_undoing_memory_preserves_content_and_completed_revision() {
    let mut doc = SlateDoc::new("artifact ancestry");
    let source = artifact_node(&mut doc, 0.0, "revision-one");
    let target = artifact_node(&mut doc, 500.0, "revision-two");
    wire(&mut doc, source, target, false);
    let trace = wire(&mut doc, source, target, true);
    let original = doc.scene.node(source).unwrap().clone();
    assert_eq!(
        artifact_content_parent(&doc.scene, target),
        Some((source, "revision-one".into()))
    );
    assert_eq!(
        artifact_memory_parent(&doc.scene, target),
        Some((source, 4))
    );
    assert!(!is_chat_card(&doc.scene, target));
    assert!(crate::agent_chat::is_agent_node(
        doc.scene.node(target).unwrap()
    ));
    let index = doc
        .scene
        .nodes
        .iter()
        .position(|node| node.id == trace)
        .unwrap();
    let mut journal = SceneJournal::default();
    let removed = doc.scene.nodes[index].clone();
    assert!(journal.commit(
        &mut doc.scene,
        vec![SceneCmd::Remove {
            index,
            node: removed
        }]
    ));
    assert!(artifact_memory_parent(&doc.scene, target).is_none());
    assert_eq!(
        artifact_content_parent(&doc.scene, target),
        Some((source, "revision-one".into()))
    );
    assert_eq!(doc.scene.node(source), Some(&original));
    assert!(journal.undo(&mut doc.scene));
    assert_eq!(
        artifact_memory_parent(&doc.scene, target),
        Some((source, 4))
    );
}

#[test]
fn downstream_content_and_memory_keep_the_authored_revision_when_source_changes() {
    let mut doc = SlateDoc::new("pinned artifact");
    let source = artifact_node(&mut doc, 0.0, "old");
    let target = artifact_node(&mut doc, 500.0, "child");
    wire(&mut doc, source, target, false);
    wire(&mut doc, source, target, true);
    let source = artifact_mut(doc.scene.node_mut(source).unwrap()).unwrap();
    source.revision = Some("later".into());
    source.checkpoint = 12;
    source.revisions.push("later".into());
    let snapshot = snapshot(
        &doc,
        target,
        AgentContextScope::Selection,
        &[],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(snapshot.wired.len(), 1, "memory is never model content");
    assert!(snapshot.wired[0].text.ends_with("revision old"));
    let (_, pin) = artifact_memory_source(&doc.scene, target).unwrap();
    assert_eq!(
        pin,
        ArtifactMemoryPin {
            revision: "old".into(),
            through: 4
        }
    );
}

#[test]
fn memory_keeps_chat_rail_geometry_while_content_uses_the_shared_top_datum() {
    let mut doc = SlateDoc::new("memory geometry");
    let source = artifact_node(&mut doc, 0.0, "a");
    let target = artifact_node(&mut doc, 500.0, "b");
    let trace = wire(&mut doc, source, target, true);
    let NodeKind::Connector(conn) = &doc.scene.node(trace).unwrap().kind else {
        panic!()
    };
    let expected = crate::agent_chat::rail(
        doc.scene.node(source).unwrap().rect,
        doc.scene.node(target).unwrap().rect,
    );
    let route = crate::connector_route_in_scene(
        &doc.scene,
        Some(trace),
        &conn.a,
        &conn.b,
        crate::WireRouting::Orthogonal,
    )
    .unwrap();
    assert_eq!(route, crate::wire::ConnectorPath::Bezier(expected));
    let host = crate::WireHost::from_node(doc.scene.node(target).unwrap());
    let content = host.anchor(Side::Left, 0.5);
    assert_eq!(content[0], 500.0);
    assert!(
        (content[1] - doc.scene.node(target).unwrap().rect.y - crate::wire_host::FLOW_PORT_TOP)
            .abs()
            < 0.0001
    );
    assert_eq!(input_ports(&doc.scene, target)[0].label, "Content");
}

#[test]
fn ordinary_web_hosts_deserialize_without_artifact_generation() {
    let portal = PortalNode::bound_web("existing page", "index.html");
    let serialized = serde_json::to_value(&portal).unwrap();
    assert!(serialized["web"].get("artifact").is_none());
    let read: PortalNode = serde_json::from_value(serialized).unwrap();
    assert!(read.web.unwrap().artifact.is_none());
}

#[test]
fn web_artifact_style_access_matches_the_web_host_frame_fields() {
    let mut doc = SlateDoc::default();
    let id = artifact_node(&mut doc, 0.0, "styled");
    let node = doc.scene.node_mut(id).unwrap();
    let hidden_chat_stroke = Stroke {
        width: 7.0,
        ..Stroke::default()
    };
    crate::agent_chat::agent_mut(node).unwrap().chat.stroke = Some(hidden_chat_stroke);
    let frame_stroke = Stroke {
        width: 3.0,
        ..Stroke::default()
    };
    let fill = Rgba([30, 80, 110, 255]);
    set_stroke(node, frame_stroke);
    set_fill(node, Some(fill));
    set_corner(node, Corner::Rounded { radius: 12.0 });
    let NodeKind::Portal(portal) = &node.kind else {
        panic!()
    };
    assert_eq!(stroke_of(node), Some(portal.stroke));
    assert_eq!(portal.stroke, frame_stroke);
    assert_eq!(fill_of(node), Some(portal.fill));
    assert_eq!(portal.fill, fill);
    assert_eq!(corner_of(node), Some(portal.corner));
    assert_eq!(
        portal.agent.as_ref().unwrap().chat.stroke,
        Some(hidden_chat_stroke)
    );
    assert!(!portal.agent.as_ref().unwrap().chat.custom_fill);
}

#[test]
fn copied_artifacts_keep_immutable_sources_and_receive_fresh_agent_authority() {
    let mut doc = SlateDoc::default();
    let id = artifact_node(&mut doc, 0.0, "kept");
    let node = doc.scene.node_mut(id).unwrap();
    artifact_mut(node).unwrap().workspace = Some(SourceUri {
        locator: "mutable-work".into(),
    });
    crate::agent_chat::agent_mut(node).unwrap().channel = Some("old-provider-handle".into());
    let original = node.clone();
    let mut copied = original.clone();
    crate::agent_chat::remap_view(&mut copied, |_| None);
    let old = crate::agent_chat::agent(&original).unwrap();
    let new = crate::agent_chat::agent(&copied).unwrap();
    assert_ne!(old.session, new.session);
    assert!(new.channel.is_none() && new.bundle.is_none());
    assert!(artifact(&copied).unwrap().workspace.is_none());
    assert_eq!(
        artifact(&copied).unwrap().revision,
        artifact(&original).unwrap().revision
    );
    assert!(crate::agent_chat::conversation(&doc.scene, id).is_empty());
    assert!(!crate::agent_chat::paste_is_train_fork(&[copied]));
}

#[test]
fn artifacts_keep_web_capture_semantics_when_wired_into_image_generators() {
    let mut doc = SlateDoc::default();
    let source = artifact_node(&mut doc, 0.0, "dashboard");
    let mut image = PortalNode::unbound_agent("image", "comfy");
    image.agent.as_mut().unwrap().view = atlas_agent::PortalView::Images;
    let image = doc.scene.build_node(
        WorldRect::new(500.0, 0.0, 320.0, 200.0),
        NodeKind::Portal(image),
    );
    let target = image.id;
    doc.scene.nodes.push(image);
    let binding = infer_binding(
        &doc.scene,
        &ConnectorEnd::Anchored {
            node: source,
            side: Side::Right,
            t: 0.5,
        },
        &ConnectorEnd::Anchored {
            node: target,
            side: Side::Left,
            t: 0.25,
        },
    )
    .unwrap();
    assert_eq!(binding.kind, InputKind::Images);
    assert_eq!(binding.slot.as_deref(), Some("media"));
    assert!(
        binding.output.is_none(),
        "an artifact revision is never an image-output id"
    );
}
