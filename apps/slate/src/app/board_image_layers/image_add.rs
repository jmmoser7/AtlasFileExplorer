//! The one owner for adding linked image pixels to an image's layer stack.
use super::*;

impl SlateApp {
    pub fn add_dropped_image_as_layer(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        remove_source: Option<NodeId>,
    ) -> bool {
        self.add_image_to_layer(target, item, remove_source, false)
    }

    pub(crate) fn add_picked_image_layer(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
    ) -> bool {
        self.add_image_to_layer(target, item, None, true)
    }

    fn add_image_to_layer(
        &mut self,
        target: NodeId,
        item: slate_doc::ItemId,
        remove_source: Option<NodeId>,
        fresh_layer: bool,
    ) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let Some(host) = self.doc().scene.node(target).cloned() else {
            return false;
        };
        let NodeKind::Image(ref host_img) = host.kind else {
            return false;
        };
        let Some(result) = self.image_result_id(target) else {
            return false;
        };
        let mut child = self.doc_mut().scene.build_node(
            WorldRect::new(host.rect.x, host.rect.y, host.rect.w, host.rect.h),
            NodeKind::Image(ImageNode::new(item)),
        );
        child.rotation_deg = host.rotation_deg;
        let local = layer_node_from_world(&host, host_img, &child);
        let mut cmds = Vec::new();
        let mut after = host.clone();
        let NodeKind::Image(ref mut after_img) = after.kind else {
            return false;
        };
        let session_layer_id = self
            .image_paint
            .as_ref()
            .filter(|session| session.image == target && session.result == result)
            .and_then(|session| {
                let index = Self::session_layer_raw_index(host_img, &result, session.layer_index)?;
                Self::selected_layers(host_img, &result).get(index)
            })
            .map(|layer| layer.id);
        Self::prepare_result_stack(after_img, result.clone());
        let count = Self::selected_layers(after_img, &result).len();
        let layer_index = session_layer_id
            .and_then(|id| {
                Self::selected_layers(after_img, &result)
                    .iter()
                    .position(|layer| layer.id == id)
            })
            .unwrap_or_else(|| count.saturating_sub(1))
            .min(count.saturating_sub(1));
        let layer_id = if fresh_layer || count == 0 {
            let id = Self::next_paint_layer_id(after_img);
            Self::selected_stack_mut(after_img, &result).add_layer(PaintLayer::new(id));
            id
        } else {
            Self::selected_layers(after_img, &result)[layer_index].id
        };
        if after != host {
            cmds.push(SceneCmd::Patch {
                before: Box::new(host.clone()),
                after: Box::new(after),
            });
        }
        let layer_index = if fresh_layer { count } else { layer_index };
        let base = self
            .doc()
            .scene
            .node(target)
            .and_then(|node| match &node.kind {
                NodeKind::Image(img) => layers_for_result(img, &result)
                    .iter()
                    .find(|layer| layer.id == layer_id)
                    .map(|layer| layer.nodes.len()),
                _ => None,
            })
            .unwrap_or(0);
        cmds.push(SceneCmd::LayerNodeAdd {
            host: target,
            layer: layer_id,
            index: base,
            node: local,
        });
        if let Some(id) = remove_source.filter(|id| *id != target) {
            if let Some(index) = self.doc().scene.nodes.iter().position(|node| node.id == id) {
                if let Some(node) = self.doc().scene.node(id).cloned() {
                    cmds.push(SceneCmd::Remove { index, node });
                }
            }
        }
        if !self.commit_scene(cmds) {
            return false;
        }
        self.image_paint = Some(ImagePaintSession {
            image: target,
            result,
            layer_index,
            focus: ImageStripFocus::Layer(layer_index),
            layer_drag: None,
        });
        self.push_history(
            atlas_commands::CommandId("board.image.layer.add_image"),
            Some("add as layer".into()),
        );
        true
    }
}
