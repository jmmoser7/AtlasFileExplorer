//! Existing endpoint-grant fence shared by text choice and result mutations.
use super::*;

impl SlateApp {
    pub(in crate::app::agent_presence) fn validate_prepared_image_history_scope(
        &self,
        grant: &CollaboratorGrant,
        call: &BoardCall,
        prepared: &PreparedBoardCommand,
    ) -> Result<(), String> {
        if prepared
            .image_history_endpoints()
            .iter()
            .any(|endpoint| !grant.conversation_ids.contains(endpoint))
        {
            return Err("An admitted Images history endpoint grant was revoked".into());
        }
        if let BoardOperation::Invoke {
            command,
            targets,
            arguments,
        } = &call.operation
        {
            if command == "board.image.history" {
                self.validate_image_history_scope(grant, targets, arguments)?;
            }
        }
        Ok(())
    }
    pub(crate) fn presence_image_history_cut_target(
        &self,
        proposal: &slate_doc::Proposal,
    ) -> Option<slate_doc::NodeId> {
        self.presence
            .proposals
            .get(&proposal.id)?
            .prepared
            .image_history_cut_target()
    }
    pub(crate) fn finish_presence_image_history(&mut self, proposal: &slate_doc::Proposal) {
        let Some(prepared) = self
            .presence
            .proposals
            .get(&proposal.id)
            .map(|p| &p.prepared)
        else {
            return;
        };
        let cut = prepared.image_history_cut_target();
        let action = prepared.image_history_action().cloned();
        if let Some(id) = cut {
            self.finish_generator_history_cut(id);
        }
        self.finish_image_history_action(action.as_ref());
    }
    pub(in crate::app::agent_presence) fn validate_image_history_scope(
        &self,
        grant: &CollaboratorGrant,
        targets: &[String],
        args: &serde_json::Value,
    ) -> Result<(), String> {
        if targets.len() != 1 {
            return Err("Choose one explicit Images history target".into());
        }
        let id = targets[0]
            .parse::<u64>()
            .map(slate_doc::NodeId)
            .map_err(|_| "Use a stable Image ID")?;
        let action = args["action"]
            .as_str()
            .ok_or("Choose an Images history action")?;
        let dependencies = self.image_history_dependencies(id, action)?;
        for dependency in dependencies {
            if !grant.allows_node(&dependency.0.to_string()) {
                return Err("Images history dependency exceeds the board grant".into());
            }
        }
        if self
            .image_history_required_endpoints(id, action)?
            .iter()
            .any(|endpoint| !grant.conversation_ids.contains(endpoint))
        {
            return Err("Grant the Images history conversation endpoint before reading or changing its context".into());
        }
        Ok(())
    }
    /// Enforce the existing endpoint grant before inspecting checkpoints and
    /// again during witnessed/proposal precommit revalidation.
    pub(in crate::app::agent_presence) fn validate_text_output_scope(
        &self,
        grant: &CollaboratorGrant,
        command: &str,
        targets: &[String],
    ) -> Result<(), String> {
        let result_text = matches!(
            command,
            "portal.agent.result.use" | "portal.agent.result.remove"
        ) && targets.iter().any(|target| {
            target.parse::<u64>().ok().is_some_and(|id| {
                self.generator_source_kind(slate_doc::NodeId(id))
                    == Some(crate::app::board_flow::PendingHost::Text)
            })
        });
        if (command == "portal.agent.text_output" || result_text)
            && targets.iter().any(|target| {
                target
                    .parse::<u64>()
                    .ok()
                    .and_then(|id| self.doc().scene.node(slate_doc::NodeId(id)))
                    .and_then(slate_doc::agent_chat::agent)
                    .is_none_or(|agent| !grant.conversation_ids.contains(&agent.session))
            })
        {
            return Err("Grant this generator's conversation endpoint before selecting a saved text response".into());
        }
        Ok(())
    }
}
