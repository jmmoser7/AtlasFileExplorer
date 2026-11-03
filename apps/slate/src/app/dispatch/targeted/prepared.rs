use super::*;

impl PreparedBoardCommand {
    pub(crate) fn image_history_endpoints(&self) -> &[String] {
        &self.history_endpoints
    }
    pub(crate) fn image_history_cut_target(&self) -> Option<NodeId> {
        self.history_cut
    }
    pub(crate) fn image_history_action(&self) -> Option<&(NodeId, String)> {
        self.history_action.as_ref()
    }
    pub(crate) fn artifact_pick_waiting(&self) -> bool {
        matches!(
            self.invocation,
            Some(invocation::DeferredInvocation::Artifact { waiting: true, .. })
        )
    }
    pub(crate) fn wait_for_artifact_pick(&mut self) {
        if let Some(invocation::DeferredInvocation::Artifact { waiting, .. }) =
            self.invocation.as_mut()
        {
            *waiting = true;
        }
    }
    pub(crate) fn pending_artifact_pick(
        &self,
    ) -> Option<(NodeId, String, Option<slate_doc::scene::HiddenOutputRef>)> {
        match self.invocation.as_ref()? {
            invocation::DeferredInvocation::Artifact {
                target,
                revision,
                remove,
                materialized: false,
                ..
            } => Some((*target, revision.clone(), remove.clone())),
            _ => None,
        }
    }
    pub(crate) fn resolve_artifact_pick(&mut self, command: SceneCmd) -> Result<(), String> {
        let Some(invocation::DeferredInvocation::Artifact { materialized, .. }) =
            self.invocation.as_mut()
        else {
            return Err("Missing saved artifact action".into());
        };
        *materialized = true;
        self.cmds = vec![command];
        Ok(())
    }
    pub(super) fn model_output_target(&self, app: &SlateApp) -> Option<NodeId> {
        let target = *self.targets.first()?;
        if self.command == "portal.agent.model_output" {
            return Some(target);
        }
        if !matches!(
            self.command.as_str(),
            "portal.agent.result.use" | "portal.agent.result.remove"
        ) || app.model_node_info(target).is_none()
        {
            return None;
        }
        self.cmds
            .iter()
            .any(|command| match command {
                SceneCmd::Patch { before, after } => match (&before.kind, &after.kind) {
                    (
                        slate_doc::scene::NodeKind::Image(old),
                        slate_doc::scene::NodeKind::Image(new),
                    ) => old.item != new.item,
                    _ => false,
                },
                _ => false,
            })
            .then_some(target)
    }
    pub(crate) fn smooth_gesture(&self) -> Option<&crate::app::board_smooth::SmoothGesture> {
        self.smooth.as_ref()
    }
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
        if self.cmds.iter().any(|cmd| {
            matches!(cmd, SceneCmd::Add { node, .. }
                if !self.expected.iter().any(|old| old.id == node.id)
                    && scene.node(node.id).is_some())
        }) {
            return Err(
                "A new object was added while this action was prepared; retry the command".into(),
            );
        }
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
