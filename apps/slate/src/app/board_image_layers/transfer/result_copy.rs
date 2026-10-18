//! One output-copy owner for drag, single actions and batch actions.
use super::*;

impl SlateApp {
    pub(crate) fn copy_agent_result_to_canvas(
        &mut self,
        host: NodeId,
        output: &str,
        at: Pos2,
    ) -> Option<NodeId> {
        self.copy_agent_results_to_canvas(host, &[(output.to_owned(), at)])
            .into_iter()
            .next()
    }

    /// One action, one journal group; every result retains its own paint stack.
    pub(crate) fn copy_agent_results_to_canvas(
        &mut self,
        host: NodeId,
        placements: &[(String, Pos2)],
    ) -> Vec<NodeId> {
        if self.refuse_read_only_edit() {
            return Vec::new();
        }
        let Some(nodes) = placements
            .iter()
            .map(|(output, at)| self.prepare_agent_result_copy(host, output, *at))
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        self.add_nodes(nodes)
    }

    /// Place an album result as an independent ordinary image. It carries
    /// host-authored styles and only that result's remapped paint stack.
    fn prepare_agent_result_copy(
        &mut self,
        host_id: NodeId,
        output_id: &str,
        at: Pos2,
    ) -> Option<Node> {
        if self.refuse_read_only_edit() {
            return None;
        }
        let host = self.doc().scene.node(host_id)?.clone();
        let NodeKind::Image(host_img) = &host.kind else {
            return None;
        };
        let images = self.agent_images(host_id);
        let output = images.iter().find(|output| output.id == output_id)?.clone();
        let result = ImageResultId::Output(output.id.clone());
        let mut source_img = host_img.clone();
        if self.image_result_id(host_id).as_ref() == Some(&result)
            && result_layers(&source_img, &result).is_none()
            && !source_img.paint_layers.is_empty()
        {
            seed_result_from_legacy(&mut source_img, result.clone());
        }
        let path =
            crate::app::board_portal::resolve_source(self.tab().path.as_deref(), &output.source);
        let item = self.item_for_ready_path(&path, 0, 0);
        let mut placed = host_img.clone();
        placed.item = item;
        placed.agent = None;
        placed.paint_layers.clear();
        placed.result_layers =
            slate_doc::image_paint::clone_result_layers_to_item(&source_img, &result, item, || {
                self.doc_mut()
                    .scene
                    .build_node(
                        host.rect,
                        NodeKind::Image(ImageNode::new(slate_doc::ItemId::NONE)),
                    )
                    .id
            })
            .into_iter()
            .collect();
        let mut node = self.doc_mut().scene.build_node(
            slate_doc::scene::WorldRect::new(at.x, at.y, host.rect.w, host.rect.h),
            NodeKind::Image(placed),
        );
        node.rotation_deg = host.rotation_deg;
        node.opacity = host.opacity;
        node.clip = host.clip;
        Some(node)
    }
}
