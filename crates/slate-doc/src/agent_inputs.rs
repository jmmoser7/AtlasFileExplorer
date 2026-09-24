//! The semantic layer of existing board wires. Geometry remains in `wire_host`.
//! Resolves one immutable run input; adapters never interpret a scene.
use crate::{scene::*, SlateDoc};
use atlas_agent::{ContextItem, ImageOutput, InputSlot, InputSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Text,
    Images,
    /// A spreadsheet feeding a dashboard. The page reads `window.slateLink`.
    Table,
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
    /// Set once a send has included this wire. A used context stays attached.
    #[serde(default)]
    pub consumed: bool,
    /// Named dashboard input (empty means `data`, the protocol's primary
    /// table), or the generator / text block port the wire lands on
    /// (`InputSlot::id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
}

pub fn endpoint_node(end: &ConnectorEnd) -> Option<NodeId> {
    match end {
        ConnectorEnd::Anchored { node, .. } => Some(*node),
        _ => None,
    }
}

pub fn infer_binding(scene: &Scene, a: &ConnectorEnd, b: &ConnectorEnd) -> Option<WireBinding> {
    infer_binding_with(scene, a, b, &|_| None)
}

/// `item_path` resolves a placed file. The scene stores an id, not the path,
/// and this function still does no I/O.
pub fn infer_binding_with<'a>(
    scene: &Scene,
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<WireBinding> {
    directional_binding(scene, a, b, true, item_path)
        .or_else(|| directional_binding(scene, b, a, false, item_path))
}

fn directional_binding<'a>(
    scene: &Scene,
    source: &ConnectorEnd,
    target: &ConnectorEnd,
    input_b: bool,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<WireBinding> {
    let midpoint = matches!(target, ConnectorEnd::Anchored { side: Side::Left, t, .. } if (*t - 0.5).abs() < 0.001);
    let target_id = endpoint_node(target)?;
    let flow = is_flow_node(scene, target_id);
    if !midpoint && !flow {
        return None;
    }
    // A flow node's output port is never one of its inputs.
    if flow
        && matches!(target, ConnectorEnd::Anchored { side: Side::Right, t, .. } if (*t - OUTPUT_T).abs() < 0.001)
    {
        return None;
    }
    let port = target_slot(scene, target_id, target);
    let source = scene.node(endpoint_node(source)?)?;
    let target = scene.node(target_id)?;
    if let Some(binding) = table_link(source, target, input_b, item_path) {
        return Some(binding);
    }
    if !crate::agent_chat::is_agent_node(target) {
        return None;
    }
    let mut kind = source_kind(source, item_path)?;
    // A web page carries its locator as words, and on a picture port the
    // picture it shows (captured when the run starts, pixels only).
    if web_page_node(source) && port.is_some_and(|slot| !slot.takes_text()) {
        kind = InputKind::Images;
    }
    let slot = match port {
        Some(slot) if slot.takes_text() != (kind == InputKind::Text) => return None,
        slot => slot,
    };
    Some(WireBinding {
        input_b,
        kind,
        output: None,
        order: vec![],
        all_images: false,
        consumed: false,
        slot: slot.map(|s| s.id().to_string()),
    })
}

/// One input port on the left edge of a generator or text block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputPort {
    pub slot: InputSlot,
    /// Fraction down the left edge.
    pub t: f32,
    pub label: &'static str,
}

/// The one output port sits at the right midpoint.
pub const OUTPUT_T: f32 = 0.5;

const GENERATOR_PORTS: [InputPort; 3] = [
    InputPort {
        slot: InputSlot::Media,
        t: 0.25,
        label: "Media",
    },
    InputPort {
        slot: InputSlot::Prompt,
        t: 0.5,
        label: "Prompt",
    },
    InputPort {
        slot: InputSlot::Style,
        t: 0.75,
        label: "Style",
    },
];

const TEXT_PORTS: [InputPort; 2] = [
    InputPort {
        slot: InputSlot::Media,
        t: 1.0 / 3.0,
        label: "Image",
    },
    InputPort {
        slot: InputSlot::Prompt,
        t: 2.0 / 3.0,
        label: "Prompt",
    },
];

/// The input ports a node draws and binds, top to bottom. Empty for every node
/// that is not a generator or text block. `WireHost::ports` reads this table.
pub fn input_ports_of(node: &Node) -> &'static [InputPort] {
    match flow_view(node) {
        Some(atlas_agent::PortalView::Images) => &GENERATOR_PORTS,
        Some(atlas_agent::PortalView::Text) => &TEXT_PORTS,
        _ => &[],
    }
}

fn flow_view(node: &Node) -> Option<atlas_agent::PortalView> {
    crate::agent_chat::agent(node)
        .map(|a| a.view)
        .filter(|v| *v != atlas_agent::PortalView::Chat)
}

pub fn input_ports(scene: &Scene, id: NodeId) -> &'static [InputPort] {
    scene.node(id).map(input_ports_of).unwrap_or(&[])
}

/// Generators and text blocks read typed ports and take a wire on any edge.
pub fn is_flow_node(scene: &Scene, id: NodeId) -> bool {
    !input_ports(scene, id).is_empty()
}

fn target_slot(scene: &Scene, id: NodeId, end: &ConnectorEnd) -> Option<InputSlot> {
    let ConnectorEnd::Anchored {
        side: Side::Left,
        t,
        ..
    } = end
    else {
        return None;
    };
    input_ports(scene, id)
        .iter()
        .find(|p| (p.t - *t).abs() < 0.001)
        .map(|p| p.slot)
}

fn web_page_node(node: &Node) -> bool {
    matches!(&node.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web && p.source.is_some())
}

/// A bound web portal. Agents read the picture it shows, never its DOM.
pub fn is_web_page(scene: &Scene, id: NodeId) -> bool {
    scene.node(id).is_some_and(web_page_node)
}

/// A wire from `source` into a flow node fills a picture port: a picture, a
/// 3D model or video, or a web page's shown view.
pub fn feeds_picture<'a>(
    scene: &Scene,
    source: NodeId,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> bool {
    is_web_page(scene, source) || wire_kind(scene, source, item_path) == Some(InputKind::Images)
}

/// What a wire from `source` carries, if it carries anything.
pub fn wire_kind<'a>(
    scene: &Scene,
    source: NodeId,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<InputKind> {
    source_kind(scene.node(source)?, item_path)
}

/// Where a wire from `source` lands when it is dropped on `target` away from a
/// port: the first free port that reads what it carries, else the first that
/// reads it at all.
pub fn default_port<'a>(
    scene: &Scene,
    target: NodeId,
    source: NodeId,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<InputPort> {
    wire_kind(scene, source, item_path)?;
    let text = !feeds_picture(scene, source, item_path);
    let ports = input_ports(scene, target);
    let taken = bound_slots(scene, target);
    let fits = |p: &&InputPort| p.slot.takes_text() == text;
    ports
        .iter()
        .filter(fits)
        .find(|p| !taken.contains(&p.slot))
        .or_else(|| ports.iter().find(fits))
        .copied()
}

/// Chat views, not image generators. Only these consume a wired context.
pub fn is_chat_card(scene: &Scene, id: NodeId) -> bool {
    scene.node(id).is_some_and(|n| {
        matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent)
            && crate::agent_chat::agent(n).is_some_and(|a| a.view == atlas_agent::PortalView::Chat)
    })
}

/// `(wire, source, card)` for every wire whose context a chat card already sent.
/// A chat card wired as context is a conversation view, never pocketed context:
/// bundles hide cards too.
fn sent_context(scene: &Scene) -> impl Iterator<Item = (NodeId, NodeId, NodeId)> + '_ {
    scene.nodes.iter().filter_map(|n| {
        let NodeKind::Connector(c) = &n.kind else {
            return None;
        };
        let binding = c.binding.as_ref().filter(|b| b.consumed)?;
        let (from, to) = if binding.input_b {
            (&c.a, &c.b)
        } else {
            (&c.b, &c.a)
        };
        let (source, card) = (endpoint_node(from)?, endpoint_node(to)?);
        (is_chat_card(scene, card) && !is_chat_card(scene, source)).then_some((n.id, source, card))
    })
}

/// `(wire, card)` for every sent context wire from `source` into an existing
/// chat card. A used context stays attached, so deleting it pockets it there.
pub fn consumed_by(scene: &Scene, source: NodeId) -> Vec<(NodeId, NodeId)> {
    sent_context(scene)
        .filter(|(_, from, _)| *from == source)
        .map(|(wire, _, card)| (wire, card))
        .collect()
}

/// Hidden context nodes that `card` already sent: the card's pocket.
pub fn pocketed(scene: &Scene, card: NodeId) -> Vec<NodeId> {
    let mut found = Vec::new();
    for (_, source, to) in sent_context(scene) {
        if to == card && !found.contains(&source) && scene.node(source).is_some_and(|s| s.hidden) {
            found.push(source);
        }
    }
    found
}

/// Every chat card with a non-empty pocket.
pub fn pocket_holders(scene: &Scene) -> BTreeSet<NodeId> {
    sent_context(scene)
        .filter(|(_, source, _)| scene.node(*source).is_some_and(|s| s.hidden))
        .map(|(_, _, card)| card)
        .collect()
}

/// Ports of `target` that already have a bound wire.
pub fn bound_slots(scene: &Scene, target: NodeId) -> Vec<InputSlot> {
    scene
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Connector(c) => {
                let binding = c.binding.as_ref()?;
                let end = if binding.input_b { &c.b } else { &c.a };
                (endpoint_node(end) == Some(target))
                    .then(|| binding.slot.as_deref().and_then(InputSlot::from_id))
                    .flatten()
            }
            _ => None,
        })
        .collect()
}

/// What a node carries along a wire: the one answer the selection strip, the
/// spawn routing and wire binding all read. A placed text document carries
/// words; every other placed file carries pictures (a video its shown frame,
/// a 3D model its captured view). `item_path` resolves a placed file without I/O.
fn source_kind<'a>(
    source: &Node,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<InputKind> {
    Some(match &source.kind {
        NodeKind::Text(_) => InputKind::Text,
        NodeKind::Shape(_) => InputKind::Text,
        NodeKind::Image(image)
            if image.agent.is_none()
                && item_path(image.item).is_some_and(|p| {
                    crate::media::media_kind(p) == crate::media::MediaKind::Text
                }) =>
        {
            InputKind::Text
        }
        NodeKind::Image(_) => InputKind::Images,
        NodeKind::Portal(p) if p.kind == PortalKind::Agent => {
            if crate::agent_chat::agent(source)?.view == atlas_agent::PortalView::Images {
                InputKind::Images
            } else {
                InputKind::Text
            }
        }
        NodeKind::Portal(p) if p.source.is_some() => InputKind::Text,
        _ => return None,
    })
}

/// The result an agent picture shows while nobody has picked one: the newest.
/// The board painter, the wire snapshot and the artifact writer all ask this.
pub fn newest_image(images: &[ImageOutput]) -> Option<&ImageOutput> {
    images.iter().rev().find(|i| !i.source.is_empty())
}

/// An image generator: the flow node whose output is pictures.
pub fn is_image_generator(scene: &Scene, id: NodeId) -> bool {
    scene.node(id).and_then(flow_view) == Some(atlas_agent::PortalView::Images)
}

/// Wires drawn before generators accepted any edge were stored unbound.
/// Only anchored, non-provenance ends count; a free end supplies nothing.
fn legacy_generator_input(
    scene: &Scene,
    conn: &ConnectorNode,
    portal: NodeId,
) -> Option<WireBinding> {
    if !is_image_generator(scene, portal) || conn.display == WireDisplay::Faint {
        return None;
    }
    let (a, b) = (endpoint_node(&conn.a), endpoint_node(&conn.b));
    if a.is_none() || b.is_none() || a == b || ![a, b].contains(&Some(portal)) {
        return None;
    }
    infer_binding(scene, &conn.a, &conn.b)
}

/// A spreadsheet wired into the left middle of a web portal feeds its chart.
fn table_link<'a>(
    source: &Node,
    target: &Node,
    input_b: bool,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<WireBinding> {
    if !matches!(&target.kind, NodeKind::Portal(p) if p.kind == PortalKind::Web) {
        return None;
    }
    let NodeKind::Image(image) = &source.kind else {
        return None;
    };
    let path = item_path(image.item)?;
    if !crate::media::is_table_source(path) {
        return None;
    }
    Some(WireBinding {
        input_b,
        kind: InputKind::Table,
        output: None,
        order: vec![],
        all_images: false,
        consumed: false,
        slot: None,
    })
}

/// Existing direction takes precedence when an endpoint changes. Pins describe
/// the source, so moving only the receiving endpoint preserves them.
pub fn rebind(scene: &Scene, before: &ConnectorNode, after: &ConnectorNode) -> Option<WireBinding> {
    rebind_with(scene, before, after, &|_| None)
}

pub fn rebind_with<'a>(
    scene: &Scene,
    before: &ConnectorNode,
    after: &ConnectorNode,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<WireBinding> {
    if endpoint_node(&after.a).is_none() || endpoint_node(&after.b).is_none() {
        return before.binding.clone();
    }
    if let Some(old) = &before.binding {
        let (old_source, source, target) = if old.input_b {
            (&before.a, &after.a, &after.b)
        } else {
            (&before.b, &after.b, &after.a)
        };
        if let Some(mut binding) =
            directional_binding(scene, source, target, old.input_b, item_path)
        {
            binding.order = old.order.clone();
            binding.consumed = old.consumed;
            if binding.kind == old.kind && endpoint_node(old_source) == endpoint_node(source) {
                binding.output = old.output.clone();
                binding.all_images = old.all_images;
                // A port slot follows the receiving anchor; a dashboard slot is authored.
                if binding.kind == InputKind::Table {
                    binding.slot = old.slot.clone();
                }
            }
            return Some(binding);
        }
    }
    let mut binding = infer_binding_with(scene, &after.a, &after.b, item_path)?;
    if let Some(old) = &before.binding {
        binding.order = old.order.clone();
        binding.consumed = old.consumed;
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

fn shape_brief(node: &Node, shape: &ShapeNode) -> String {
    let kind = match shape.shape {
        ShapeKind::Rect => "rectangle",
        ShapeKind::Ellipse => "ellipse",
        ShapeKind::Line => "line",
        ShapeKind::Path => "path",
    };
    let mut lines = vec![format!(
        "Sketch {kind} {:.0}×{:.0} at ({:.0}, {:.0})",
        node.rect.w, node.rect.h, node.rect.x, node.rect.y
    )];
    if let Some(text) = &shape.text {
        let body = text.body.trim();
        if !body.is_empty() {
            lines.push(body.to_string());
        }
    }
    if let Some(path) = &shape.path {
        lines.push(path_brief(path));
    }
    lines.join("\n")
}

fn path_brief(path: &PathData) -> String {
    let mut pts = vec![path.start];
    for seg in path.segs.iter().take(80) {
        match seg {
            PathSeg::Line { to } | PathSeg::Quad { to, .. } | PathSeg::Cubic { to, .. } => {
                pts.push(*to);
            }
        }
    }
    let body: String = pts
        .iter()
        .map(|p| format!("{:.0},{:.0}", p[0], p[1]))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "path {}{}",
        body,
        if path.segs.len() > 80 { " …" } else { "" }
    )
}

/// Completed upstream values only. A wire never runs another program implicitly.
pub fn snapshot(
    doc: &SlateDoc,
    portal: NodeId,
    scope: AgentContextScope,
    selection: &[NodeId],
    outputs: &BTreeMap<NodeId, ContextItem>,
) -> Result<InputSnapshot, String> {
    // A flow node's prompt is a shape's words; chat agents also read its geometry.
    let generator = is_flow_node(&doc.scene, portal);
    let item = |id| -> Option<ContextItem> {
        let node = doc.scene.node(id)?;
        Some(match &node.kind {
            // An agent's reply the person has not made their own yet.
            NodeKind::Text(t) if t.agent.is_some() && t.text.trim().is_empty() => {
                outputs.get(&id)?.clone()
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
            // A picked result is an ordinary placed picture.
            NodeKind::Image(i) if i.agent.is_some() && !i.item.is_none() => ContextItem {
                node: id.0,
                text: String::new(),
                images: vec![doc.item(i.item)?.path.to_string_lossy().into_owned()],
                outputs: Default::default(),
                active: None,
                depth: None,
                slot: None,
            },
            // A placed text document's words, or an agent picture's newest
            // result, read by the caller.
            NodeKind::Image(_) if outputs.contains_key(&id) => outputs.get(&id)?.clone(),
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
            NodeKind::Connector(c) => Some((
                c.binding
                    .as_ref()
                    .map(|b| {
                        if b.order.is_empty() {
                            vec![n.id.0]
                        } else {
                            b.order.clone()
                        }
                    })
                    .unwrap_or_else(|| vec![n.id.0]),
                n,
            )),
            _ => None,
        })
        .collect();
    wires.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
    for (_, node) in wires {
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
        let slot = binding.slot.as_deref().and_then(InputSlot::from_id);
        if !seen.insert((
            source,
            format!("{:?}{slot:?}", binding.kind),
            binding.output.clone(),
            binding.all_images,
        )) {
            continue;
        }
        let mut value = item(source).ok_or("A connected source has no completed output yet.")?;
        value.slot = slot;
        match binding.kind {
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
                routing: None,
                a,
                b,
                binding: None,
                stroke: Stroke::none(),
                arrow_a: false,
                arrow_b: false,
                label: None,
                display,
            })
        };
        // An unbound wire saved by an older build still feeds the generator.
        add(
            &mut doc,
            wire(from.clone(), off_midpoint(target), WireDisplay::Default),
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
            family: Default::default(),
            size: 16.0,
            color: Rgba::BLACK,
            align: Default::default(),
            fill: None,
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
                routing: None,
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
    fn old_wires_and_portals_deserialize_without_new_fields() {
        let p = PortalNode::unbound_agent("legacy", "cursor");
        let mut value = serde_json::to_value(&p).unwrap();
        for field in ["bundle", "seed", "view"] {
            value["agent"].as_object_mut().unwrap().remove(field);
        }
        let read: PortalNode = serde_json::from_value(value).unwrap();
        assert_eq!(read.agent.unwrap().view, atlas_agent::PortalView::Chat);
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
                flip: false,
                path: None,
                text: Some(ShapeText {
                    body: "north wing".into(),
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
                routing: None,
                a,
                b,
                binding: Some(binding),
                stroke: Stroke::none(),
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
                routing: None,
                a,
                b,
                binding,
                stroke: Stroke::none(),
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
                routing: None,
                a,
                b,
                binding,
                stroke: Stroke::none(),
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
        let NodeKind::Text(written) = &doc.scene.node(block).unwrap().kind else {
            panic!("a text block opens as a note");
        };
        assert!(written.text.is_empty() && written.agent.is_some());
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
                routing: None,
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
        let (g, b) = (
            doc.scene.index_of(generator).unwrap(),
            doc.scene.index_of(block).unwrap(),
        );
        if let NodeKind::Image(i) = &mut doc.scene.nodes[g].kind {
            i.item = picked;
        }
        if let NodeKind::Text(t) = &mut doc.scene.nodes[b].kind {
            t.text = "My own words.".into();
        }
        let snap = snapshot(&doc, reader, AgentContextScope::Selection, &[], &outputs).unwrap();
        assert!(snap
            .wired
            .iter()
            .any(|w| w.images.iter().any(|i| i.ends_with("run-1.png"))));
        assert!(snap.wired.iter().any(|w| w.text == "My own words."));

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
}
