//! Saved output materialization returns to the existing witnessed proposal owner.
use super::witness::{WitnessPurpose, WitnessedCommand};
use super::*;
use crate::app::dispatch::PreparedBoardCommand;
use slate_doc::scene::SceneCmd;

pub(crate) struct ArtifactResultLease {
    participant: String,
    author: String,
    epoch: u64,
    call: BoardCall,
    prepared: PreparedBoardCommand,
    purpose: WitnessPurpose,
}
impl SlateApp {
    pub(super) fn defer_presence_artifact_pick(
        &mut self,
        a: &mut Collaborator,
        mut work: WitnessedCommand,
    ) -> Option<WitnessedCommand> {
        let prepared = work.board_prepared()?;
        let Some((target, revision, remove)) = prepared.pending_artifact_pick() else {
            return Some(work);
        };
        let result = self
            .validate_presence_work(a, &work)
            .and_then(|()| self.check_targeted_command(prepared));
        if let Err(error) = result {
            self.fail_presence_artifact_pick(a, &work.call, &error);
            return None;
        }
        let lease = ArtifactResultLease {
            participant: a.profile.id.clone(),
            author: a.profile.name.clone(),
            epoch: a.control_epoch,
            call: work.call.clone(),
            prepared: prepared.clone(),
            purpose: work.purpose.clone(),
        };
        if !self.artifact_pick_revision_removing(target, &revision, remove, Some(Box::new(lease))) {
            self.fail_presence_artifact_pick(
                a,
                &work.call,
                "Saved artifact result could not prepare",
            );
            return None;
        }
        if let witness::WitnessAction::Board(prepared) = &mut work.action {
            prepared.wait_for_artifact_pick();
        }
        a.work = Some(work);
        a.status = "Preparing saved artifact output".into();
        None
    }
    fn fail_presence_artifact_pick(&mut self, a: &mut Collaborator, call: &BoardCall, error: &str) {
        let reply = BoardReply::error(call, error);
        self.presence.observe_production_reply(&reply);
        let _ = self.presence.links.reply(&a.dir, reply);
        a.status = error.into();
        a.activity = "blocked".into();
    }
    pub(crate) fn complete_presence_artifact_pick(
        &mut self,
        mut lease: ArtifactResultLease,
        result: Result<SceneCmd, String>,
    ) {
        let Some(index) = self
            .presence
            .agents
            .iter()
            .position(|a| a.profile.id == lease.participant)
        else {
            return;
        };
        let mut a = self.presence.agents.remove(index);
        let owns_work = a.work.as_ref().is_some_and(|work| {
            work.call.id == lease.call.id
                && work.call.session == lease.call.session
                && work.call.request_id == lease.call.request_id
                && work.call.sequence == lease.call.sequence
                && work
                    .board_prepared()
                    .is_some_and(|prepared| prepared.artifact_pick_waiting())
        });
        // Stop/retirement already replies and removes this exact work. A late
        // derived result must not reply twice or replace a newer action.
        if !owns_work {
            self.presence.agents.insert(index, a);
            return;
        }
        let outcome: Result<WitnessedCommand, String> = (|| {
            if !owns_work
                || a.work.as_ref().unwrap().purpose != lease.purpose
                || a.profile.name != lease.author
                || a.control_epoch != lease.epoch
                || a.paused
                || a.portal.is_some()
                || a.read.is_some()
                || !a.launched.is_empty()
            {
                return Err("The originating saved-output action was cancelled or replaced".into());
            }
            self.validate_presence_work(&a, a.work.as_ref().unwrap())?;
            self.check_targeted_command(&lease.prepared)?;
            lease.prepared.resolve_artifact_pick(result?)?;
            self.check_targeted_command(&lease.prepared)?;
            let mut work = WitnessedCommand::new(
                lease.call.clone(),
                lease.prepared,
                &self.doc().scene,
                self.board_wire_routing,
            )?;
            work.purpose = lease.purpose;
            work.set_drawing_helpers(a.grant.as_ref().map_or(0, |grant| grant.helper_limit));
            Ok(work)
        })();
        match outcome {
            Ok(work) => {
                a.work = Some(work);
                a.status = "Preparing saved-output proposal visibly".into();
            }
            Err(error) => {
                if owns_work {
                    a.work = None;
                }
                self.fail_presence_artifact_pick(&mut a, &lease.call, &error);
            }
        }
        self.presence.agents.insert(index, a);
    }
}
