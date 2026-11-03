//! Plain authored graph sources. Program spawning remains a separate command.
use super::*;
#[cfg(test)]
mod tests;

impl SlateApp {
    /// Typed graph wiring materializes only authored settings, never a program.
    pub(crate) fn graph_wire_settings_commands(
        &self,
        wire: &slate_doc::scene::Node,
    ) -> Vec<SceneCmd> {
        let NodeKind::Connector(conn) = &wire.kind else {
            return Vec::new();
        };
        let Some(binding) = conn
            .binding
            .as_ref()
            .filter(|b| matches!(b.slot.as_deref(), Some("media" | "prompt" | "style")))
        else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for id in [
            agent_inputs::endpoint_node(binding.source_end(conn)),
            agent_inputs::endpoint_node(binding.target_end(conn)),
        ]
        .into_iter()
        .flatten()
        {
            if !seen.insert(id) {
                continue;
            }
            let Some(before) = self
                .doc()
                .scene
                .node(id)
                .filter(|n| slate_doc::agent_chat::agent(n).is_none())
            else {
                continue;
            };
            if let Ok(after) = self.graph_settings_node(id) {
                commands.push(SceneCmd::Patch {
                    before: Box::new(before.clone()),
                    after: Box::new(after),
                });
            }
        }
        commands
    }
    pub(crate) fn graph_settings_node(&self, id: NodeId) -> Result<slate_doc::scene::Node, String> {
        let mut after = self
            .doc()
            .scene
            .node(id)
            .ok_or("The source is missing")?
            .clone();
        if self.tab().read_only || after.locked {
            return Err("Choose an unlocked source in an editable workbook".into());
        }
        if slate_doc::agent_chat::agent(&after).is_none() {
            let view = match self.generator_source_kind(id) {
                Some(PendingHost::Image) => PortalView::Images,
                Some(PendingHost::Text) => PortalView::Text,
                Some(PendingHost::Model | PendingHost::Artifact) => PortalView::Chat,
                _ => return Err("This source has no generator graph settings".into()),
            };
            let settings = Some(Box::new(slate_doc::scene::AgentPortalRef::unbound(view)));
            match &mut after.kind {
                NodeKind::Text(text) => text.agent = settings,
                NodeKind::Image(image) => image.agent = settings,
                NodeKind::Portal(portal) => portal.agent = settings,
                _ => return Err("This source has no generator graph settings".into()),
            }
        }
        Ok(after)
    }
    /// Both human menus and targeted commands prepare this same journal step.
    pub(crate) fn prepare_graph_input_source(
        &self,
        kind: &str,
        at: [f32; 2],
    ) -> Result<Vec<SceneCmd>, String> {
        if self.tab().read_only {
            return Err("This workbook is read-only".into());
        }
        if at.iter().any(|v| !v.is_finite() || v.abs() > 1.0e9) {
            return Err("Input position must be finite board coordinates".into());
        }
        let (width, height) = match kind {
            "image" | "style" => (220.0, 160.0),
            "text" => (260.0, 130.0),
            _ => return Err("Choose image, text or style input".into()),
        };
        let rect = slate_doc::WorldRect::new(at[0], at[1], width, height);
        let mut node = if kind == "text" {
            // Text is an authored native recipe, not a kit-backed BoardTool.
            // Reuse the same prepared creation owner as the registered command.
            let prepared = self.prepare_targeted_command(
                "board.objects.create_text", &[],
                &serde_json::json!({"x":rect.x,"y":rect.y,"width":rect.w,"height":rect.h,"text":""}),
            )?;
            if prepared.cmds.len() != 1 {
                return Err("The plain Text tool must create one source".into());
            }
            let Some(SceneCmd::Add { mut node, .. }) = prepared.cmds.into_iter().next() else {
                return Err("The plain Text tool must create one source".into());
            };
            let NodeKind::Text(text) = &mut node.kind else {
                return Err("The plain Text tool does not create a text source".into());
            };
            text.text.clear();
            text.agent = None;
            node
        } else {
            let mut scratch = self.doc().scene.clone();
            scratch.build_node(
                rect,
                NodeKind::Image(slate_doc::scene::ImageNode::new(slate_doc::ItemId::NONE)),
            )
        };
        let mut metadata = slate_doc::scene::AgentPortalRef::unbound(if kind == "text" {
            PortalView::Text
        } else {
            PortalView::Images
        });
        metadata.generator.preserve_history = Some(false);
        metadata.generator.input_source_kind = Some(match kind {
            "text" => slate_doc::scene::GraphInputKind::Text,
            "style" => slate_doc::scene::GraphInputKind::Style,
            _ => slate_doc::scene::GraphInputKind::Image,
        });
        match &mut node.kind {
            NodeKind::Text(text) => text.agent = Some(Box::new(metadata)),
            NodeKind::Image(image) => image.agent = Some(Box::new(metadata)),
            _ => unreachable!(),
        }
        Ok(vec![SceneCmd::Add {
            index: self.doc().scene.nodes.len(),
            node,
        }])
    }

    pub(crate) fn add_graph_input_source(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            kind: String,
            at: [f32; 2],
        }
        let Some(input) = detail.and_then(|s| serde_json::from_str::<Input>(s).ok()) else {
            return false;
        };
        match self.prepare_graph_input_source(&input.kind, input.at) {
            Ok(commands) => self.commit_scene(commands),
            Err(error) => {
                self.toast(error);
                false
            }
        }
    }
}
