//! Model-node queries shared by painting and interaction.
use super::*;

impl SlateApp {
    /// Snapshot of the node when it's a placed 3D model.
    pub fn model_node_info(&self, id: NodeId) -> Option<ModelNodeInfo> {
        self.model_node_info_in(self.tab().id, id)
    }

    pub(crate) fn model_node_info_in(&self, doc: u64, id: NodeId) -> Option<ModelNodeInfo> {
        let tab = self.tabs.iter().find(|tab| tab.id == doc)?;
        let node = tab.doc.scene.node(id)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let item = tab.doc.item(img.item)?;
        let recognized = slate_doc::media_kind(&item.path) == slate_doc::MediaKind::Model
            || self.model3d.external.contains(&item.cache_key);
        if !recognized || item.cache_key.is_empty() {
            return None;
        }
        Some(ModelNodeInfo {
            node: id,
            cache_key: item.cache_key.clone(),
            path: slate_doc::scene::resolve_source(
                tab.path.as_deref(),
                &item.path.to_string_lossy(),
            ),
            cam: img.model,
            rect: node.rect,
        })
    }

    /// All placed model nodes in the active document.
    pub fn model_nodes(&self) -> Vec<ModelNodeInfo> {
        self.doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| self.model_node_info(n.id))
            .collect()
    }

    /// The topmost node at a world point when it's an *unlocked* viewport.
    pub fn live_model_at(&self, wx: f32, wy: f32) -> Option<NodeId> {
        let id = self.doc().scene.node_at(wx, wy)?;
        (self.board_selected_frame_owner(id).is_none()
            && self.model3d.live.contains_key(&id)
            && self.model_node_info(id).is_some())
        .then_some(id)
    }
}
