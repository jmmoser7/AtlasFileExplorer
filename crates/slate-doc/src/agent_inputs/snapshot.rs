//! Capture completed upstream values while preserving authored output identity.
use super::*;
use std::collections::HashMap;

/// Authored connector candidates for one scene revision. The caller rebuilds
/// this index when the scene changes. Values and legacy inference stay live.
#[derive(Default)]
pub struct IncomingWires {
    by_target: HashMap<NodeId, Vec<NodeId>>,
}

impl IncomingWires {
    pub fn record(&mut self, node: &Node) {
        for target in candidate_targets(node).into_iter().flatten() {
            self.by_target.entry(target).or_default().push(node.id);
        }
    }

    pub fn candidate_ids(&self, target: NodeId) -> &[NodeId] {
        self.by_target.get(&target).map_or(&[], Vec::as_slice)
    }

    pub fn snapshot_with_value_source(
        &self,
        doc: &SlateDoc,
        portal: NodeId,
        scope: AgentContextScope,
        selection: &[NodeId],
        output: impl FnMut(NodeId) -> Option<ContextItem>,
        check_source: impl FnMut(NodeId) -> Result<(), String>,
    ) -> Result<InputSnapshot, String> {
        let candidates = self
            .candidate_ids(portal)
            .iter()
            .filter_map(|id| doc.scene.node(*id));
        snapshot_from_candidates(
            doc,
            portal,
            scope,
            selection,
            candidates,
            output,
            check_source,
        )
    }
}

fn candidate_targets(node: &Node) -> [Option<NodeId>; 2] {
    let NodeKind::Connector(conn) = &node.kind else {
        return [None, None];
    };
    if let Some(binding) = &conn.binding {
        return [endpoint_node(binding.target_end(conn)), None];
    }
    // A legacy unbound connector may supply either of its anchored ends.
    let a = endpoint_node(&conn.a);
    let b = endpoint_node(&conn.b).filter(|b| Some(*b) != a);
    [a, b]
}

fn wire_order(node: &Node) -> &[u64] {
    if let NodeKind::Connector(conn) = &node.kind {
        if let Some(binding) = conn.binding.as_ref().filter(|b| !b.order.is_empty()) {
            return &binding.order;
        }
    }
    std::slice::from_ref(&node.id.0)
}

/// Completed upstream values only. A wire never runs another program implicitly.
pub fn snapshot(
    doc: &SlateDoc,
    portal: NodeId,
    scope: AgentContextScope,
    selection: &[NodeId],
    outputs: &BTreeMap<NodeId, ContextItem>,
) -> Result<InputSnapshot, String> {
    snapshot_with_source_check(doc, portal, scope, selection, outputs, |_| Ok(()))
}

/// Check only active input sources before validating their completed values.
/// The host may reject a pending cached source without interpreting wires again.
pub fn snapshot_with_source_check(
    doc: &SlateDoc,
    portal: NodeId,
    scope: AgentContextScope,
    selection: &[NodeId],
    outputs: &BTreeMap<NodeId, ContextItem>,
    check_source: impl FnMut(NodeId) -> Result<(), String>,
) -> Result<InputSnapshot, String> {
    snapshot_with_value_source(
        doc,
        portal,
        scope,
        selection,
        |id| outputs.get(&id).cloned(),
        check_source,
    )
}

/// Resolve projected values only for authored inputs actually read by this snapshot.
/// The host retains readiness checks; this interpreter still owns wire semantics.
pub fn snapshot_with_value_source(
    doc: &SlateDoc,
    portal: NodeId,
    scope: AgentContextScope,
    selection: &[NodeId],
    output: impl FnMut(NodeId) -> Option<ContextItem>,
    check_source: impl FnMut(NodeId) -> Result<(), String>,
) -> Result<InputSnapshot, String> {
    let candidates = doc
        .scene
        .nodes
        .iter()
        .filter(|node| candidate_targets(node).contains(&Some(portal)));
    snapshot_from_candidates(
        doc,
        portal,
        scope,
        selection,
        candidates,
        output,
        check_source,
    )
}

fn snapshot_from_candidates<'a>(
    doc: &'a SlateDoc,
    portal: NodeId,
    scope: AgentContextScope,
    selection: &[NodeId],
    candidates: impl Iterator<Item = &'a Node>,
    mut output: impl FnMut(NodeId) -> Option<ContextItem>,
    mut check_source: impl FnMut(NodeId) -> Result<(), String>,
) -> Result<InputSnapshot, String> {
    // A flow node's prompt is a shape's words; chat agents also read its geometry.
    let generator = is_flow_node(&doc.scene, portal);
    let mut item = |id| -> Option<ContextItem> {
        let node = doc.scene.node(id)?;
        let projected = output(id);
        Some(match &node.kind {
            // An agent's reply the person has not made their own yet.
            NodeKind::Text(t)
                if t.agent.as_ref().is_some_and(|agent| {
                    t.text.trim().is_empty() || agent.generator.shown_text.is_some()
                }) =>
            {
                projected?
            }
            NodeKind::Text(t) => ContextItem {
                node: id.0,
                text: t.text.clone(),
                images: vec![],
                outputs: Default::default(),
                active: None,
                depth: None,
                slot: None,
            },
            NodeKind::Shape(shape) => ContextItem {
                node: id.0,
                text: if generator {
                    shape
                        .text
                        .as_ref()
                        .map(|t| t.body.trim().to_string())
                        .unwrap_or_default()
                } else {
                    shape_brief(node, shape)
                },
                images: vec![],
                outputs: Default::default(),
                active: None,
                depth: None,
                slot: None,
            },
            // Keep output identities when a picked generator supplies its linked file.
            NodeKind::Image(i) if is_image_generator(&doc.scene, id) && !i.item.is_none() => {
                ContextItem {
                    node: id.0,
                    text: String::new(),
                    images: vec![doc.item(i.item)?.path.to_string_lossy().into_owned()],
                    outputs: projected
                        .as_ref()
                        .map(|value| value.outputs.clone())
                        .unwrap_or_default(),
                    active: projected.as_ref().and_then(|value| value.active.clone()),
                    depth: None,
                    slot: None,
                }
            }
            // A placed text document's words, or an agent picture's newest
            // result, read by the caller.
            NodeKind::Image(_) if projected.is_some() => projected?,
            NodeKind::Image(i) => ContextItem {
                node: id.0,
                text: String::new(),
                images: vec![doc.item(i.item)?.path.to_string_lossy().into_owned()],
                outputs: Default::default(),
                active: None,
                depth: None,
                slot: None,
            },
            NodeKind::Portal(p) if p.kind != PortalKind::Agent => ContextItem {
                node: id.0,
                text: p.source.as_ref()?.locator.clone(),
                images: vec![],
                outputs: Default::default(),
                active: None,
                depth: None,
                slot: None,
            },
            _ => projected?,
        })
    };
    let mut context: Vec<ContextItem> = context_nodes(&doc.scene, portal, scope, selection)
        .into_iter()
        .filter_map(&mut item)
        .collect();
    let mut wired = Vec::new();
    let mut seen = BTreeSet::new();
    // Stable wire identity/order, independent of node movement and album splitting.
    let mut wires: Vec<_> = candidates.collect();
    wires.sort_by(|a, b| {
        wire_order(a)
            .cmp(wire_order(b))
            .then_with(|| a.id.cmp(&b.id))
    });
    for node in wires {
        let NodeKind::Connector(c) = &node.kind else {
            continue;
        };
        let owned = c
            .binding
            .clone()
            .or_else(|| legacy_generator_input(&doc.scene, c, portal));
        let Some(binding) = owned.as_ref() else {
            continue;
        };
        if matches!(binding.kind, InputKind::Memory | InputKind::Control) {
            continue;
        }
        let source = binding.source_end(c);
        let target = binding.target_end(c);
        if endpoint_node(target) != Some(portal) {
            continue;
        }
        let Some(source) = endpoint_node(source) else {
            continue;
        }; // detached/pasted external end is inert
        if source == portal {
            return Err("A portal cannot use its own output as input.".into());
        }
        let slot = binding.slot.as_deref().and_then(InputSlot::from_id);
        if !seen.insert((
            source,
            format!("{:?}{slot:?}", binding.kind),
            binding.output.clone(),
            binding.all_images,
        )) {
            continue;
        }
        check_source(source)?;
        let mut value = item(source).ok_or("A connected source has no completed output yet.")?;
        value.slot = slot;
        match binding.kind {
            InputKind::Memory | InputKind::Control => continue,
            InputKind::Artifact => {
                let source = artifact(
                    doc.scene
                        .node(source)
                        .ok_or("Artifact source is missing.")?,
                )
                .ok_or("The connected source is not an artifact.")?;
                let revision = binding
                    .output
                    .as_deref()
                    .ok_or("The artifact content wire has no pinned revision.")?;
                value.images.clear();
                value.text = format!("Artifact {} revision {}", source.family, revision);
            }
            InputKind::Text => {
                value.images.clear();
                if value.text.is_empty() {
                    return Err("Connected text is empty.".into());
                }
            }
            InputKind::Table => {
                value.images.clear();
                if value.text.is_empty() {
                    value.text = "Linked table".into();
                }
            }
            InputKind::Images => {
                value.text.clear();
                // A web page's picture is captured by the caller as the run starts.
                if value.images.is_empty() && !is_web_page(&doc.scene, source) {
                    return Err("Connected images are not ready.".into());
                }
            }
        }
        if let Some(output) = binding
            .output
            .as_ref()
            .filter(|_| binding.kind == InputKind::Images)
        {
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
    for input in &context {
        check_source(NodeId(input.node))?;
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
