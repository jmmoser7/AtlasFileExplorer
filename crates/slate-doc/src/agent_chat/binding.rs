//! One lookup owner for nodes carrying an agent binding.
use super::*;

/// The agent bound to a node: a chat card's portal binding, or the agent an
/// image or a text note was summoned with. The one owner of that lookup.
pub fn agent(node: &Node) -> Option<&AgentPortalRef> {
    match &node.kind {
        NodeKind::Portal(p) => p.agent.as_deref(),
        NodeKind::Image(i) => i.agent.as_deref(),
        NodeKind::Text(t) => t.agent.as_deref(),
        _ => None,
    }
}

pub fn agent_mut(node: &mut Node) -> Option<&mut AgentPortalRef> {
    match &mut node.kind {
        NodeKind::Portal(p) => p.agent.as_deref_mut(),
        NodeKind::Image(i) => i.agent.as_deref_mut(),
        NodeKind::Text(t) => t.agent.as_deref_mut(),
        _ => None,
    }
}

/// An agent card of any kind: a chat portal, a generating image, a text note
/// an agent writes, or a not-yet-converted generator or text block.
pub fn is_agent_node(node: &Node) -> bool {
    match &node.kind {
        NodeKind::Portal(p) => {
            p.kind == crate::scene::PortalKind::Agent
                || (crate::scene::artifact(node).is_some() && p.agent.is_some())
        }
        _ => agent(node).is_some(),
    }
}

/// Chat header anchors apply to chat cards, never an artifact's content ports.
pub fn is_chat_host(node: &Node) -> bool {
    crate::scene::artifact(node).is_none()
        && agent(node).is_some_and(|agent| agent.view == atlas_agent::PortalView::Chat)
}

/// True when the clipboard payload is a chat train that must fork on paste.
pub fn paste_is_train_fork(payload: &[crate::scene::Node]) -> bool {
    payload.iter().any(|n| {
        agent(n).is_some_and(|a| {
            a.chat.train && is_chat_host(n) && matches!(n.kind, crate::scene::NodeKind::Portal(_))
        })
    })
}

/// A copied card is a new view of the same checkpoint, never a new history edge
/// into a different workbook. Copying a whole train preserves internal refs.
pub fn remap_view(node: &mut Node, remap: impl Fn(NodeId) -> Option<NodeId>) {
    if let Some(artifact) = crate::scene::artifact_mut(node) {
        artifact.workspace = None;
        if let Some(agent) = agent_mut(node) {
            agent.session = crate::scene::new_agent_session_id();
            agent.channel = None;
            agent.bundle = None;
            agent.chat = Default::default();
        }
        return;
    }
    if let NodeKind::Portal(p) = &mut node.kind {
        if let Some(a) = &mut p.agent {
            a.chat.parent = a.chat.parent.and_then(&remap);
            a.chat.bundled = a.chat.bundled.iter().filter_map(|id| remap(*id)).collect();
            for layer in &mut a.chat.bundle_layers {
                *layer = layer.iter().filter_map(|id| remap(*id)).collect();
            }
        }
    }
}
