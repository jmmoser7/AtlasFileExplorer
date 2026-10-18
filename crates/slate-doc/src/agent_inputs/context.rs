//! Explicit context and pocket ownership.
use super::*;

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
        let (source, card) = (
            endpoint_node(binding.source_end(c))?,
            endpoint_node(binding.target_end(c))?,
        );
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

pub(super) fn shape_brief(node: &Node, shape: &ShapeNode) -> String {
    let kind = match shape.shape {
        ShapeKind::Rect => "rectangle",
        ShapeKind::Ellipse => "ellipse",
        ShapeKind::RegularPolygon => "polygon",
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
