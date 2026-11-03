//! Explicit CLI production enters the ordinary witnessed command owner.
//! This registry is ephemeral host authority, never a provider/mailbox grant.
use super::*;
use serde_json::Value;
use slate_doc::NodeId;
use std::path::Path;

#[cfg(test)]
#[path = "production_tests.rs"]
mod tests;

#[derive(Clone, PartialEq, Eq)]
struct BoardPin {
    tab: u64,
    root: PathBuf,
    workbook: PathBuf,
}
pub(super) struct ProductionActor {
    author: String,
    board: BoardPin,
    binding: Option<BoardLinkSession>,
    epoch: u64,
    grant: Option<CollaboratorGrant>,
    request: String,
}
pub(super) struct ProductionWitness {
    actor: String,
    call: BoardCall,
    created: Vec<NodeId>,
    terminal: Option<Result<(), String>>,
}

impl PresenceRuntime {
    /// Only the canonical normal command completion may report committed work.
    pub(super) fn observe_production_reply(&mut self, reply: &BoardReply) {
        let Some(witness) = self.production_witnesses.get_mut(&reply.id) else {
            return;
        };
        if witness.terminal.is_some() || !reply.matches(&witness.call) {
            return;
        }
        witness.terminal = Some(if !reply.success {
            Err(reply.result["error"]
                .as_str()
                .unwrap_or("Production command failed")
                .into())
        } else {
            let expected: Vec<_> = witness.created.iter().map(|id| id.0.to_string()).collect();
            let actual: Option<Vec<_>> = reply.result["created_ids"].as_array().and_then(|ids| {
                ids.iter()
                    .map(|id| id.as_str().map(str::to_owned))
                    .collect()
            });
            if reply.result["status"]
                .as_str()
                .is_some_and(|s| s.starts_with("applied:"))
                && actual.as_ref() == Some(&expected)
            {
                Ok(())
            } else {
                Err("Production did not receive its matching applied command receipt".into())
            }
        });
    }
}

impl SlateApp {
    fn production_board_pin(&self, root: &Path) -> Result<BoardPin, String> {
        if self.at_home || self.tabs.is_empty() || self.tab().read_only {
            return Err("Production requires its writable board workbook".into());
        }
        if !root.is_absolute() {
            return Err("Production workspace must be absolute".into());
        }
        let root = root.canonicalize().map_err(|e| e.to_string())?;
        let workbook = self
            .tab()
            .path
            .clone()
            .ok_or("Production workbook must be saved first")?;
        let resolved = workbook.canonicalize().map_err(|e| e.to_string())?;
        if !resolved.starts_with(&root) {
            return Err("Production workbook is outside its declared workspace".into());
        }
        let pin = BoardPin {
            tab: self.tab().id,
            root,
            workbook,
        };
        if self
            .presence
            .production_actors
            .values()
            .any(|actor| actor.board != pin)
        {
            return Err("Production is pinned to another workbook or workspace root".into());
        }
        Ok(pin)
    }
    fn validate_production_actor(&self, who: &str) -> Result<usize, String> {
        if !self.presence.production_actors.contains_key(who) {
            return Err("Unknown production actor".into());
        }
        let index = self
            .presence
            .agents
            .iter()
            .position(|a| a.profile.id == who)
            .ok_or("Production collaborator departed; a new grant requires human direction")?;
        self.validate_production_presence(&self.presence.agents[index])?;
        Ok(index)
    }
    /// Rechecked by the normal prepared-work owner while it temporarily owns a
    /// detached collaborator, including immediately before the journal commit.
    pub(super) fn validate_production_presence(&self, actor: &Collaborator) -> Result<(), String> {
        let pin = self.presence.production_actors.get(&actor.profile.id);
        let Some(pin) = pin else {
            return Ok(());
        };
        if self.at_home
            || self.tabs.is_empty()
            || self.tab().read_only
            || self.tab().id != pin.board.tab
            || self.tab().path.as_ref() != Some(&pin.board.workbook)
        {
            return Err("Production workbook is no longer active or writable".into());
        }
        if actor.paused
            || actor.departure.is_some()
            || actor.profile.retired
            || actor.tab != pin.board.tab
            || actor.profile.name != pin.author
            || actor.profile.provider != "local"
            || actor.control_epoch != pin.epoch
            || Some(&actor.binding) != pin.binding.as_ref()
            || actor.grant.as_ref() != pin.grant.as_ref()
            || actor.grant.is_none()
            || actor.request != pin.request
        {
            return Err(
                "Production collaborator was paused, revoked, departed, or changed by the human"
                    .into(),
            );
        }
        Ok(())
    }
    pub(crate) fn production_witness_command(
        &mut self,
        ctx: &egui::Context,
        workspace_root: &Path,
        author: &str,
        command: &str,
        targets: &[String],
        arguments: &Value,
    ) -> Result<(Vec<NodeId>, String), String> {
        if author.trim().is_empty() || author.len() > 128 {
            return Err("Production needs a bounded named author".into());
        }
        // Media has its own shared linked-item owner. All admitted commands here
        // must pass the normal prepared-board/witnessed mutation path.
        if !command.starts_with("board.")
            || !BOARD_COMMANDS.contains(&command)
            || command == "board.objects.place_images"
        {
            return Err("Production command is outside the witnessed board surface".into());
        }
        if self.presence.production_witnesses.len() >= 16 {
            return Err("Finish pending production receipts before requesting more".into());
        }
        let board = self.production_board_pin(workspace_root)?;
        let who = format!(
            "production-{}",
            model_preview::view_meta::hash_bytes(author.as_bytes())
        );
        if !self.presence.production_actors.contains_key(&who) {
            if self.presence.production_actors.len() >= 8
                || self.presence.agents.iter().any(|a| a.profile.id == who)
            {
                return Err("Production actor limit reached or identity already belongs to another collaborator".into());
            }
            self.presence.production_actors.insert(
                who.clone(),
                ProductionActor {
                    author: author.into(),
                    board: board.clone(),
                    binding: None,
                    epoch: 0,
                    grant: None,
                    request: String::new(),
                },
            );
            if let Err(error) = self.join_scoped_presence_at(
                StoredAgent {
                    id: who.clone(),
                    name: author.into(),
                    hat: "none".into(),
                    provider: "local".into(),
                    retired: false,
                },
                None,
                board.root.clone(),
            ) {
                self.presence.production_actors.remove(&who);
                return Err(error);
            }
            let actor = self
                .presence
                .agents
                .iter_mut()
                .find(|a| a.profile.id == who)
                .ok_or("Production collaborator did not join")?;
            let grant = actor
                .grant
                .as_mut()
                .ok_or("Production collaborator has no grant")?;
            grant.commands.clear();
            grant.conversation_ids.clear();
            grant.readable_sources.clear();
            grant.readable_nodes.clear();
            grant.readable_board = true;
            grant.mutation_autonomy = true;
            grant.helper_limit = 4;
            grant.max_actions = 1;
            let pin = self.presence.production_actors.get_mut(&who).unwrap();
            pin.binding = Some(actor.binding.clone());
            pin.epoch = actor.control_epoch;
            pin.grant = actor.grant.clone();
        }
        let index = self.validate_production_actor(&who)?;
        if self.presence.agents[index].board_action_active()
            || self.presence.has_pending_for(&self.presence.agents[index])
            || self
                .presence
                .production_witnesses
                .values()
                .any(|w| w.actor == who)
        {
            return Err("Finish this production collaborator's current receipt first".into());
        }
        let mut actor = self.presence.agents.remove(index);
        let outcome = (|| {
            let grant = actor.grant.as_mut().ok_or("Production grant revoked")?;
            grant.commands = vec![command.into()];
            grant.validate().map_err(str::to_owned)?;
            actor.request = atlas_agent::request_id();
            actor.sequences.clear();
            actor.calls = 0;
            self.presence
                .roster
                .session_mut(&actor.motion)
                .ok_or("Production motion session ended")?
                .begin_request(id(&actor.request)?)
                .map_err(|e| e.to_string())?;
            let pin = self.presence.production_actors.get_mut(&who).unwrap();
            pin.grant = actor.grant.clone();
            pin.request = actor.request.clone();
            let call = BoardCall {
                id: atlas_agent::request_id(),
                session: actor.binding.clone(),
                request_id: actor.request.clone(),
                sequence: 0,
                operation: BoardOperation::Invoke {
                    command: command.into(),
                    targets: targets.to_vec(),
                    arguments: arguments.clone(),
                },
            };
            let reply = self.presence_call_with_context(ctx, &mut actor, &call);
            if let Some(reply) = reply {
                return Err(reply.result["error"]
                    .as_str()
                    .unwrap_or("Production command was not witnessed")
                    .into());
            }
            let created = actor
                .work
                .as_ref()
                .and_then(|w| w.board_prepared())
                .ok_or("Production command did not enter witnessed work")?
                .created_ids();
            let ticket = call.id.clone();
            self.presence.production_witnesses.insert(
                ticket.clone(),
                ProductionWitness {
                    actor: who.clone(),
                    call,
                    created: created.clone(),
                    terminal: None,
                },
            );
            ctx.request_repaint();
            Ok((created, ticket))
        })();
        self.presence.agents.insert(index, actor);
        outcome
    }
    pub(crate) fn production_witness_poll(
        &mut self,
        ticket: &str,
    ) -> Result<Option<Vec<NodeId>>, String> {
        let witness = self
            .presence
            .production_witnesses
            .get(ticket)
            .ok_or("Unknown production receipt")?;
        let who = witness.actor.clone();
        let index = match self.validate_production_actor(&who) {
            Ok(index) => index,
            Err(error) => {
                // A stopped production must not leave a resumable command behind
                // that can later commit after the runner recorded failure.
                let _ = self.stop_presence(&who, false);
                self.presence.production_witnesses.remove(ticket);
                return Err(error);
            }
        };
        let witness = self.presence.production_witnesses.get(ticket).unwrap();
        let outcome = witness.terminal.clone().or_else(|| {
            let actor = &self.presence.agents[index];
            (!actor
                .work
                .as_ref()
                .is_some_and(|work| work.call == witness.call))
            .then(|| {
                Err(format!(
                    "Production action ended without an applied receipt: {}",
                    actor.status
                ))
            })
        });
        match outcome {
            None => Ok(None),
            Some(result) => {
                let witness = self.presence.production_witnesses.remove(ticket).unwrap();
                result.map(|()| Some(witness.created))
            }
        }
    }
}
