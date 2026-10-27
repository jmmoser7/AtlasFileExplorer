//! Extracted adapter on the existing owner.
use super::*;
use slate_doc::agent_inputs;

#[cfg(test)]
mod tests;

impl SlateApp {
    /// Resting objects retain their ordinary ports until generation is enabled.
    pub(crate) fn wire_host(
        &self,
        node: &slate_doc::scene::Node,
    ) -> slate_doc::wire_host::WireHost {
        slate_doc::wire_host::WireHost::from_node(node)
    }

    pub(crate) fn wire_input_host(&self, node: &Node) -> slate_doc::wire_host::WireHost {
        slate_doc::wire_host::WireHost::from_node_flow(node, self.wire_input_ports(node))
    }

    pub(crate) fn wire_input_ports(&self, node: &Node) -> &'static [agent_inputs::InputPort] {
        let ports = agent_inputs::input_ports_for(node, &|id| {
            self.doc().item(id).map(|item| item.path.as_path())
        });
        // Model View wires keep the ordinary edge snap, with no resting dot.
        if ports
            .iter()
            .any(|port| port.slot == atlas_agent::InputSlot::View)
        {
            &[]
        } else {
            ports
        }
    }

    fn wire_item_paths(
        &self,
        ends: &[&slate_doc::scene::ConnectorEnd],
    ) -> std::collections::HashMap<slate_doc::ItemId, std::path::PathBuf> {
        let mut paths = std::collections::HashMap::new();
        for end in ends {
            let Some(node_id) = slate_doc::agent_inputs::endpoint_node(end) else {
                continue;
            };
            let Some(node) = self.doc().scene.node(node_id) else {
                continue;
            };
            let NodeKind::Image(image) = &node.kind else {
                continue;
            };
            if let Some(item) = self.doc().item(image.item) {
                paths.insert(image.item, item.path.clone());
            }
        }
        paths
    }

    fn bind_wire(
        &self,
        a: &slate_doc::scene::ConnectorEnd,
        b: &slate_doc::scene::ConnectorEnd,
        pending: &[Node],
    ) -> Option<slate_doc::agent_inputs::WireBinding> {
        let ends = [a, b].map(slate_doc::agent_inputs::endpoint_node);
        if let Some(binding) = self.dashboard_control_wire(a, b) {
            return Some(binding);
        }
        if !pending.iter().any(|n| ends.contains(&Some(n.id))) {
            let paths = self.wire_item_paths(&[a, b]);
            return slate_doc::agent_inputs::infer_binding_with(&self.doc().scene, a, b, &|id| {
                paths.get(&id).map(std::path::PathBuf::as_path)
            });
        }
        // Inference reads only the two endpoint nodes, so a scene of those
        // two binds exactly as the committed scene will.
        let mut ends_only = Scene::default();
        ends_only.nodes = ends
            .iter()
            .flatten()
            .filter_map(|id| {
                pending
                    .iter()
                    .find(|n| n.id == *id)
                    .or_else(|| self.doc().scene.node(*id))
                    .cloned()
            })
            .collect();
        let mut paths = std::collections::HashMap::new();
        for node in &ends_only.nodes {
            if let NodeKind::Image(image) = &node.kind {
                if let Some(item) = self.doc().item(image.item) {
                    paths.insert(image.item, item.path.clone());
                }
            }
        }
        slate_doc::agent_inputs::infer_binding_with(&ends_only, a, b, &|id| {
            paths.get(&id).map(std::path::PathBuf::as_path)
        })
    }

    pub(super) fn rebind_wire(
        &self,
        old: &slate_doc::scene::ConnectorNode,
        new: &slate_doc::scene::ConnectorNode,
    ) -> Option<slate_doc::agent_inputs::WireBinding> {
        if let Some(binding) = self.dashboard_control_wire(&new.a, &new.b) {
            return Some(binding);
        }
        let paths = self.wire_item_paths(&[&new.a, &new.b]);
        slate_doc::agent_inputs::rebind_with(&self.doc().scene, old, new, &|id| {
            paths.get(&id).map(std::path::PathBuf::as_path)
        })
    }

    /// Journaled connector Add (unauthored stroke, no arrows).
    pub(crate) fn build_connector(&mut self, a: ConnectorEnd, b: ConnectorEnd) -> slate_doc::Node {
        self.build_connector_with(a, b, &[])
    }

    /// [`Self::build_connector`] for a batch: `pending` are nodes built for
    /// the same `add_nodes` call, so the wire binds and routes against them
    /// before they are committed.
    pub(crate) fn build_connector_with(
        &mut self,
        a: ConnectorEnd,
        b: ConnectorEnd,
        pending: &[Node],
    ) -> slate_doc::Node {
        self.build_connector_bound(a, b, pending, None)
    }

    /// An explicitly typed edge uses the same binding, styling and bounds owner.
    pub(crate) fn build_connector_bound(
        &mut self,
        a: ConnectorEnd,
        b: ConnectorEnd,
        pending: &[Node],
        binding: Option<agent_inputs::WireBinding>,
    ) -> slate_doc::Node {
        let node = self.connector_template_bound(a, b, pending, binding);
        self.doc_mut().scene.build_node(node.rect, node.kind)
    }

    /// Prepare the same typed, routed wire without reserving a document ID.
    pub(crate) fn connector_template_bound(
        &self,
        a: ConnectorEnd,
        b: ConnectorEnd,
        pending: &[Node],
        binding: Option<agent_inputs::WireBinding>,
    ) -> slate_doc::Node {
        let stroke = self.default_wire_stroke();
        let mut conn = ConnectorNode {
            crosstalk: None,
            routing: Some(self.board_wire_routing),
            binding: binding.or_else(|| self.bind_wire(&a, &b, pending)),
            a,
            b,
            stroke,
            color: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: WireDisplay::Default,
        };
        if let Some(binding) = &mut conn.binding {
            if binding.kind == slate_doc::agent_inputs::InputKind::Images {
                // TWIN: slate_doc::agent_inputs::WireBinding::source 2026-09-25
                let source = if binding.input_b { &conn.a } else { &conn.b };
                binding.output = slate_doc::agent_inputs::endpoint_node(source)
                    .and_then(|id| self.agent_active_output(id));
            }
        }
        // Wires into a chat train start in the train's calm gray instead of the
        // drawing color. It is only the default: a color picked later is kept.
        if let Some(binding) = &conn.binding {
            // TWIN: slate_doc::agent_inputs::WireBinding::target 2026-09-25
            let input = if binding.input_b { &conn.b } else { &conn.a };
            if binding.kind == agent_inputs::InputKind::Memory
                || slate_doc::agent_inputs::endpoint_node(input)
                    .is_some_and(|id| slate_doc::agent_inputs::is_chat_card(&self.doc().scene, id))
            {
                conn.set_color(Some(self.chat_wire_color()));
            }
        }
        let scene = &self.doc().scene;
        let obstacles = scene_wire_obstacles(scene);
        let rect = connector_aabb_routed(
            &conn,
            |id| {
                pending
                    .iter()
                    .find(|n| n.id == id)
                    .or_else(|| scene.node(id))
                    .map(|n| self.wire_host(n))
            },
            self.board_wire_routing,
            &obstacles,
            OrthoLane::default(),
        )
        .unwrap_or(WorldRect::new(0.0, 0.0, 1.0, 1.0));
        let mut scratch = scene.clone();
        scratch.build_node(rect, NodeKind::Connector(conn))
    }

    pub(crate) fn add_connector(&mut self, a: ConnectorEnd, b: ConnectorEnd) -> Option<NodeId> {
        let node = self.build_connector(a, b);
        let id = node.id;
        let mut commands = Vec::new();
        if let Some(command) = self.image_wire_target_command(&node) {
            commands.push(command);
        }
        commands.push(SceneCmd::Add {
            index: self.doc().scene.nodes.len(),
            node,
        });
        if !self.commit_scene(commands) {
            return None;
        }
        self.note_view_wires_added(&[id]);
        self.push_history(
            atlas_commands::CommandId("board.wire.add"),
            Some("connected".into()),
        );
        Some(id)
    }
}
