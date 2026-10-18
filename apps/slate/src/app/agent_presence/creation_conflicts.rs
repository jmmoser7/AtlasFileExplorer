//! Admission for node IDs reserved by another in-flight agent creation.
use super::*;
use crate::app::dispatch::PreparedBoardCommand;
use slate_doc::{scene::SceneCmd, Proposal, ProposalStatus};

const CREATION_CONFLICT: &str =
    "A collaborator has a pending object creation; wait for it to finish or be reviewed, then retry this command";

impl SlateApp {
    pub(super) fn validate_creation_admission(
        &self,
        actor: &Collaborator,
        call: &BoardCall,
        prepared: &PreparedBoardCommand,
    ) -> Result<(), String> {
        let incoming = prepared.created_ids();
        if incoming.is_empty() {
            return Ok(());
        }

        let approved_proposal = self.presence.proposals.iter().find_map(|(id, context)| {
            (context.participant == actor.profile.id
                && context.tab == actor.tab
                && context.call.id == call.id)
                .then_some(id.as_str())
        });
        let mut reserved = Vec::new();
        for other in self.presence.agents.iter().filter(|other| {
            other.profile.id != actor.profile.id
                && other.tab == actor.tab
                && other.departure.is_none()
        }) {
            if let Some(work) = other.work.as_ref() {
                if let Some(prepared) = work.board_prepared() {
                    reserved.extend(prepared.created_ids());
                }
            }
        }

        let workbook = self.tab().path.as_deref();
        for proposal in self.pending_stage_proposals().iter().filter(|proposal| {
            matches!(
                proposal.status,
                ProposalStatus::Pending | ProposalStatus::RecoveryRequired
            ) && Some(proposal.id.as_str()) != approved_proposal
        }) {
            let same_tab = self.presence.proposals.get(&proposal.id).map_or_else(
                || proposal.target.workbook.as_deref() == workbook,
                |context| context.tab == actor.tab,
            );
            if same_tab {
                if let Some(context) = self.presence.proposals.get(&proposal.id) {
                    reserved.extend(context.prepared.created_ids());
                } else if workbook.is_some() && proposal.target.workbook.as_deref() == workbook {
                    reserved.extend(proposal_created_ids(proposal));
                }
            }
        }

        if created_ids_overlap(&incoming, &reserved) {
            Err(CREATION_CONFLICT.into())
        } else {
            Ok(())
        }
    }
}

fn proposal_created_ids(proposal: &Proposal) -> impl Iterator<Item = slate_doc::NodeId> + '_ {
    proposal.cmds.iter().filter_map(|cmd| match cmd {
        SceneCmd::Add { node, .. } => Some(node.id),
        _ => None,
    })
}

fn created_ids_overlap(incoming: &[slate_doc::NodeId], reserved: &[slate_doc::NodeId]) -> bool {
    incoming.iter().any(|id| reserved.contains(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_doc::scene::NodeId;

    use crate::app::agent_presence::tests::{add, host, proposal_support};

    fn attempt_create(h: &mut crate::app::tests::Harness, who: &str) -> Result<(), String> {
        let index = h
            .app
            .presence
            .agents
            .iter()
            .position(|agent| agent.profile.id == who)
            .unwrap();
        let mut actor = h.app.presence.agents.remove(index);
        if actor.request.is_empty() {
            actor.request = atlas_agent::request_id();
            h.app
                .presence
                .roster
                .session_mut(&actor.motion)
                .unwrap()
                .begin_request(super::super::id(&actor.request).unwrap())
                .unwrap();
        }
        let call = BoardCall {
            id: atlas_agent::request_id(),
            session: actor.binding.clone(),
            request_id: actor.request.clone(),
            sequence: actor.calls as u64,
            operation: BoardOperation::Invoke {
                command: "board.objects.create_frame".into(),
                targets: Vec::new(),
                arguments: serde_json::json!({
                    "x": 600,
                    "y": 100,
                    "width": 160,
                    "height": 100
                }),
            },
        };
        let reply = h.app.presence_call(&mut actor, &call);
        h.app.presence.agents.insert(index, actor);
        match reply {
            Some(reply) if !reply.success => Err(reply.result["error"]
                .as_str()
                .unwrap_or("rejected")
                .to_owned()),
            _ => Ok(()),
        }
    }

    #[test]
    fn another_same_tab_agent_cannot_preview_a_colliding_creation() {
        let mut h = host();
        let first = add(&mut h, "Dot");
        let second = add(&mut h, "Tess");
        proposal_support::begin_command(
            &mut h,
            &first,
            "board.objects.create_frame",
            &[],
            serde_json::json!({"x":600,"y":100,"width":160,"height":100}),
        );

        h.app
            .presence
            .agents
            .iter_mut()
            .find(|agent| agent.profile.id == second)
            .unwrap()
            .inflight = true;
        let second_inflight = h
            .app
            .presence
            .agents
            .iter()
            .find(|agent| agent.profile.id == second)
            .unwrap()
            .inflight;
        let error = attempt_create(&mut h, &second).unwrap_err();
        assert!(error.contains("wait for it to finish or be reviewed"));
        assert!(h
            .app
            .presence
            .agents
            .iter()
            .find(|agent| agent.profile.id == first)
            .unwrap()
            .work
            .is_some());
        let rejected = h
            .app
            .presence
            .agents
            .iter()
            .find(|agent| agent.profile.id == second)
            .unwrap();
        assert!(rejected.work.is_none());
        assert_eq!(rejected.inflight, second_inflight);
    }

    #[test]
    fn disjoint_existing_node_work_remains_admissible() {
        let mut h = host();
        let first = add(&mut h, "Dot");
        let second = add(&mut h, "Tess");
        let make_portal = |h: &mut crate::app::tests::Harness, x| {
            let node = h.app.doc_mut().scene.build_node(
                slate_doc::scene::WorldRect::new(x, 0.0, 160.0, 100.0),
                slate_doc::scene::NodeKind::Portal(slate_doc::scene::PortalNode::unbound_web(
                    "existing",
                )),
            );
            h.app.add_nodes(vec![node])[0]
        };
        let one = make_portal(&mut h, 0.0);
        let two = make_portal(&mut h, 220.0);
        proposal_support::begin_command(
            &mut h,
            &first,
            "board.objects.move",
            &[one],
            serde_json::json!({"dx":20,"dy":0}),
        );
        assert!(attempt_existing_move(&mut h, &second, two).is_ok());
    }

    fn attempt_existing_move(
        h: &mut crate::app::tests::Harness,
        who: &str,
        target: NodeId,
    ) -> Result<(), String> {
        let index = h
            .app
            .presence
            .agents
            .iter()
            .position(|agent| agent.profile.id == who)
            .unwrap();
        let mut actor = h.app.presence.agents.remove(index);
        if actor.request.is_empty() {
            actor.request = atlas_agent::request_id();
            h.app
                .presence
                .roster
                .session_mut(&actor.motion)
                .unwrap()
                .begin_request(super::super::id(&actor.request).unwrap())
                .unwrap();
        }
        let call = BoardCall {
            id: atlas_agent::request_id(),
            session: actor.binding.clone(),
            request_id: actor.request.clone(),
            sequence: actor.calls as u64,
            operation: BoardOperation::Invoke {
                command: "board.objects.move".into(),
                targets: vec![target.0.to_string()],
                arguments: serde_json::json!({"dx":20,"dy":0}),
            },
        };
        let reply = h.app.presence_call(&mut actor, &call);
        h.app.presence.agents.insert(index, actor);
        match reply {
            Some(reply) if !reply.success => Err(reply.result["error"]
                .as_str()
                .unwrap_or("rejected")
                .to_owned()),
            _ => Ok(()),
        }
    }

    #[test]
    fn another_proposal_by_the_same_agent_waits_for_its_pending_creation() {
        let mut h = host();
        let who = add(&mut h, "Dot");
        let proposal = proposal_support::stage_command(
            &mut h,
            &who,
            "board.objects.create_frame",
            &[],
            serde_json::json!({"x":600,"y":100,"width":160,"height":100}),
        );
        let error = attempt_create(&mut h, &who).unwrap_err();
        assert!(error.contains("wait for it to finish or be reviewed"));
        assert!(h.app.presence.proposals.contains_key(&proposal));
        h.app.accept_stage_proposal(&proposal);
        assert!(h
            .app
            .presence
            .agents
            .iter()
            .find(|agent| agent.profile.id == who)
            .unwrap()
            .work
            .as_ref()
            .is_some_and(|work| {
                work.purpose == witness::WitnessPurpose::ApprovedProposal(proposal)
            }));
    }
}
