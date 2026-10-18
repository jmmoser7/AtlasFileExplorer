//! Accept and reject staged proposals.

use super::*;

impl SlateApp {
    pub(crate) fn pending_stage_proposals(&self) -> &[Proposal] {
        &self.agents.pending
    }

    pub(crate) fn live_presence_proposal(&self, id: &str) -> Option<&Proposal> {
        self.agents.presence_proposals.get(id)
    }

    pub(crate) fn mark_stage_proposal_recovery(&mut self, id: &str) {
        if let Some(pending) = self.agents.pending.iter_mut().find(|p| p.id == id) {
            pending.status = slate_doc::ProposalStatus::RecoveryRequired;
        }
    }

    pub(crate) fn discard_live_presence_proposal(&mut self, id: &str) {
        self.presence_end_proposal_motion(id, true);
        self.agents.presence_proposals.remove(id);
        self.agents.remove_pending(id);
    }

    pub(crate) fn queue_stage_proposal(&mut self, proposal: Proposal) -> Result<(), String> {
        if self.agents.pending.len() >= 128 {
            return Err("Review pending proposals before requesting more".into());
        }
        if self
            .agents
            .pending
            .iter()
            .any(|pending| pending.id == proposal.id)
        {
            return Err("This proposal is already pending".into());
        }
        if proposal.id.starts_with("presence-") {
            proposal
                .session
                .as_deref()
                .filter(|session| !session.is_empty())
                .ok_or("A presence proposal requires its live session")?;
            self.agents
                .presence_proposals
                .insert(proposal.id.clone(), proposal.clone());
        }
        self.agents.pending.push(proposal);
        Ok(())
    }

    /// A revocation fences acceptance immediately, independently of disk health.
    /// Presence proposals are ephemeral; a retired session cannot regain them.
    pub(crate) fn cancel_presence_proposals(&mut self, session: &str) {
        let ids: Vec<_> = self.presence_proposal_ids_for_session(session);
        for id in ids {
            self.presence_end_proposal_motion(&id, true);
        }
        self.agents
            .presence_proposals
            .retain(|_, proposal| proposal.session.as_deref() != Some(session));
        self.agents.pending.retain(|proposal| {
            !(proposal.id.starts_with("presence-") && proposal.session.as_deref() == Some(session))
        });
    }

    pub(crate) fn accept_stage_proposal(&mut self, proposal_id: &str) {
        let Some(proposal) = self
            .agents
            .pending
            .iter()
            .find(|p| p.id == proposal_id)
            .cloned()
        else {
            return;
        };
        self.accept_stage_proposal_data(&proposal);
    }

    /// Both file-backed portal proposals and in-memory collaborator proposals
    /// use the same replay barrier, applicability checks, and recovery state.
    pub(crate) fn accept_stage_proposal_data(&mut self, proposal: &Proposal) {
        let live_presence = proposal.session.is_some()
            && self.agents.presence_proposals.get(&proposal.id) == Some(proposal);
        if proposal.id.starts_with("presence-") && !live_presence {
            self.discard_live_presence_proposal(&proposal.id);
            self.toast("This collaborator proposal was cancelled or belongs to an ended session.");
            return;
        }
        if live_presence {
            if let Err(error) = self.start_presence_proposal(proposal) {
                self.toast(format!("Proposal not applied: {error}."));
            }
            return;
        }
        self.commit_accepted_stage_proposal(proposal);
    }

    /// One canonical stage/journal owner, reached after an approved Presence
    /// witness completes or immediately for ordinary file-backed proposals.
    pub(crate) fn commit_accepted_stage_proposal(&mut self, proposal: &Proposal) {
        if proposal.id.starts_with("presence-") {
            if let Err(error) = self.presence_validate_completed_proposal(proposal) {
                self.toast(format!("Proposal not applied: {error}."));
                return;
            }
        }
        if self.refuse_read_only_edit() {
            return;
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Could not accept the proposal: the AI workspace is unavailable.");
            return;
        };
        let outcome = {
            let tab = self.tab_mut();
            stage::accept_and_record(
                proposal,
                tab.path.as_deref(),
                &mut tab.doc.scene,
                &mut tab.journal,
                &ws,
            )
        };
        match outcome {
            Ok(result) => {
                self.agents.presence_proposals.remove(&proposal.id);
                self.agents.remove_pending(&proposal.id);
                if result == ProposalResult::Accepted {
                    self.note_accepted_stage_proposal(proposal);
                } else {
                    self.presence_end_proposal_motion(&proposal.id, true);
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
                    self.note_accepted_stage_proposal(proposal);
                }
                match stage::read_result(&ws, &proposal.id) {
                    Ok(Some(ProposalResult::Applying)) | Err(_) => {
                        self.mark_stage_proposal_recovery(&proposal.id);
                    }
                    Ok(Some(_)) => {
                        self.agents.presence_proposals.remove(&proposal.id);
                        self.agents.remove_pending(&proposal.id);
                        self.presence_end_proposal_motion(&proposal.id, true);
                    }
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

    fn note_accepted_stage_proposal(&mut self, proposal: &Proposal) {
        self.presence_accepted_proposal(proposal);
        self.agents.presence_proposals.remove(&proposal.id);
        self.finish_targeted_proposal_bookkeeping(&proposal.cmds);
        let tab = self.tab_mut();
        tab.dirty = true;
        tab.edits.push(super::super::board::BoardMark::Scene);
        tab.edit_redo.clear();
        self.paint_layer_texture_cache.clear();
        self.note_scene_change();
        if let Some(command) = super::super::commands::SPECS
            .iter()
            .find(|spec| spec.id.0 == proposal.title)
            .map(|spec| spec.id)
        {
            self.push_history_as(
                command,
                Some(format!("accepted {}", proposal.id)),
                atlas_commands::CmdAuthor::Agent(proposal.author.clone()),
            );
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
        // A failed durable rejection must not leave a live mutation permit.
        let presence = proposal.id.starts_with("presence-");
        if presence {
            self.discard_live_presence_proposal(&proposal.id);
        }
        let Some(ws) = self.ai.config.valid_workspace() else {
            self.toast(if presence {
                "Collaborator proposal cancelled. Its rejection could not be saved: the AI workspace is unavailable."
            } else { "Could not save the rejection: the AI workspace is unavailable." });
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
                let state = if presence {
                    "Collaborator proposal remains cancelled"
                } else {
                    "Rejection was not saved"
                };
                self.toast(format!(
                    "Could not read the proposal decision: {e}. {state}."
                ));
                return;
            }
        }
        if let Err(e) = stage::write_result(ws, &proposal.id, &reject(&proposal)) {
            let state = if presence {
                "The collaborator proposal remains cancelled"
            } else {
                "The proposal remains available"
            };
            self.toast(format!("Could not save the rejection: {e}. {state}."));
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
