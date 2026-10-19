use super::witness::WitnessedCommand;
use super::work::command_activity;
use super::*;
use serde_json::{json, Value};

impl SlateApp {
    #[cfg(test)]
    pub(super) fn presence_call(
        &mut self,
        a: &mut Collaborator,
        call: &BoardCall,
    ) -> Option<BoardReply> {
        self.presence_call_context(None, a, call)
    }
    pub(super) fn presence_call_with_context(
        &mut self,
        ctx: &egui::Context,
        a: &mut Collaborator,
        call: &BoardCall,
    ) -> Option<BoardReply> {
        self.presence_call_context(Some(ctx), a, call)
    }
    fn presence_call_context(
        &mut self,
        ctx: Option<&egui::Context>,
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
                    && a.work.as_ref().is_some_and(|w| {
                        matches!(w.purpose, witness::WitnessPurpose::ApprovedProposal(_))
                    }))
                || (a.paused
                    && matches!(&call.operation, BoardOperation::Invoke { command, .. } if !matches!(command.as_str(), "presence.read_image" | "presence.read_text" | "presence.preview")))
            {
                return Err("Board is inactive or collaborator paused".into());
            }
            if a.calls >= usize::from(grant.max_actions) {
                return Err(
                    "Task action budget exhausted; send Continue for a fresh task budget.".into(),
                );
            }
            if !a.sequences.insert(call.sequence) {
                return Err("Duplicate board call sequence".into());
            }
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
                    if a.board_action_active() {
                        return Err("Wait for the current action or proposal decision".into());
                    }
                    if portal_ops::recognizes(command) {
                        return self.begin_presence_portal_call(ctx, a, call);
                    }
                    if matches!(
                        command.as_str(),
                        "presence.read_image" | "presence.read_text" | "presence.preview"
                    ) {
                        self.begin_presence_read(a, call)?;
                        return Ok(None);
                    }
                    if command == "presence.send" {
                        let relay = self.prepare_presence_relay(a, targets, arguments)?;
                        a.observed_target = relay.node();
                        a.activity = "exchanging".into();
                        a.status = "Approaching conversation · visible relay".into();
                        a.work = Some(WitnessedCommand::relay(call.clone(), relay));
                        return Ok(None);
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
                            let _ = self.retire_presence(&who, chrome::RANDOM_DEPARTURE);
                            return Err(error);
                        }
                        return Ok(Some(
                            json!({"helper":who,"status":"assigned","limit_seconds":300}),
                        ));
                    }
                    if command == "app.save" {
                        if !targets.is_empty()
                            || !arguments
                                .as_object()
                                .is_some_and(|object| object.is_empty())
                        {
                            return Err("Save takes no targets or arguments".into());
                        }
                        if a.parent.is_some() || !grant.mutation_autonomy {
                            return Err("Saving requires an independent collaborator with explicit mutation autonomy".into());
                        }
                        let tab = self.tab();
                        if tab.read_only {
                            return Err("The current workbook is read-only; ask the human to choose a writable copy".into());
                        }
                        let path = tab.path.clone().ok_or(
                            "This workbook has no saved path yet; use Save As in Slate first",
                        )?;
                        let deferred = self.layer_bakes.pending_for(tab.id);
                        let ctx =
                            ctx.ok_or("Saving is available only from a live board-link action")?;
                        self.dispatch(ctx, atlas_commands::CommandId("app.save"), None);
                        return Ok(Some(json!({
                            "status":"save_requested",
                            "workbook":path,
                            "deferred_for_background_asset_save":deferred
                        })));
                    }
                    if command == "app.feedback.report" {
                        if !targets.is_empty() {
                            return Err("Feedback reports take no targets".into());
                        }
                        let fields = arguments
                            .as_object()
                            .ok_or("Feedback report arguments must be an object")?;
                        if fields
                            .keys()
                            .any(|key| !matches!(key.as_str(), "what" | "tried" | "missing"))
                        {
                            return Err(
                                "Feedback report accepts only what, tried, and missing".into()
                            );
                        }
                        let read_field = |key: &str, required: bool| -> Result<String, String> {
                            let Some(value) = fields.get(key) else {
                                return if required {
                                    Err("Feedback needs a what sentence".into())
                                } else {
                                    Ok(String::new())
                                };
                            };
                            let text = value
                                .as_str()
                                .ok_or_else(|| format!("Feedback field {key} must be text"))?;
                            if text.chars().count() > 800 {
                                return Err(format!("Feedback field {key} exceeds 800 characters"));
                            }
                            Ok(text.to_owned())
                        };
                        let note = atlas_agent::FeedbackNote {
                            id: atlas_agent::request_id(),
                            what: read_field("what", true)?,
                            tried: read_field("tried", false)?,
                            missing: read_field("missing", false)?,
                        };
                        atlas_ai::feedback::record_local(&a.dir, note)
                            .map_err(|error| format!("Could not record local feedback: {error}"))?;
                        return Ok(Some(json!({"status":"recorded_locally"})));
                    }
                    let prepared = match ctx {
                        Some(ctx) => self.prepare_targeted_command_with_context(
                            ctx, command, targets, arguments,
                        )?,
                        None => self.prepare_targeted_command(command, targets, arguments)?,
                    };
                    self.validate_presence_action(a, call, &prepared)?;
                    self.validate_creation_admission(a, call, &prepared)?;
                    a.activity = command_activity(command).into();
                    a.observed_target = prepared.targets.first().copied();
                    a.status = "Approaching target · visible action".into();
                    let mut work = WitnessedCommand::new(
                        call.clone(),
                        prepared,
                        &self.doc().scene,
                        self.board_wire_routing,
                    )?;
                    work.set_drawing_helpers(grant.helper_limit);
                    work.purpose = if grant.mutation_autonomy {
                        witness::WitnessPurpose::Autonomous
                    } else {
                        witness::WitnessPurpose::ProposalPreview
                    };
                    a.work = Some(work);
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
