use super::*;

impl SlateApp {
    /// Stable image-result identity selected by an ordinary or generated
    /// picture. Generated images use the active bundle id, which does not
    /// change when the album is reordered.
    pub(crate) fn image_result_id(&self, image: NodeId) -> Option<ImageResultId> {
        let node = self.doc().scene.node(image)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        if img.agent.is_some() {
            self.agent_active_output(image)
                .map(ImageResultId::Output)
                .or_else(|| (!img.item.is_none()).then_some(ImageResultId::Item(img.item)))
        } else {
            Some(ImageResultId::Item(img.item))
        }
    }

    /// Clone the currently displayed image result with its own authored layer
    /// stack in the legacy field expected by raster/composite interpreters.
    pub(crate) fn image_for_selected_result(&self, image: NodeId) -> Option<ImageNode> {
        let node = self.doc().scene.node(image)?;
        let NodeKind::Image(img) = &node.kind else {
            return None;
        };
        let result = self.image_result_id(image)?;
        Some(image_for_result(img, &result))
    }

    /// Place an album result as an independent ordinary image. It carries
    /// host-authored styles and only that result's remapped paint stack.
    pub(crate) fn copy_agent_result_to_canvas(
        &mut self,
        host_id: NodeId,
        output_id: &str,
        at: Pos2,
    ) -> Option<NodeId> {
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
            super::super::board_portal::resolve_source(self.tab().path.as_deref(), &output.source);
        let item = self.item_for_path(&path)?;
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
        let id = node.id;
        (!self.add_nodes(vec![node]).is_empty()).then_some(id)
    }

    /// Copy a hosted vector layer to the board as a flat, selectable group.
    /// Child image nodes retain their linked source and authored clip paths.
    pub(crate) fn copy_paint_layer_to_canvas(
        &mut self,
        host_id: NodeId,
        layer_id: slate_doc::image_paint::PaintLayerId,
        at: Pos2,
    ) -> Option<Vec<NodeId>> {
        if self.refuse_read_only_edit() {
            return None;
        }
        let host = self.doc().scene.node(host_id)?.clone();
        let NodeKind::Image(img) = &host.kind else {
            return None;
        };
        let result = self.image_result_id(host_id)?;
        let layer = super::SlateApp::selected_layers(img, &result)
            .iter()
            .find(|layer| layer.id == layer_id)?;
        if !layer.visible || layer.nodes.is_empty() {
            return None;
        }
        let opacity = layer.opacity;
        let dx = at.x - host.rect.x;
        let dy = at.y - host.rect.y;
        let mut nodes: Vec<Node> = layer
            .nodes
            .iter()
            .map(|child| {
                let mut world = slate_doc::image_paint::layer_node_to_world(&host, img, child);
                world.rect = world.rect.translated(dx, dy);
                world.opacity *= opacity;
                world
            })
            .collect();
        let group = self.doc_mut().scene.alloc_group_key();
        for node in &mut nodes {
            node.group = Some(group);
            let id = self
                .doc_mut()
                .scene
                .build_node(node.rect, node.kind.clone())
                .id;
            node.id = id;
            if let NodeKind::Image(img) = &mut node.kind {
                let rect = node.rect;
                slate_doc::image_paint::fresh_layer_node_ids(img, || {
                    self.doc_mut()
                        .scene
                        .build_node(
                            rect,
                            NodeKind::Image(ImageNode::new(slate_doc::ItemId::NONE)),
                        )
                        .id
                });
            }
        }
        let node_ids: Vec<NodeId> = nodes.iter().map(|node| node.id).collect();
        (!self.add_nodes(nodes).is_empty()).then_some(node_ids)
    }
}
