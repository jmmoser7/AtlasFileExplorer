use super::*;
use slate_doc::agent_inputs::{ArtifactMemoryPin, InputKind, WireBinding};
use slate_doc::scene::{ConnectorEnd, Side};

impl SlateApp {
    pub(crate) fn build_artifact_memory_wire(
        &mut self,
        source: NodeId,
        target: &Node,
    ) -> Option<Node> {
        let a = self.doc().scene.node(source).and_then(artifact)?;
        let pin = ArtifactMemoryPin {
            revision: a.revision.clone()?,
            through: a.checkpoint,
        };
        let binding = WireBinding {
            input_b: true,
            kind: InputKind::Memory,
            output: Some(serde_json::to_string(&pin).ok()?),
            order: vec![],
            all_images: false,
            consumed: false,
            slot: Some("memory".into()),
        };
        let mut wire = self.build_connector_bound(
            ConnectorEnd::Anchored {
                node: source,
                side: Side::Top,
                t: 1.0,
            },
            ConnectorEnd::Anchored {
                node: target.id,
                side: Side::Top,
                t: 0.0,
            },
            std::slice::from_ref(target),
            Some(binding),
        );
        if let NodeKind::Connector(conn) = &mut wire.kind {
            conn.label = Some("Memory".into());
        }
        Some(wire)
    }
    pub(crate) fn artifact_disconnect_memory(&mut self, id: NodeId) -> bool {
        if self.refuse_read_only_edit() {
            return false;
        }
        let cmds = self
            .doc()
            .scene
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let NodeKind::Connector(conn) = &node.kind else {
                    return None;
                };
                let binding = conn.binding.as_ref()?;
                (binding.kind == InputKind::Memory
                    && slate_doc::agent_inputs::endpoint_node(binding.target_end(conn)) == Some(id))
                .then(|| SceneCmd::Remove {
                    index,
                    node: node.clone(),
                })
            })
            .collect::<Vec<_>>();
        if cmds.is_empty() {
            self.toast("No inherited Memory is connected. The next Play already starts fresh.");
            return false;
        }
        let changed = self.commit_scene(cmds);
        if changed {
            self.agents.artifact.inspect.remove(&id);
            self.toast("Memory disconnected. The next Play starts a fresh conversation.");
        }
        changed
    }
    pub(crate) fn artifact_fork(&mut self, id: NodeId) -> bool {
        if !self
            .doc()
            .scene
            .node(id)
            .and_then(artifact)
            .is_some_and(|a| a.revision.is_some())
        {
            self.toast("Apply a completed artifact revision before creating a fork.");
            return false;
        }
        let forked = self.spawn_flow_node(Some(
            &serde_json::json!({"source":id.0,"kind":"artifact"}).to_string(),
        ));
        if forked {
            self.toast("Artifact fork created with the selected revision and Memory.");
        }
        forked
    }
}
