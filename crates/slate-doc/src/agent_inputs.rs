//! The semantic layer of existing board wires. Geometry remains in `wire_host`.
//! Resolves one immutable run input; adapters never interpret a scene.
use crate::{scene::*, SlateDoc};
use atlas_agent::{ContextItem, ImageOutput, InputSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Text,
    Images,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireBinding {
    /// Direction is relative to the connector, never another pair of node ids.
    pub input_b: bool,
    pub kind: InputKind,
    /// None follows the source's latest completed output; Some pins a revision.
    #[serde(default)]
    pub output: Option<String>,
    /// Stable input ordering across fan-out; never an endpoint identity.
    #[serde(default)]
    pub order: Vec<u64>,
    #[serde(default)]
    pub all_images: bool,
}

pub fn endpoint_node(end: &ConnectorEnd) -> Option<NodeId> {
    match end {
        ConnectorEnd::Anchored { node, .. } => Some(*node),
        _ => None,
    }
}

pub fn infer_binding(scene: &Scene, a: &ConnectorEnd, b: &ConnectorEnd) -> Option<WireBinding> {
    directional_binding(scene, a, b, true).or_else(|| directional_binding(scene, b, a, false))
}

fn directional_binding(
    scene: &Scene,
    source: &ConnectorEnd,
    target: &ConnectorEnd,
    input_b: bool,
) -> Option<WireBinding> {
    let source = scene.node(endpoint_node(source)?)?;
    let target = scene.node(endpoint_node(target)?)?;
    if !matches!(&target.kind,NodeKind::Portal(p) if p.kind==PortalKind::Agent) {
        return None;
    }
    let kind = match &source.kind {
        NodeKind::Text(_) => InputKind::Text,
        NodeKind::Image(_) => InputKind::Images,
        NodeKind::Portal(p) if p.kind == PortalKind::Agent => {
            if p.agent.as_ref()?.view == atlas_agent::PortalView::Images {
                InputKind::Images
            } else {
                InputKind::Text
            }
        }
        _ => return None,
    };
    Some(WireBinding {
        input_b,
        kind,
        output: None,
        order: vec![],
        all_images: false,
    })
}

/// Existing direction takes precedence when an endpoint changes. Pins describe
/// the source, so moving only the receiving endpoint preserves them.
pub fn rebind(scene: &Scene, before: &ConnectorNode, after: &ConnectorNode) -> Option<WireBinding> {
    if endpoint_node(&after.a).is_none() || endpoint_node(&after.b).is_none() {
        return before.binding.clone();
    }
    if let Some(old) = &before.binding {
        let (old_source, source, target) = if old.input_b {
            (&before.a, &after.a, &after.b)
        } else {
            (&before.b, &after.b, &after.a)
        };
        if let Some(mut binding) = directional_binding(scene, source, target, old.input_b) {
            binding.order = old.order.clone();
            if binding.kind == old.kind && endpoint_node(old_source) == endpoint_node(source) {
                binding.output = old.output.clone();
                binding.all_images = old.all_images;
            }
            return Some(binding);
        }
    }
    let mut binding = infer_binding(scene, &after.a, &after.b)?;
    if let Some(old) = &before.binding {
        binding.order = old.order.clone();
    }
    Some(binding)
}

/// Authored context uses the scene's existing frame ownership rule.
pub fn context_nodes(
    scene: &Scene,
    portal: NodeId,
    scope: AgentContextScope,
    selection: &[NodeId],
) -> Vec<NodeId> {
    let mut ids = match scope {
        AgentContextScope::Selection => selection.to_vec(),
        AgentContextScope::Frame => scene
            .frame_of(portal)
            .map(|id| scene.members_of(id))
            .unwrap_or_default(),
        AgentContextScope::Board => scene.nodes.iter().map(|n| n.id).collect(),
    };
    ids.retain(|id| *id != portal);
    ids.sort();
    ids.dedup();
    ids
}

/// Completed upstream values only. A wire never runs another program implicitly.
pub fn snapshot(
    doc: &SlateDoc,
    portal: NodeId,
    scope: AgentContextScope,
    selection: &[NodeId],
    outputs: &BTreeMap<NodeId, ContextItem>,
) -> Result<InputSnapshot, String> {
    let item = |id| -> Option<ContextItem> {
        let node = doc.scene.node(id)?;
        Some(match &node.kind {
            NodeKind::Text(t) => ContextItem {
                node: id.0,
                text: t.text.clone(),
                images: vec![],
                outputs: Default::default(),
                active: None,
            },
            NodeKind::Image(i) => ContextItem {
                node: id.0,
                text: String::new(),
                images: vec![doc.item(i.item)?.path.to_string_lossy().into_owned()],
                outputs: Default::default(),
                active: None,
            },
            _ => outputs.get(&id)?.clone(),
        })
    };
    let mut context: Vec<ContextItem> = context_nodes(&doc.scene, portal, scope, selection)
        .into_iter()
        .filter_map(item)
        .collect();
    let mut wired = Vec::new();
    let mut seen = BTreeSet::new();
    // Stable wire identity/order, independent of node movement and album splitting.
    let mut wires: Vec<_> = doc
        .scene
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Connector(c) => c.binding.as_ref().map(|b| {
                (
                    if b.order.is_empty() {
                        vec![n.id.0]
                    } else {
                        b.order.clone()
                    },
                    n,
                )
            }),
            _ => None,
        })
        .collect();
    wires.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
    for (_, node) in wires {
        let NodeKind::Connector(c) = &node.kind else {
            continue;
        };
        let Some(binding) = &c.binding else { continue };
        let (source, target) = if binding.input_b {
            (&c.a, &c.b)
        } else {
            (&c.b, &c.a)
        };
        if endpoint_node(target) != Some(portal) {
            continue;
        }
        let Some(source) = endpoint_node(source) else {
            continue;
        }; // detached/pasted external end is inert
        if source == portal {
            return Err("A portal cannot use its own output as input.".into());
        }
        if !seen.insert((
            source,
            format!("{:?}", binding.kind),
            binding.output.clone(),
            binding.all_images,
        )) {
            continue;
        }
        let mut value = item(source).ok_or("A connected source has no completed output yet.")?;
        match binding.kind {
            InputKind::Text => {
                value.images.clear();
                if value.text.is_empty() {
                    return Err("Connected text is empty.".into());
                }
            }
            InputKind::Images => {
                value.text.clear();
                if value.images.is_empty() {
                    return Err("Connected images are not ready.".into());
                }
            }
        }
        if let Some(output) = &binding.output {
            value.images = vec![value
                .outputs
                .get(output)
                .ok_or("The pinned image output is unavailable.")?
                .clone()];
        } else if binding.kind == InputKind::Images && !binding.all_images {
            value.images = if let Some(active) = &value.active {
                vec![value
                    .outputs
                    .get(active)
                    .ok_or("The active image is unavailable.")?
                    .clone()]
            } else {
                value.images.into_iter().take(1).collect()
            };
        }
        wired.push(value);
    }
    if scope == AgentContextScope::Selection && !wired.is_empty() {
        context.clear();
    }
    let mut result = InputSnapshot {
        revision: String::new(),
        context,
        wired,
    };
    let bytes = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
    let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    });
    result.revision = format!("{hash:016x}");
    Ok(result)
}

/// All children keep settings and incoming wires, but own new sessions. The active
/// image retains the original NodeId, so its outgoing wires retain their identity.
pub fn unbundle(
    scene: &Scene,
    id: NodeId,
    images: &[ImageOutput],
    active: usize,
) -> Result<(Vec<SceneCmd>, Vec<NodeId>), String> {
    if images.len() < 2 || active >= images.len() {
        return Err("This portal has no bundle to split.".into());
    }
    let original = scene.node(id).ok_or("Portal no longer exists.")?;
    let NodeKind::Portal(p) = &original.kind else {
        return Err("Select an image portal.".into());
    };
    if p.agent.is_none() {
        return Err("Select an image portal.".into());
    }
    let mut allocator = scene.clone();
    let mut commands = Vec::new();
    let mut ids = Vec::new();
    let mut insertion = scene.nodes.len();
    for (i, image) in images.iter().enumerate() {
        let mut child = original.clone();
        if i != active {
            child.id = allocator.build_node(child.rect, child.kind.clone()).id;
            child.rect.x += (ids.len() as f32 + 1.0) * (child.rect.w + 24.0);
        }
        let NodeKind::Portal(ref mut portal) = child.kind else {
            unreachable!()
        };
        let agent = portal.agent.as_mut().unwrap();
        agent.session = new_agent_session_id();
        agent.channel = None;
        agent.bundle = None;
        agent.seed = Some(image.clone());
        ids.push(child.id);
        if i == active {
            commands.push(SceneCmd::Patch {
                before: Box::new(original.clone()),
                after: Box::new(child),
            });
        } else {
            let child_id = child.id;
            commands.push(SceneCmd::Add {
                index: insertion,
                node: child,
            });
            insertion += 1;
            for wire in &scene.nodes {
                let NodeKind::Connector(c) = &wire.kind else {
                    continue;
                };
                let Some(binding) = &c.binding else { continue };
                let end = if binding.input_b { &c.b } else { &c.a };
                if endpoint_node(end) != Some(id) {
                    continue;
                }
                let mut copy = wire.clone();
                copy.id = allocator.build_node(copy.rect, copy.kind.clone()).id;
                let NodeKind::Connector(ref mut c) = copy.kind else {
                    unreachable!()
                };
                if let Some(b) = &mut c.binding {
                    if b.order.is_empty() {
                        b.order = vec![wire.id.0];
                    }
                }
                let end = if binding.input_b { &mut c.b } else { &mut c.a };
                if let ConnectorEnd::Anchored { node, .. } = end {
                    *node = child_id;
                }
                commands.push(SceneCmd::Add {
                    index: insertion,
                    node: copy,
                });
                insertion += 1;
            }
        }
    }
    // Outgoing pins follow their matching child. An image-set wire fans out into
    // ordered image wires, preserving the destination's submission order.
    for wire in &scene.nodes {
        let NodeKind::Connector(c) = &wire.kind else {
            continue;
        };
        let Some(binding) = &c.binding else { continue };
        let source = if binding.input_b { &c.a } else { &c.b };
        if endpoint_node(source) != Some(id) {
            continue;
        }
        if binding.kind != InputKind::Images {
            return Err("Disconnect the outgoing text wire before unbundling images.".into());
        }
        let destinations: Vec<usize> = if let Some(pin) = &binding.output {
            vec![images
                .iter()
                .position(|image| &image.id == pin)
                .ok_or("A pinned output is missing from this bundle.")?]
        } else if binding.all_images {
            (0..images.len()).collect()
        } else {
            vec![active]
        };
        for (ordinal, image_index) in destinations.into_iter().enumerate() {
            let mut copy = wire.clone();
            if ordinal > 0 {
                copy.id = allocator.build_node(copy.rect, copy.kind.clone()).id;
            }
            let NodeKind::Connector(ref mut c) = copy.kind else {
                unreachable!()
            };
            let source = if binding.input_b { &mut c.a } else { &mut c.b };
            if let ConnectorEnd::Anchored { node, .. } = source {
                *node = ids[image_index];
            }
            let b = c.binding.as_mut().unwrap();
            b.output = if binding.output.is_some() || binding.all_images {
                Some(images[image_index].id.clone())
            } else {
                None
            };
            b.all_images = false;
            if b.order.is_empty() {
                b.order.push(wire.id.0);
            }
            b.order.push(ordinal as u64);
            if ordinal == 0 {
                commands.push(SceneCmd::Patch {
                    before: Box::new(wire.clone()),
                    after: Box::new(copy),
                });
            } else {
                commands.push(SceneCmd::Add {
                    index: insertion,
                    node: copy,
                });
                insertion += 1;
            }
        }
    }
    // Validate the complete group before it reaches the journal, then update wire
    // bounds against the planned children so newly fanned inputs cull correctly.
    let mut planned = scene.clone();
    if !planned.apply_all(&commands) {
        return Err("Could not prepare the image bundle split.".into());
    }
    for command in &mut commands {
        let node = match command {
            SceneCmd::Add { node, .. } => node,
            SceneCmd::Patch { after, .. } => after.as_mut(),
            _ => continue,
        };
        if let NodeKind::Connector(c) = &node.kind {
            if let Some(rect) = connector_aabb(c, |id| planned.node(id).map(|n| n.rect)) {
                node.rect = rect;
            }
        }
    }
    Ok((commands, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
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
            family: Default::default(),
            size: 16.0,
            color: Rgba::BLACK,
            align: Default::default(),
            fill: None,
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
                a,
                b,
                binding,
                stroke: Stroke::none(),
                arrow_a: false,
                arrow_b: false,
                label: None,
                display: Default::default(),
            }),
            0.0,
        )
    }
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
    fn old_wires_and_portals_deserialize_without_new_fields() {
        let p = PortalNode::unbound_agent("legacy", "cursor");
        let mut value = serde_json::to_value(&p).unwrap();
        for field in ["bundle", "seed", "view"] {
            value["agent"].as_object_mut().unwrap().remove(field);
        }
        let read: PortalNode = serde_json::from_value(value).unwrap();
        assert_eq!(read.agent.unwrap().view, atlas_agent::PortalView::Chat);
    }
}
