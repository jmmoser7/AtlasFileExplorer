//! One command plan for authored marks entering an image's active layer.
use super::*;
use slate_doc::image_paint::layer_node_kind_allowed;

pub(crate) struct PaintLayerPlan {
    pub commands: Vec<SceneCmd>,
    pub nodes: Vec<NodeId>,
    image: NodeId,
    layer_index: usize,
}

impl PaintLayerPlan {
    fn empty(image: NodeId) -> Self {
        Self {
            commands: Vec::new(),
            nodes: Vec::new(),
            image,
            layer_index: 0,
        }
    }
}

impl SlateApp {
    /// Apply the same plan used when capturing an in-flight text draft.
    pub fn commit_paint_layer_nodes(&mut self, nodes: Vec<Node>) -> Option<Vec<NodeId>> {
        let plan = self.paint_layer_node_plan(nodes)?;
        if plan.commands.is_empty() || !self.commit_scene(plan.commands) {
            return Some(Vec::new());
        }
        if let Some(session) = &mut self.image_paint {
            if session.image == plan.image {
                session.layer_index = plan.layer_index;
                session.focus = ImageStripFocus::Layer(plan.layer_index);
            }
        }
        Some(plan.nodes)
    }

    /// Immutable document commands: no file reads, live commits or focus changes.
    pub(crate) fn paint_layer_node_plan(&self, nodes: Vec<Node>) -> Option<PaintLayerPlan> {
        let session = self.image_paint.clone()?;
        if nodes.is_empty() {
            return Some(PaintLayerPlan::empty(session.image));
        }
        if self.image_result_id(session.image).as_ref() != Some(&session.result) {
            return Some(PaintLayerPlan::empty(session.image));
        }
        if !nodes.iter().all(|n| layer_node_kind_allowed(&n.kind)) {
            return Some(PaintLayerPlan::empty(session.image));
        }
        let Some(host) = self.doc().scene.node(session.image).cloned() else {
            return Some(PaintLayerPlan::empty(session.image));
        };
        let NodeKind::Image(ref host_img) = host.kind else {
            return Some(PaintLayerPlan::empty(session.image));
        };
        let accepted: Vec<Node> = nodes
            .into_iter()
            .filter(|n| self.stroke_intersects_image_window(&host, host_img, n))
            .collect();
        if accepted.is_empty() {
            return Some(PaintLayerPlan::empty(session.image));
        }
        let mut locals: Vec<Node> = accepted
            .iter()
            .map(|n| layer_node_from_world(&host, host_img, n))
            .collect();
        if let Some(contours) = self.image_selection_clip(session.image) {
            for local in &mut locals {
                local.clip = clip_path_for_layer_node(local, &contours);
            }
        }
        let ids: Vec<NodeId> = locals.iter().map(|n| n.id).collect();
        let active_layer_id =
            Self::session_layer_raw_index(host_img, &session.result, session.layer_index)
                .and_then(|index| Self::selected_layers(host_img, &session.result).get(index))
                .map(|layer| layer.id);
        let mut cmds = Vec::new();
        let mut after = host.clone();
        let NodeKind::Image(ref mut after_img) = after.kind else {
            return Some(PaintLayerPlan::empty(session.image));
        };
        Self::prepare_result_stack(after_img, session.result.clone());
        let needs_layer = Self::selected_layers(after_img, &session.result).is_empty();
        let layer_id = if needs_layer {
            let layer_id = Self::next_paint_layer_id(after_img);
            Self::selected_stack_mut(after_img, &session.result)
                .add_layer(PaintLayer::new(layer_id));
            layer_id
        } else {
            let layers = Self::selected_layers(after_img, &session.result);
            active_layer_id
                .filter(|id| layers.iter().any(|layer| layer.id == *id))
                .unwrap_or(layers[0].id)
        };
        let layer_index = Self::selected_layers(after_img, &session.result)
            .iter()
            .position(|layer| layer.id == layer_id)
            .unwrap_or(0);
        if after != host {
            cmds.push(SceneCmd::Patch {
                before: Box::new(host.clone()),
                after: Box::new(after),
            });
        }
        let base = self
            .doc()
            .scene
            .node(session.image)
            .and_then(|n| match &n.kind {
                NodeKind::Image(img) => layers_for_result(img, &session.result)
                    .iter()
                    .find(|l| l.id == layer_id)
                    .map(|l| l.nodes.len()),
                _ => None,
            })
            .unwrap_or(0);
        for (i, local) in locals.into_iter().enumerate() {
            cmds.push(SceneCmd::LayerNodeAdd {
                host: session.image,
                layer: layer_id,
                index: base + i,
                node: local,
            });
        }
        Some(PaintLayerPlan {
            commands: cmds,
            nodes: ids,
            image: host.id,
            layer_index,
        })
    }
}
