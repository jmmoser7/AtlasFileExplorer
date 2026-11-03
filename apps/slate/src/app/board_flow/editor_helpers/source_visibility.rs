//! Per-source presentation of canonical connected slots.
use super::*;

impl SlateApp {
    /// Agent expansion belongs to this source in this workbook, never a mode
    /// shared by the other native media on the board.
    pub(crate) fn source_agent_dialogue_open(&self, id: NodeId) -> bool {
        self.generator_history_open(id)
    }

    fn source_agent_uses_full_inputs(&self, id: NodeId) -> bool {
        self.source_agent_dialogue_open(id) || self.doc().scene.node(id).is_some_and(|node| {
            matches!(&node.kind, NodeKind::Portal(portal) if portal.kind == slate_doc::scene::PortalKind::Agent)
        })
    }

    fn refresh_connected_input_slots(&self) {
        let generation = (self.tab().id, self.scene_gen, self.doc().scene.scene_gen());
        let mut cached = self.agents.flow.connected_inputs.borrow_mut();
        if cached
            .as_ref()
            .is_some_and(|(previous, _)| *previous == generation)
        {
            return;
        }
        let mut targets =
            HashMap::<NodeId, std::collections::HashSet<atlas_agent::InputSlot>>::new();
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            let Some(slot) = binding
                .slot
                .as_deref()
                .and_then(atlas_agent::InputSlot::from_id)
            else {
                continue;
            };
            if let Some(target) = agent_inputs::endpoint_node(binding.target_end(conn)) {
                targets.entry(target).or_default().insert(slot);
            }
        }
        *cached = Some((generation, targets));
    }

    /// Closing Agent retains actual connected slots without promoting the
    /// source into a generator or revealing its unused input roles.
    pub(crate) fn source_agent_input_visible(
        &self,
        id: NodeId,
        slot: atlas_agent::InputSlot,
    ) -> bool {
        if self.source_agent_uses_full_inputs(id) {
            return true;
        }
        self.refresh_connected_input_slots();
        self.agents
            .flow
            .connected_inputs
            .borrow()
            .as_ref()
            .is_some_and(|(_, targets)| targets.get(&id).is_some_and(|slots| slots.contains(&slot)))
    }

    pub(crate) fn source_agent_inputs_visible(&self, id: NodeId) -> bool {
        if self.source_agent_uses_full_inputs(id) {
            return true;
        }
        self.refresh_connected_input_slots();
        self.agents
            .flow
            .connected_inputs
            .borrow()
            .as_ref()
            .is_some_and(|(_, targets)| targets.contains_key(&id))
    }
}
