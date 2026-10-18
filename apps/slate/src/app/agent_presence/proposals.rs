//! Ephemeral execution context; the immutable stage proposal is the authority.
use super::witness::{WitnessPurpose, WitnessedCommand};
use super::*;
use crate::app::dispatch::PreparedBoardCommand;
use serde_json::json;
use slate_doc::{scene::Node, Proposal, ProposalStatus};

#[cfg(test)]
thread_local! {
    pub(super) static FULL_PROPOSAL_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) struct ProposalContext {
    pub(super) participant: String,
    pub(super) tab: u64,
    pub(super) call: BoardCall,
    pub(super) prepared: PreparedBoardCommand,
    pub(super) previews: HashMap<slate_doc::NodeId, Node>,
    pub(super) approval: Option<Approval>,
}
pub(super) struct Approval {
    epoch: u64,
    listening: bool,
}
impl PresenceRuntime {
    pub(super) fn has_pending(&self, who: &str) -> bool {
        self.proposals.values().any(|p| p.participant == who)
    }
    pub(super) fn has_pending_for(&self, a: &Collaborator) -> bool {
        self.proposals.values().any(|p| {
            p.participant == a.profile.id
                && p.call.session == a.binding
                && p.call.request_id == a.request
        })
    }
    /// A listening agent may execute only the exact action explicitly accepted.
    pub(super) fn approved_while_listening(&self, a: &Collaborator) -> bool {
        a.work.as_ref().is_some_and(|work| match &work.purpose {
            WitnessPurpose::ApprovedProposal(id) => self.proposals.get(id).is_some_and(|p| {
                p.approval
                    .as_ref()
                    .is_some_and(|token| token.listening && token.epoch == a.control_epoch)
            }),
            _ => false,
        })
    }
}
impl SlateApp {
    pub(crate) fn presence_proposal_ids_for_session(&self, session: &str) -> Vec<String> {
        self.presence
            .proposals
            .iter()
            .filter(|(_, p)| p.call.session.session_id == session)
            .map(|(id, _)| id.clone())
            .collect()
    }
    pub(super) fn finish_presence_work(
        &mut self,
        a: &mut Collaborator,
        work: WitnessedCommand,
        now: Instant,
    ) {
        if matches!(work.action, witness::WitnessAction::Relay(_)) {
            self.finish_contacted_presence_relay(a, &work, now);
            return;
        }
        let prepared = work.board_prepared().expect("board action variant").clone();
        let created = prepared.created_ids();
        let generator_target = (prepared.command == "portal.agent.run")
            .then(|| {
                prepared
                    .witness_nodes(&self.doc().scene)
                    .first()
                    .map(|n| n.id)
            })
            .flatten();
        let autonomy = work.purpose == WitnessPurpose::Autonomous;
        let outcome = self.validate_presence_work(a, &work).and_then(|()| {
            if !autonomy && self.presence.proposals.len() >= 128 {
                return Err("Review pending proposals before requesting more".into());
            }
            self.finish_targeted_command(
                prepared.clone(),
                &a.profile.name,
                autonomy,
                &a.binding.session_id,
            )
        });
        if let Ok(status) = &outcome {
            if let Some(id) = status.strip_prefix("staged:") {
                self.presence.proposals.insert(
                    id.into(),
                    ProposalContext {
                        participant: a.profile.id.clone(),
                        tab: a.tab,
                        call: work.call.clone(),
                        prepared,
                        previews: work.previews,
                        approval: None,
                    },
                );
            } else if !created.is_empty() {
                if let Err(error) = self.presence_register_created_chats(a, &created) {
                    a.status = error;
                }
            }
            if let (Some(target), Some(request)) =
                (generator_target, status.strip_prefix("queued:"))
            {
                a.launched.push((target, request.into()));
            }
        }
        let reply = match outcome {
            Ok(status) => {
                a.status = if autonomy {
                    "Action completed"
                } else {
                    "Proposal ready · accept or reject in chat"
                }
                .into();
                a.activity = if autonomy { "success" } else { "approval" }.into();
                BoardReply {
                    id: work.call.id.clone(),
                    session_id: a.binding.session_id.clone(),
                    request_id: a.request.clone(),
                    success: true,
                    result: json!({"status":status,"created_ids":created.iter()
                        .map(|id|id.0.to_string()).collect::<Vec<_>>()}),
                }
            }
            Err(error) => {
                a.note_helper_outcome("blocked", &error);
                a.status = error.clone();
                a.activity = "blocked".into();
                BoardReply::error(&work.call, &error)
            }
        };
        let _ = self.presence.links.reply(&a.dir, reply);
        a.last_active = now;
    }

    pub(crate) fn start_presence_proposal(&mut self, proposal: &Proposal) -> Result<(), String> {
        let context = self
            .presence
            .proposals
            .get(&proposal.id)
            .ok_or("This proposal has no live prepared action; request a fresh proposal")?;
        let who = context.participant.clone();
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .ok_or("Collaborator is no longer present")?;
        self.validate_presence_proposal(proposal, a)?;
        if context.approval.is_some()
            && a.work
                .as_ref()
                .is_some_and(|w| w.purpose == WitnessPurpose::ApprovedProposal(proposal.id.clone()))
        {
            return Ok(());
        }
        if a.work.is_some()
            || a.portal.is_some()
            || (a.read.is_some() && !a.paused)
            || !a.launched.is_empty()
        {
            return Err(
                "Finish or stop the current action before accepting another proposal".into(),
            );
        }
        let prepared = context.prepared.clone();
        let call = context.call.clone();
        let drawing_helpers = a.grant.as_ref().map_or(0, |grant| grant.helper_limit);
        self.check_presence_proposal_decision(proposal)?;
        self.check_targeted_command(&prepared)?;
        let mut work =
            WitnessedCommand::new(call, prepared, &self.doc().scene, self.board_wire_routing)?;
        work.purpose = WitnessPurpose::ApprovedProposal(proposal.id.clone());
        work.set_drawing_helpers(drawing_helpers);
        self.presence_forget_avatar_hold(&who);
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who)
            .unwrap();
        // A paused read can remain parked, but its old cursor travel must
        // not become the approved command's first motion sample.
        a.cancel_motion(&mut self.presence.roster)?;
        self.presence
            .proposals
            .get_mut(&proposal.id)
            .unwrap()
            .approval = Some(Approval {
            epoch: a.control_epoch,
            listening: a.paused,
        });
        a.work = Some(work);
        a.activity = "working".into();
        a.status = "Applying approved change visibly".into();
        Ok(())
    }

    pub(super) fn validate_presence_proposal(
        &self,
        proposal: &Proposal,
        a: &Collaborator,
    ) -> Result<(), String> {
        #[cfg(test)]
        FULL_PROPOSAL_CHECKS.with(|checks| checks.set(checks.get() + 1));
        if self.live_presence_proposal(&proposal.id) != Some(proposal)
            || !self.pending_stage_proposals().iter().any(|p| p == proposal)
            || proposal.status != ProposalStatus::Pending
        {
            return Err("This proposal was cancelled, changed, or requires recovery".into());
        }
        let p = self
            .presence
            .proposals
            .get(&proposal.id)
            .ok_or("The live prepared proposal is no longer available")?;
        if p.participant != a.profile.id
            || p.tab != a.tab
            || proposal.session.as_deref() != Some(a.binding.session_id.as_str())
            || proposal.cmds != p.prepared.cmds
            || proposal.title != p.prepared.command
            || proposal.target.workbook.as_deref() != p.prepared.workbook()
            || proposal.target.format_version != slate_doc::SlateDoc::CURRENT
        {
            return Err("The proposal no longer matches its exact prepared action".into());
        }
        self.validate_presence_action(a, &p.call, &p.prepared)
    }

    fn check_presence_proposal_decision(&mut self, proposal: &Proposal) -> Result<(), String> {
        let ws = self
            .ai
            .config
            .valid_workspace()
            .ok_or("The AI workspace is unavailable")?;
        match slate_doc::stage::read_result(ws, &proposal.id) {
            Ok(None) => Ok(()),
            outcome => {
                if matches!(
                    &outcome,
                    Ok(Some(slate_doc::stage::ProposalResult::Applying)) | Err(_)
                ) {
                    self.mark_stage_proposal_recovery(&proposal.id);
                }
                match outcome {
                    Err(error) => Err(format!("Could not verify the proposal decision: {error}")),
                    Ok(Some(
                        slate_doc::stage::ProposalResult::Accepted
                        | slate_doc::stage::ProposalResult::Rejected
                        | slate_doc::stage::ProposalResult::Stale { .. },
                    )) => {
                        self.discard_live_presence_proposal(&proposal.id);
                        Err("This proposal already has a recorded decision".into())
                    }
                    _ => Err("This proposal already has a decision or requires recovery".into()),
                }
            }
        }
    }

    /// Shared admission and revalidation for a prepared board action. Only real
    /// execution reserves targets, scoped to the board that owns their IDs.
    pub(super) fn validate_presence_action(
        &self,
        a: &Collaborator,
        call: &BoardCall,
        prepared: &PreparedBoardCommand,
    ) -> Result<(), String> {
        if self.tab().read_only
            || a.departure.is_some()
            || a.tab != self.tab().id
            || a.request.is_empty()
            || prepared.tab_id() != a.tab
            || prepared.workbook() != self.tab().path.as_deref()
        {
            return Err("The collaborator, request, or active board changed".into());
        }
        let grant = a.grant.as_ref().ok_or("Board grant revoked")?;
        grant
            .authorize(call, &a.binding, &a.request)
            .map_err(str::to_owned)?;
        if prepared.requires_linked_inputs()
            && !grant
                .readable_sources
                .iter()
                .any(|s| s == "board-file-inputs")
        {
            return Err("The linked-file input grant was revoked".into());
        }
        if prepared
            .targets
            .iter()
            .any(|id| self.doc().scene.node(*id).is_some() && !grant.allows_node(&id.0.to_string()))
        {
            return Err("Prepared targets exceed the current board grant".into());
        }
        if self.human_owns_presence_targets(&prepared.targets) {
            return Err("The human is editing this action's target".into());
        }
        if self
            .presence
            .agents
            .iter()
            .filter(|other| {
                other.profile.id != a.profile.id && other.tab == a.tab && other.departure.is_none()
            })
            .filter_map(|other| other.work.as_ref())
            .filter(|work| work.purpose.replaces_objects())
            .any(|work| {
                work.targets()
                    .iter()
                    .any(|id| prepared.targets.contains(id))
            })
        {
            return Err("Another collaborator is handling this target".into());
        }
        Ok(())
    }

    pub(super) fn validate_presence_work(
        &self,
        a: &Collaborator,
        work: &WitnessedCommand,
    ) -> Result<(), String> {
        if let witness::WitnessAction::Relay(relay) = &work.action {
            if work.purpose != WitnessPurpose::Interaction {
                return Err("Invalid conversation witness purpose".into());
            }
            return self.validate_presence_relay(a, &work.call, relay);
        }
        self.validate_presence_action(
            a,
            &work.call,
            work.board_prepared().ok_or("Missing board action")?,
        )?;
        match &work.purpose {
            WitnessPurpose::ApprovedProposal(id) => {
                let proposal = self
                    .live_presence_proposal(id)
                    .ok_or("The approved proposal was cancelled")?;
                let context = self.presence.proposals.get(id).ok_or("Approval ended")?;
                // The canonical entry is immutable and duplicate IDs are
                // refused by the stage owner. Deep command/snapshot equality
                // belongs at Accept and immediately precommit, never per frame.
                let pending = self.pending_stage_proposals().iter().find(|p| p.id == *id);
                if proposal.status != ProposalStatus::Pending
                    || !pending.is_some_and(|p| p.status == ProposalStatus::Pending)
                    || context.participant != a.profile.id
                    || context.tab != a.tab
                    || context.approval.is_none()
                    || context.call.id != work.call.id
                    || context.call.session != a.binding
                    || context.call.request_id != a.request
                    || context.call.sequence != work.call.sequence
                {
                    return Err("The scoped approval is no longer active".into());
                }
            }
            WitnessPurpose::Autonomous if !a.grant.as_ref().unwrap().mutation_autonomy => {
                return Err("Autonomous board edits were revoked".into())
            }
            _ => {}
        }
        Ok(())
    }

    /// Called after reinserting the collaborator, so every validator sees it.
    pub(super) fn complete_presence_proposal(&mut self, id: &str) {
        if let Some(proposal) = self.live_presence_proposal(id).cloned() {
            self.commit_accepted_stage_proposal(&proposal);
        }
        // Recovery and transient failure retain the immutable pending proposal,
        // but never keep a live applying token or silently retry a commit.
        self.presence_end_proposal_motion(id, false);
    }

    /// The canonical stage adapter also calls this, so no direct caller can
    /// bypass scoped approval, completed motion, or immediate precommit checks.
    pub(crate) fn presence_validate_completed_proposal(
        &mut self,
        proposal: &Proposal,
    ) -> Result<(), String> {
        let p = self
            .presence
            .proposals
            .get(&proposal.id)
            .ok_or("Approval ended")?;
        let a = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == p.participant)
            .ok_or("Collaborator retired")?;
        let work = a.work.as_ref().ok_or("Approved action ended")?;
        if a.paused && !self.presence.approved_while_listening(a) {
            return Err("The human paused this approved action".into());
        }
        if work.purpose != WitnessPurpose::ApprovedProposal(proposal.id.clone())
            || work.index != work.steps.len()
        {
            return Err("Approved motion is incomplete".into());
        }
        self.validate_presence_work(a, work)?;
        self.validate_presence_proposal(proposal, a)?;
        let prepared = p.prepared.clone();
        self.check_targeted_command(&prepared)
    }

    pub(crate) fn presence_end_proposal_motion(&mut self, id: &str, forget: bool) {
        let Some(p) = self.presence.proposals.get_mut(id) else {
            return;
        };
        p.approval = None;
        if let Some(a) = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == p.participant)
        {
            if a.work
                .as_ref()
                .is_some_and(|w| w.purpose == WitnessPurpose::ApprovedProposal(id.into()))
            {
                if let Err(error) = a.cancel_motion(&mut self.presence.roster) {
                    a.status = error;
                    a.activity = "blocked".into();
                    return;
                }
                a.work = None;
                a.status = if a.paused {
                    "Listening; task suspended"
                } else {
                    "Proposal ready"
                }
                .into();
                a.activity = if a.paused { "listening" } else { "approval" }.into();
            }
        }
        if forget {
            self.presence.proposals.remove(id);
        }
    }

    /// The originating call's single response is staged. Completion is
    /// visible in Slate and its recorded stage result; it is not a second
    /// provider reply or wakeup. Newly created chat scope is registered here,
    /// only after the canonical journal commit and explicit human opt-in.
    pub(crate) fn presence_accepted_proposal(&mut self, proposal: &Proposal) {
        let Some(p) = self.presence.proposals.get(&proposal.id) else {
            return;
        };
        let Some(index) = self
            .presence
            .agents
            .iter()
            .position(|a| a.profile.id == p.participant)
        else {
            return;
        };
        let nodes = p.prepared.created_ids();
        self.presence_end_proposal_motion(&proposal.id, true);
        let mut a = self.presence.agents.remove(index);
        if !a.paused {
            a.status = "Action completed".into();
            a.activity = "success".into();
        }
        if !nodes.is_empty() {
            if let Err(error) = self.presence_register_created_chats(&mut a, &nodes) {
                a.status = error;
            }
        }
        self.presence.agents.insert(index, a);
    }
}
