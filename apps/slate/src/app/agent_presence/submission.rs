use super::*;

impl SlateApp {
    pub(super) fn send_presence_message(&mut self, who: &str, text: &str) -> Result<(), String> {
        self.send_presence_message_from(who, text, None)
    }
    pub(super) fn send_presence_message_from(
        &mut self,
        who: &str,
        text: &str,
        from: Option<&str>,
    ) -> Result<(), String> {
        if text.trim().is_empty() || text.len() > 32_768 {
            return Err("Message is empty or too long".into());
        }
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
        self.presence.conversations.edit(who)?;
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
        if self
            .presence
            .agents
            .iter()
            .any(|a| a.profile.id == who && a.paused)
            && self.presence.task_busy(who)
        {
            self.stop_presence(who, false)?;
        }
        let history = self.presence.conversations.edit(who)?.replay();
        let a = self
            .presence
            .agents
            .iter_mut()
            .find(|a| a.profile.id == who && a.tab == tab && a.departure.is_none())
            .ok_or("Collaborator is not on this board")?;
        if a.inflight || a.work.is_some() || a.proposed.is_some() || a.read.is_some() {
            return Err("This collaborator is busy or awaiting a proposal decision".into());
        }
        let grant = a
            .grant
            .clone()
            .ok_or("Board grant was revoked; restore a fresh session")?;
        let request = atlas_agent::request_id();
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
        a.reply_from = a.snapshot.as_ref().map_or(history.len(), |s| s.turns.len());
        let req = atlas_agent::AgentRequest {
            id: request.clone(),
            prompt: text.trim().into(),
            at: atlas_ai::context::now_secs(),
            collaborator: Some(a.binding.clone()),
            relayed_from: from.map(str::to_owned),
            history: if a.snapshot.is_none() {
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
        a.last_event.clear();
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
        conversation.push(
            "user",
            text.trim(),
            &author,
            Some(&provider),
            &request,
            "sent",
        );
        conversation.set_draft("");
        self.presence.conversations.save(who);
        Ok(())
    }
}
