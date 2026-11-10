//! Existing generator editor eligibility for bound media and flow nodes.
use super::*;

impl SlateApp {
    /// Chat-bound media uses the same provider/catalog owner as Chat Train.
    /// This is editor eligibility only; it never changes flow-port semantics.
    pub(crate) fn bound_chat_generator_host(&self, id: NodeId) -> bool {
        let Some(node) = self.doc().scene.node(id) else {
            return false;
        };
        if !slate_doc::agent_chat::agent(node).is_some_and(|agent| agent.view == PortalView::Chat) {
            return false;
        }
        match &node.kind {
            NodeKind::Text(_) => true,
            NodeKind::Image(image) => {
                self.model_node_info(id).is_some()
                    || self.doc().item(image.item).is_some_and(|item| {
                        slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Text
                    })
            }
            _ => false,
        }
    }

    pub(crate) fn has_attached_generator_editor(&self, id: NodeId) -> bool {
        agent_inputs::is_flow_node(&self.doc().scene, id) || self.bound_chat_generator_host(id)
    }
}
