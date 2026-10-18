use super::*;
/// One envelope for typed/relayed/queued task bodies. Check bytes before trim
/// so an oversized whitespace prefix cannot make unbounded frame work.
pub(super) const MESSAGE_BYTES: usize = 32_768;
pub(super) fn validate_message_text(text: &str) -> Result<(), String> {
    if text.len() > MESSAGE_BYTES || text.trim().is_empty() {
        return Err("Message is empty or too long (maximum 32768 UTF-8 bytes)".into());
    }
    Ok(())
}

impl SlateApp {
    pub(super) fn send_presence_message(&mut self, who: &str, text: &str) -> Result<(), String> {
        if self.presence.task_busy(who) {
            return self.presence_send_update(who, text);
        }
        self.send_presence_message_from(who, text, None)
    }
    pub(super) fn send_presence_message_from(
        &mut self,
        who: &str,
        text: &str,
        from: Option<&str>,
    ) -> Result<(), String> {
        self.submit_presence_message(who, text, from, None)
    }
    /// Queued board messages reuse their durable prompt/request; no second row
    /// and no mutation of the human's unsent composer draft.
    pub(super) fn submit_presence_message(
        &mut self,
        who: &str,
        text: &str,
        from: Option<&str>,
        queued: Option<&str>,
    ) -> Result<(), String> {
        validate_message_text(text)?;
        let provider = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who)
            .ok_or("Collaborator is not present; restore its saved profile first")?
            .profile
            .provider
            .clone();
        let choice = self.presence_model_choice(who, &provider);
        self.validate_presence_model(&provider, choice.model.as_deref(), &choice.parameters)?;
        let c = self.presence.conversations.edit(who)?;
        if let Some(request) = queued {
            let turn = c
                .request_index(request)
                .and_then(|i| c.turns.get(i))
                .ok_or("Queued message disappeared")?;
            if turn.role != "user"
                || turn.text != text
                || turn.status != "dispatching"
                || turn
                    .board_origin
                    .as_ref()
                    .is_some_and(|o| o.recipient != who)
            {
                return Err("Queued message identity or delivery state changed".into());
            }
        }
        let author = from
            .unwrap_or(&self.settings.presence_identity.name)
            .to_owned();
        let ws = self
            .ai
            .config
            .valid_workspace()
            .ok_or("The AI workspace is unavailable")?
            .to_path_buf();
        let workbook = self.tab().path.clone();
        let tab = self.tab().id;
        let history = self.presence.conversations.edit(who)?.replay();
        let images = {
            let c = self.presence.conversations.edit(who)?;
            queued
                .and_then(|q| c.message_images.get(q))
                .unwrap_or(&c.draft_images)
                .clone()
        };
        let pending_proposals = self.presence.has_pending(who);
        let previous = self
            .presence
            .agents
            .iter()
            .find(|a| a.profile.id == who && a.tab == tab && a.departure.is_none())
            .ok_or("Collaborator is not on this board")?;
        if previous.inflight || previous.board_action_active() || pending_proposals {
            return Err("This collaborator is busy or awaiting a proposal decision".into());
        }
        let previous_request = previous.request.clone();
        self.release_presence_portal_focus(&previous_request);
        self.agent_atlas_clear_observer(&previous_request);
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who && a.tab == tab && a.departure.is_none())
            .ok_or("Collaborator is not on this board")?;
        if a.inflight || a.board_action_active() || pending_proposals {
            return Err("This collaborator is busy or awaiting a proposal decision".into());
        }
        let grant = a
            .grant
            .clone()
            .ok_or("Board grant was revoked; restore a fresh session")?;
        let resume_after_stop =
            from.is_none() && queued.is_none() && a.paused && a.activity == "cancel";
        let request = queued
            .map(str::to_owned)
            .unwrap_or_else(atlas_agent::request_id);
        self.presence
            .links
            .set_session(&a.dir, a.binding.clone(), grant, request.clone())
            .map_err(str::to_owned)?;
        self.presence
            .roster
            .session_mut(&a.motion)
            .ok_or("Session has retired")?
            .begin_request(id(&request)?)
            .map_err(|e| e.to_string())?;
        a.reply_from = if provider == "codex" && a.runtime.is_none() {
            history.len()
        } else {
            a.snapshot.as_ref().map_or(history.len(), |s| s.turns.len())
        };
        let req = atlas_agent::AgentRequest {
            id: request.clone(),
            prompt: text.trim().into(),
            at: atlas_ai::context::now_secs(),
            collaborator: Some(a.binding.clone()),
            relayed_from: from.map(str::to_owned),
            inputs: attachments::image_inputs(&images),
            history: if provider == "codex" || a.snapshot.is_none() {
                history
            } else {
                Vec::new()
            },
            model: choice.model,
            model_parameters: choice.parameters,
            ..Default::default()
        };
        a.inflight = true;
        a.chat_only = a.paused;
        a.request = request.clone();
        a.helper_outcome = None;
        a.sequences = 0;
        a.calls = 0;
        a.seen_running = false;
        a.last_event = None;
        let delivered = (|| -> Result<(), String> {
            if a.profile.provider == "codex" {
                let runtime = a.runtime.get_or_insert_with(|| {
                    atlas_ai::runtime::CodexLink::start_beside(
                        a.dir.clone(),
                        ws.clone(),
                        "codex".into(),
                        workbook,
                    )
                });
                runtime
                    .send(req)
                    .map_err(|e| format!("Could not send task: {e}"))?;
            } else if a.profile.provider == "cursor" {
                let sender = self.presence.sources.prepare_request(&a.dir, &req);
                let cursor = a.cursor.get_or_insert_with(|| {
                    atlas_ai::sidecar::CursorLink::start(
                        ws.clone(),
                        a.binding.session_id.clone(),
                        ws.clone(),
                        a.dir.clone(),
                    )
                });
                cursor.send(req, sender)?;
            } else {
                self.presence
                    .sources
                    .send(&a.dir, &req)
                    .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if let Err(error) = delivered {
            a.note_helper_outcome("error", &error);
            a.inflight = false;
            a.request.clear();
            self.presence.links.revoke(&a.dir);
            if let Some(m) = self.presence.roster.session_mut(&a.motion) {
                m.cancel_request();
            }
            return Err(error);
        }
        if resume_after_stop {
            a.paused = false;
            a.resume_task = false;
            a.chat_only = false;
            a.control_epoch = a.control_epoch.wrapping_add(1);
        }
        self.presence.sources.expect(&a.dir, &request);
        a.status = if a.profile.provider == "codex" {
            "Processing"
        } else {
            "Waiting for connected sidecar"
        }
        .into();
        a.activity = "reasoning".into();
        a.last_active = Instant::now();
        a.had_response = false;
        if from.is_none() {
            a.relays = 0;
        }
        let dir = a.dir.clone();
        self.note_presence_binding(who, &provider, &dir)?;
        let conversation = self.presence.conversations.edit(who)?;
        if let Some(request) = queued {
            let index = conversation
                .request_index(request)
                .ok_or("Queued message disappeared")?;
            conversation.set_turn_status(index, "sent");
        } else {
            conversation.push(
                "user",
                text.trim(),
                &author,
                Some(&provider),
                &request,
                "sent",
            );
            conversation.set_draft("");
            conversation.set_message_images(&request, images);
            conversation.set_draft_images(Vec::new());
        }
        self.presence.conversations.save(who);
        Ok(())
    }
}
