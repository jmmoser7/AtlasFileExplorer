use super::*;

impl PreparedBoardCommand {
    pub(crate) fn camera_zoom(&self) -> f32 {
        self.camera_zoom
    }
    pub(crate) fn vertex_drawing(&self, node: NodeId) -> bool {
        self.vertex_drawings.contains(&node)
    }
    pub(crate) fn erase_path(&self) -> &[[f64; 2]] {
        &self.erase_path
    }
    pub(crate) fn erase_span(&self) -> Option<slate_doc::scene::StrokeSpan> {
        self.erase_span
    }
    pub(crate) fn crop_gesture(&self) -> Option<crate::app::board_crop::CropGesture> {
        self.crop
    }

    pub(crate) fn created_ids(&self) -> Vec<NodeId> {
        self.cmds
            .iter()
            .filter_map(|cmd| match cmd {
                SceneCmd::Add { node, .. }
                    if !self.expected.iter().any(|old| old.id == node.id) =>
                {
                    Some(node.id)
                }
                _ => None,
            })
            .collect()
    }

    pub(crate) fn requires_linked_inputs(&self) -> bool {
        self.command == "board.objects.create_portal"
            || !self.linked_items.is_empty()
            || self
                .invocation
                .as_ref()
                .is_some_and(invocation::DeferredInvocation::requires_linked_inputs)
    }
    /// Explicit existing targets for commands whose effect is a provider job.
    pub(crate) fn witness_nodes(&self, scene: &Scene) -> Vec<Node> {
        if let Some(invocation) = &self.invocation {
            return scene
                .node(invocation.target())
                .cloned()
                .into_iter()
                .collect();
        }
        self.targets
            .iter()
            .filter_map(|id| scene.node(*id).cloned())
            .collect()
    }
    pub(crate) fn tab_id(&self) -> u64 {
        self.tab_id
    }
    pub(crate) fn workbook(&self) -> Option<&Path> {
        self.workbook.as_deref()
    }

    /// A preview never licenses applying stale before-states after human edits.
    pub(crate) fn applicable(&self, scene: &Scene) -> Result<(), String> {
        self.expected_matches(scene)?;
        let mut scratch = scene.clone();
        for cmd in &self.cmds {
            match cmd {
                SceneCmd::Patch { before, .. }
                    if scratch.node(before.id) != Some(before.as_ref()) =>
                {
                    return Err(format!("Object {} changed", before.id.0));
                }
                SceneCmd::Remove { node, .. } if scratch.node(node.id) != Some(node) => {
                    return Err(format!("Object {} changed", node.id.0));
                }
                _ => {}
            }
            if !scratch.apply(cmd) {
                return Err("The prepared command no longer applies to this board".into());
            }
        }
        Ok(())
    }

    /// Cheap conflict check for live previews; full preflight runs at commit.
    pub(crate) fn expected_matches(&self, scene: &Scene) -> Result<(), String> {
        for expected in &self.expected {
            if scene.node(expected.id) != Some(expected) {
                return Err(format!(
                    "Object {} changed while the action was prepared",
                    expected.id.0
                ));
            }
            if expected.locked
                && self
                    .invocation
                    .as_ref()
                    .is_none_or(|invocation| invocation.target() == expected.id)
            {
                return Err(format!("Object {} is locked", expected.id.0));
            }
        }
        if self.order.as_ref().is_some_and(|order| {
            !order
                .iter()
                .copied()
                .eq(scene.nodes.iter().map(|node| node.id))
        }) {
            return Err("The board order changed while the action was prepared".into());
        }
        Ok(())
    }
}
