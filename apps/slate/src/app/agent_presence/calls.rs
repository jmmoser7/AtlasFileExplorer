use super::witness::WitnessedCommand;
use super::work::command_activity;
use super::*;
use serde_json::{json, Value};

impl SlateApp {
    pub(super) fn presence_call(
        &mut self,
        a: &mut Collaborator,
        call: &BoardCall,
    ) -> Option<BoardReply> {
        let result = (|| -> Result<Option<Value>, String> {
            let grant = a.grant.as_ref().ok_or("Board grant revoked")?;
            grant
                .authorize(call, &a.binding, &a.request)
                .map_err(str::to_owned)?;
            if a.tab != self.tab().id
                || a.departure.is_some()
                || (a.paused
                    && matches!(&call.operation, BoardOperation::Invoke { command, .. } if command != "presence.read_image"))
            {
                return Err("Board is inactive or collaborator paused".into());
            }
            if a.calls >= usize::from(grant.max_actions)
                || (a.sequences & (1u128 << call.sequence)) != 0
            {
                return Err("Action budget exhausted or duplicate sequence".into());
            }
            a.sequences |= 1u128 << call.sequence;
            a.calls += 1;
            a.last_active = Instant::now();
            match &call.operation {
                BoardOperation::Read { .. } | BoardOperation::ReadParticipants { .. } => {
                    self.begin_presence_read(a, call)?;
                    Ok(None)
                }
                BoardOperation::Invoke {
                    command,
                    targets,
                    arguments,
                } => {
                    if a.work.is_some() || a.proposed.is_some() || a.read.is_some() {
                        return Err("Wait for the current action or proposal decision".into());
                    }
                    if command == "presence.read_image" {
                        self.begin_presence_read(a, call)?;
                        return Ok(None);
                    }
                    if command == "presence.send" {
                        return self.relay_presence_message(a, targets, arguments).map(Some);
                    }
                    if command == "presence.helper" {
                        let task = arguments["task"]
                            .as_str()
                            .filter(|t| !t.trim().is_empty() && t.len() <= 8192)
                            .ok_or("A temporary helper needs a bounded task")?;
                        if a.parent.is_some() || grant.helper_limit == 0 {
                            return Err("Helper spawning is outside this grant".into());
                        }
                        let who = atlas_agent::request_id();
                        let profile = StoredAgent {
                            id: who.clone(),
                            name: self
                                .presence
                                .next_helper_name(&a.profile.id, &a.profile.name),
                            hat: "beanie".into(),
                            provider: a.profile.provider.clone(),
                            retired: false,
                        };
                        let mut scope = grant.clone();
                        if !scope.conversation_ids.contains(&a.profile.id) {
                            scope.conversation_ids.push(a.profile.id.clone());
                        }
                        self.join_scoped_presence(
                            profile,
                            Some((a.profile.id.clone(), a.motion.clone(), scope)),
                        )?;
                        if let Some(g) = a.grant.as_mut() {
                            if !g.conversation_ids.contains(&who) {
                                g.conversation_ids.push(who.clone());
                            }
                        }
                        if let Err(error) =
                            self.send_presence_message_from(&who, task, Some(&a.profile.name))
                        {
                            let _ = self.retire_presence(&who, "bow");
                            return Err(error);
                        }
                        return Ok(Some(
                            json!({"helper":who,"status":"assigned","limit_seconds":300}),
                        ));
                    }
                    let prepared = self.prepare_targeted_command(command, targets, arguments)?;
                    if self.human_owns_presence_targets(&prepared.targets) {
                        return Err(
                            "The human is editing this target; wait for them to finish".into()
                        );
                    }
                    if prepared.requires_linked_inputs()
                        && !grant
                            .readable_sources
                            .iter()
                            .any(|s| s == "board-file-inputs")
                    {
                        return Err("This generator reads linked files; enable its participant's wired-file input grant first".into());
                    }
                    if prepared.targets.iter().any(|id| {
                        self.doc().scene.node(*id).is_some()
                            && !grant.allows_node(&id.0.to_string())
                    }) {
                        return Err("Expanded command targets exceed the board grant".into());
                    }
                    if self
                        .presence
                        .agents
                        .iter()
                        .filter_map(|other| other.work.as_ref())
                        .any(|w| {
                            w.prepared
                                .targets
                                .iter()
                                .any(|id| prepared.targets.contains(id))
                        })
                    {
                        return Err("Another collaborator is handling this target".into());
                    }
                    a.activity = command_activity(command).into();
                    a.observed_target = prepared.targets.first().copied();
                    a.status = "Approaching target · visible action".into();
                    a.work = Some(WitnessedCommand::new(
                        call.clone(),
                        prepared,
                        &self.doc().scene,
                        self.board_wire_routing,
                    )?);
                    Ok(None)
                }
            }
        })();
        match result {
            Ok(None) => None,
            Ok(Some(value)) => {
                if serde_json::to_vec(&value).map_or(true, |s| s.len() > 60_000) {
                    return Some(BoardReply::error(
                        call,
                        "Context exceeds the reply limit; request fewer explicit node IDs",
                    ));
                }
                Some(BoardReply {
                    id: call.id.clone(),
                    session_id: a.binding.session_id.clone(),
                    request_id: a.request.clone(),
                    success: true,
                    result: value,
                })
            }
            Err(e) => {
                if !a.request.is_empty()
                    && call.request_id == a.request
                    && call.session == a.binding
                {
                    a.note_helper_outcome("blocked", &e);
                }
                a.status = e.clone();
                a.activity = "blocked".into();
                Some(BoardReply::error(call, &e))
            }
        }
    }
}
