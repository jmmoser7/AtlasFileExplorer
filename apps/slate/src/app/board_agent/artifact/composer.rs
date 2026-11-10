//! Structural guards keep accepted work independent of the next editable draft.
use super::*;

impl SlateApp {
    /// A valid parked request keeps its file scope. Tab focus, selection and
    /// camera are not task authority; publication retains the source guard.
    pub(super) fn subscription_artifact_target_current(&self, plan: &Plan) -> bool {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == plan.tab) else {
            return false;
        };
        let Some(node) = tab.doc.scene.node(plan.node) else {
            return false;
        };
        let Some(agent) = slate_doc::agent_chat::agent(node) else {
            return false;
        };
        !tab.read_only
            && !node.locked
            && artifact(node).is_some_and(|binding| binding.revision == plan.previous)
            && agent.provider == plan.provider
            && agent.session == plan.session
            && agent.model == plan.request.model
            && agent.model_parameters == plan.request.model_parameters
            && self
                .agent_runtime_in(plan.tab)
                .and_then(|runtime| runtime.request_of(plan.node))
                == Some(plan.request.id.as_str())
    }

    /// An admitted batch owns its captured question; the live field is the next turn.
    pub(super) fn artifact_plan_guard(&self, plan: &Plan) -> Option<String> {
        if plan.accepted_batch {
            self.artifact_batch_guard_with_prompt(plan.node, Some(&plan.authored_prompt))
        } else {
            self.artifact_batch_guard(plan.node)
        }
    }

    /// Only a validated successful publication rebases expected source/revision.
    pub(super) fn rebase_artifact_siblings(&mut self, id: NodeId) {
        let Some(mut queue) = self.agents.artifact.queued.remove(&id) else {
            return;
        };
        let revision = self
            .doc()
            .scene
            .node(id)
            .and_then(artifact)
            .and_then(|a| a.revision.clone());
        for (plan, _) in &mut queue {
            plan.previous = revision.clone();
            plan.guard = self.artifact_plan_guard(plan).unwrap_or_default();
        }
        self.agents.artifact.queued.insert(id, queue);
    }

    pub(super) fn artifact_batch_guard(&self, id: NodeId) -> Option<String> {
        // Before admission the live draft is a preflight witness. After admission
        // the captured question owns this run; the composer belongs to the next.
        let submitted = self
            .agents
            .artifact
            .running
            .get(&id)
            .map(|running| running.prepared.plan.authored_prompt.as_str());
        self.artifact_batch_guard_with_prompt(id, submitted)
    }

    pub(super) fn artifact_batch_guard_with_prompt(
        &self,
        id: NodeId,
        submitted: Option<&str>,
    ) -> Option<String> {
        let node = self.doc().scene.node(id)?;
        let agent = slate_doc::agent_chat::agent(node)?;
        let source = match &node.kind {
            NodeKind::Portal(portal) => &portal.source,
            _ => return None,
        };
        let inputs: Vec<_> =
            self.doc()
                .scene
                .nodes
                .iter()
                .filter_map(|node| {
                    let NodeKind::Connector(conn) = &node.kind else {
                        return None;
                    };
                    let binding = conn.binding.as_ref()?;
                    (slate_doc::agent_inputs::endpoint_node(binding.target_end(conn)) == Some(id))
                        .then(|| {
                            (
                                node.id,
                                (
                                    binding.kind,
                                    &binding.output,
                                    &binding.order,
                                    binding.all_images,
                                    &binding.slot,
                                ),
                                slate_doc::agent_inputs::endpoint_node(binding.source_end(conn)),
                            )
                        })
                })
                .collect();
        let prompt = submitted
            .unwrap_or_else(|| self.agents.prompt(id).unwrap_or(&agent.instruction))
            .trim();
        let captured = self.agent_input_snapshot(id).ok()?;
        let values: Vec<_> = captured
            .wired
            .iter()
            .map(|item| (item.node, &item.text, &item.images, &item.depth, item.slot))
            .collect();
        serde_json::to_string(&(
            prompt,
            inputs,
            values,
            source,
            &artifact(node)?.revision,
            &agent.provider,
            &agent.model,
            &agent.model_parameters,
            (
                agent.generator.count,
                agent.generator.preserve_history,
                &agent.generator.grafted_slots,
                agent.generator.history_cut,
            ),
            node.locked,
        ))
        .ok()
    }
}
