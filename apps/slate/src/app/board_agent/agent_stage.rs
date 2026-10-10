//! Accept and reject staged proposals.

use super::*;

impl SlateApp {
    pub(crate) fn accept_stage_proposal(&mut self, proposal_id: &str) {
        if self.refuse_read_only_edit() {
            return;
        }
        let Some(proposal) = self
            .agents
            .pending
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
        else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Could not accept the proposal: the AI workspace is unavailable.");
            return;
        };
        let outcome = {
            let tab = self.tab_mut();
            stage::accept_and_record(
                &proposal,
                tab.path.as_deref(),
                &mut tab.doc.scene,
                &mut tab.journal,
                &ws,
            )
        };
        match outcome {
            Ok(result) => {
                self.agents.remove_pending(&proposal.id);
                if result == ProposalResult::Accepted {
                    self.tab_mut().dirty = true;
                    self.note_scene_change();
                }
                match result {
                    ProposalResult::Accepted => self.toast("Accepted agent proposal."),
                    ProposalResult::Stale { reason } => {
                        self.toast(format!("Proposal not applied: {reason}."))
                    }
                    ProposalResult::Applying | ProposalResult::Rejected => unreachable!(),
                }
            }
            Err(failure) => {
                if failure.applied {
                    self.tab_mut().dirty = true;
                    self.note_scene_change();
                }
                match stage::read_result(&ws, &proposal.id) {
                    Ok(Some(ProposalResult::Applying)) | Err(_) => {
                        if let Some(p) =
                            self.agents.pending.iter_mut().find(|p| p.id == proposal.id)
                        {
                            p.status = slate_doc::ProposalStatus::RecoveryRequired;
                        }
                    }
                    Ok(Some(_)) => self.agents.remove_pending(&proposal.id),
                    Ok(None) => {}
                }
                let state = if failure.applied {
                    "Proposal applied, but its confirmation could not be saved. Review the board before dismissing it"
                } else {
                    "Proposal not applied"
                };
                self.toast(format!("{state}: {}.", failure.error));
            }
        }
    }
    pub(crate) fn reject_stage_proposal(&mut self, proposal_id: &str) {
        let Some(proposal) = self
            .agents
            .pending
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
        else {
            return;
        };
        let Some(ws) = self.ai.config.valid_workspace() else {
            self.toast("Could not save the rejection: the AI workspace is unavailable.");
            return;
        };
        match stage::read_result(ws, &proposal.id) {
            Ok(Some(
                ProposalResult::Accepted | ProposalResult::Rejected | ProposalResult::Stale { .. },
            )) => {
                self.agents.remove_pending(&proposal.id);
                self.toast("This proposal already has a recorded decision. No board changes made.");
                return;
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {} // Human explicitly dismisses an unreadable decision.
            Err(e) => {
                self.toast(format!(
                    "Could not read the proposal decision: {e}. Rejection was not saved."
                ));
                return;
            }
        }
        if let Err(e) = stage::write_result(ws, &proposal.id, &reject(&proposal)) {
            self.toast(format!(
                "Could not save the rejection: {e}. The proposal remains available."
            ));
            return;
        }
        self.agents.remove_pending(&proposal.id);
        if proposal.status == slate_doc::ProposalStatus::RecoveryRequired {
            self.toast("Dismissed the recovery proposal. Previously applied board changes are unchanged; use Undo if needed.");
        } else {
            self.toast("Rejected agent proposal.");
        }
    }
    pub(crate) fn accept_selected_stage_proposal(&mut self) -> bool {
        let Some(session) = self
            .selected_agent_portal()
            .and_then(|id| self.agent_session_for(id).map(|(session, _)| session))
        else {
            self.toast("Select an agent portal first.");
            return false;
        };
        let Some(id) = self
            .agents
            .pending_for(&session)
            .next()
            .map(|p| p.id.clone())
        else {
            self.toast("No pending agent proposal for this portal.");
            return false;
        };
        self.accept_stage_proposal(&id);
        true
    }
    pub(crate) fn reject_selected_stage_proposal(&mut self) -> bool {
        let Some(session) = self
            .selected_agent_portal()
            .and_then(|id| self.agent_session_for(id).map(|(session, _)| session))
        else {
            self.toast("Select an agent portal first.");
            return false;
        };
        let Some(id) = self
            .agents
            .pending_for(&session)
            .next()
            .map(|p| p.id.clone())
        else {
            self.toast("No pending agent proposal for this portal.");
            return false;
        };
        self.reject_stage_proposal(&id);
        true
    }
    pub(crate) fn agent_session_for(&self, portal: NodeId) -> Option<(String, String)> {
        let agent = slate_doc::agent_chat::agent(self.doc().scene.node(portal)?)?;
        Some((agent.session.clone(), agent.provider.clone()))
    }
    /// The selected agent card: a chat, or media an agent makes.
    pub(crate) fn selected_agent_portal(&self) -> Option<NodeId> {
        self.board_sel.iter().copied().find(|id| {
            self.doc()
                .scene
                .node(*id)
                .is_some_and(slate_doc::agent_chat::is_agent_node)
        })
    }
    pub(super) fn agent_portal_unbound(&self, id: NodeId) -> bool {
        self.doc().scene.node(id).is_some_and(|n| {
            matches!(&n.kind, NodeKind::Portal(p) if p.kind == PortalKind::Agent && p.source.is_none())
        })
    }
}
