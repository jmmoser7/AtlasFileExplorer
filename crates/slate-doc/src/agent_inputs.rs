//! The semantic layer of existing board wires. Geometry remains in `wire_host`.
//! Resolves one immutable run input; adapters never interpret a scene.
mod context;
mod ports;
use context::shape_brief;
pub use context::{consumed_by, context_nodes, is_chat_card, pocket_holders, pocketed};
mod unbundle;
pub use unbundle::unbundle;
mod artifact;
pub use artifact::{
    artifact_content_parent, artifact_memory_parent, artifact_memory_source, ArtifactMemoryPin,
};
pub use ports::*;
#[cfg(test)]
mod compatibility_tests;
#[cfg(test)]
use ports::TEXT_PORTS;
use ports::{flow_view, model_view_target};

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
    /// Exact source revision of a generated interactive application.
    Artifact,
    /// A conversation checkpoint; never part of a prompt's content snapshot.
    Memory,
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

impl WireBinding {
    /// The connector end that supplies data along this wire.
    pub fn source_end<'a>(&self, conn: &'a ConnectorNode) -> &'a ConnectorEnd {
        if self.input_b {
            &conn.a
        } else {
            &conn.b
        }
    }

    /// The connector end that receives data along this wire.
    pub fn target_end<'a>(&self, conn: &'a ConnectorNode) -> &'a ConnectorEnd {
        if self.input_b {
            &conn.b
        } else {
            &conn.a
        }
    }

    pub fn source_end_mut<'a>(&self, conn: &'a mut ConnectorNode) -> &'a mut ConnectorEnd {
        if self.input_b {
            &mut conn.a
        } else {
            &mut conn.b
        }
    }

    pub fn target_end_mut<'a>(&self, conn: &'a mut ConnectorNode) -> &'a mut ConnectorEnd {
        if self.input_b {
            &mut conn.b
        } else {
            &mut conn.a
        }
    }
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
    let model_view = model_view_target(scene, target_id, item_path);
    let flow = !input_ports_with(scene, target_id, item_path).is_empty();
    if !midpoint && !flow {
        return None;
    }
    // A flow node's output port is never one of its inputs.
    if flow
        && !model_view
        && matches!(target, ConnectorEnd::Anchored { side: Side::Right, t, .. } if (*t - OUTPUT_T).abs() < 0.001)
    {
        return None;
    }
    let port = target_slot(scene, target_id, target, item_path);
    if let Some(binding) = model_view_binding(scene, source, target, target_id, input_b, item_path)
    {
        return Some(binding);
    }
    let source = scene.node(endpoint_node(source)?)?;
    let target = scene.node(target_id)?;
    if let Some(binding) = table_link(source, target, input_b, item_path) {
        return Some(binding);
    }
    if !crate::agent_chat::is_agent_node(target) && !image_generation_target(target, item_path) {
        return None;
    }
    let mut kind = source_kind(source, item_path)?;
    if kind == InputKind::Artifact && artifact(target).is_none() {
        kind = InputKind::Text;
    }
    // A web page carries its locator as words, and on a picture port the
    // picture it shows (captured when the run starts, pixels only).
    if kind != InputKind::Artifact
        && web_page_node(source)
        && port.is_some_and(|slot| !slot.takes_text())
    {
        kind = InputKind::Images;
    }
    let slot = match port {
        Some(slot) if slot.takes_text() != (kind == InputKind::Text) => return None,
        slot => slot,
    };
    Some(WireBinding {
        input_b,
        kind,
        output: (kind == InputKind::Artifact)
            .then(|| artifact(source)?.revision.clone())
            .flatten(),
        order: vec![],
        all_images: false,
        consumed: false,
        slot: slot.map(|s| s.id().to_string()),
    })
}

fn model_view_binding<'a>(
    scene: &Scene,
    source_end: &ConnectorEnd,
    target_end: &ConnectorEnd,
    target_id: NodeId,
    input_b: bool,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<WireBinding> {
    let target_node = scene.node(target_id)?;
    let NodeKind::Image(img) = &target_node.kind else {
        return None;
    };
    let path = item_path(img.item)?;
    if !image_model_view_port(path) {
        return None;
    }
    // Any edge: the port has no resting site, only the wire snap's.
    if !matches!(target_end, ConnectorEnd::Anchored { .. }) {
        return None;
    }
    let source_id = endpoint_node(source_end)?;
    let NodeKind::Image(src) = &scene.node(source_id)?.kind else {
        return None;
    };
    item_path(src.item)?;
    Some(WireBinding {
        input_b,
        kind: InputKind::Images,
        output: None,
        order: vec![],
        all_images: false,
        consumed: false,
        slot: Some(InputSlot::View.id().into()),
    })
}

/// Generators and text blocks read typed ports and take a wire on any edge.
pub fn is_flow_node(scene: &Scene, id: NodeId) -> bool {
    !input_ports(scene, id).is_empty()
}

fn target_slot<'a>(
    scene: &Scene,
    id: NodeId,
    end: &ConnectorEnd,
    item_path: &dyn Fn(crate::ids::ItemId) -> Option<&'a std::path::Path>,
) -> Option<InputSlot> {
    let ConnectorEnd::Anchored {
        side: Side::Left,
        t,
        ..
    } = end
    else {
        return None;
    };
    input_ports_with(scene, id, item_path)
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
    is_web_page(scene, source)
        || matches!(
            wire_kind(scene, source, item_path),
            Some(InputKind::Images | InputKind::Artifact)
        )
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
    let ports = input_ports_with(scene, target, item_path);
    let taken = bound_slots(scene, target);
    let fits = |p: &&InputPort| p.slot.takes_text() == text;
    ports
        .iter()
        .filter(fits)
        .find(|p| !taken.contains(&p.slot))
        .or_else(|| ports.iter().find(fits))
        .copied()
}

/// Ports of `target` that already have a bound wire.
pub fn bound_slots(scene: &Scene, target: NodeId) -> Vec<InputSlot> {
    scene
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Connector(c) => {
                let binding = c.binding.as_ref()?;
                let end = binding.target_end(c);
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
        _ if artifact(source).is_some() => InputKind::Artifact,
        NodeKind::Text(_) => InputKind::Text,
        NodeKind::Shape(_) => InputKind::Text,
        NodeKind::Image(image)
            if image
                .agent
                .as_ref()
                .is_some_and(|a| a.view == atlas_agent::PortalView::Text) =>
        {
            InputKind::Text
        }
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
        if old.kind == InputKind::Memory {
            return artifact::rebind_memory(scene, before, after, old);
        }
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

mod snapshot;
pub use snapshot::{snapshot, snapshot_with_source_check};

#[cfg(test)]
mod tests;
