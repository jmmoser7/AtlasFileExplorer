//! Send a prompt and boot the sidecar.

use super::*;

impl SlateApp {
    pub(crate) fn send_agent_prompt(&mut self, portal: NodeId) -> Option<NodeId> {
        if (self.agents.connection_pending == Some(portal) && !self.agents.connection_background)
            || self.agents.project_picker == Some(portal)
            || self
                .agents
                .chat_picker
                .as_ref()
                .is_some_and(|p| p.portal == portal)
        {
            self.toast("Choose a project and conversation first.");
            return None;
        }
        if self.agents.connection_background && self.agents.connection_pending == Some(portal) {
            if !self.queue_agent_send(portal) {
                self.toast("Conversation is refreshing; send again in a moment.");
            }
            return None;
        }
        let Some((_session, provider)) = self.agent_session_for(portal) else {
            self.toast("Select an agent portal first.");
            return None;
        };
        if self.is_text_block(portal) {
            self.queue_text_block(portal);
            return None;
        }
        if !atlas_ai::agent::local_image_engine(&provider)
            && matches!(
                self.agents.awaiting.get(&portal),
                Some(
                    AgentAwait::Sent { .. }
                        | AgentAwait::Thinking { .. }
                        | AgentAwait::Responding { .. }
                )
            )
        {
            self.toast("Wait for this response before sending another prompt.");
            return None;
        }
        if provider.is_empty() {
            self.toast("Choose a program first.");
            return None;
        }
        if atlas_ai::agent::local_image_engine(&provider) {
            self.queue_generation(portal);
            return None;
        }
        if self.agent_linear(portal) {
            let selected = self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.channel.as_deref());
            if selected.is_some_and(|id| {
                self.agents
                    .session(portal)
                    .is_none_or(|s| s.conversation != id)
            }) && self.ai.config.valid_workspace().is_some()
            {
                if !self.queue_agent_send(portal) {
                    self.toast(life::NOT_CONNECTED);
                }
                return None;
            }
        }
        if provider == "ollama"
            && self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.model.is_none())
        {
            self.toast("Choose an installed local model in the card title before sending.");
            return None;
        }
        let inputs = match self.agent_input_snapshot(portal) {
            Ok(v) => v,
            Err(e) => {
                self.fail_agent_await(portal, e);
                return None;
            }
        };
        let mut prompt = self.agents.prompt_mut(portal).trim().to_string();
        if prompt.is_empty() {
            prompt = inputs
                .wired
                .iter()
                .map(|v| v.text.as_str())
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join("\n\n");
        }

        if prompt.is_empty() {
            prompt = self
                .agent_images(portal)
                .first()
                .map(|i| i.prompt.clone())
                .unwrap_or_default();
        }
        if prompt.is_empty() {
            self.toast("Type a prompt or connect a text node first.");
            return None;
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.fail_agent_await(
                portal,
                "Set an AI workspace before sending — the agent link has nowhere to write.".into(),
            );
            self.toast("Set an AI workspace before sending an agent prompt.");
            #[cfg(not(test))]
            self.ai.pick_workspace();
            return None;
        };
        let composer = portal;
        let typing_here = self.agents.composing(composer);
        let (portal, history) = self.prepare_agent_train_send(portal, &ws)?;
        self.consume_agent_context(composer);
        // The next message is typed on the new tail. A send that waited to
        // connect leaves the caret wherever the person has moved since.
        if typing_here && portal != composer {
            self.agents.composer_editing = None;
            self.agents.composer_focus = Some(portal);
            self.agent_focus(portal);
        }
        let (session, provider) = self.agent_session_for(portal)?;
        let req = AgentRequest {
            model: self
                .doc()
                .scene
                .node(portal)
                .and_then(slate_doc::agent_chat::agent)
                .and_then(|a| a.model.clone()),
            history,
            inputs,
            id: atlas_ai::agent::request_id(),
            prompt,
            at: atlas_ai::context::now_secs(),
            image: None,
            output_dir: self.agent_output_dir(portal, &ws, &session),
            oneshot: false,
            policy: self.crosstalk_policy(&session),
            relayed_from: self.agents.crosstalk.relaying.clone(),
            goal: self.crosstalk_goal(&session),
        };
        let dir = self
            .agent_link_dir(portal, &ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &session));
        let manifest = slate_doc::SourceUri {
            locator: super::super::board_portal::source_locator(
                self.tab().path.as_deref(),
                &dir.join("session.json"),
            ),
        };
        self.patch_nodes(&[portal], |n| {
            if let Some(a) = slate_doc::agent_chat::agent_mut(n) {
                if a.bundle.is_none() {
                    a.bundle = Some(manifest.clone());
                }
            }
        });
        self.agents.requests.insert(portal, req.id.clone());
        self.agents.bindings.insert(portal, session.clone());
        let mut sent = None;
        match self.agents.sources.send(&dir, &req) {
            Ok(()) => {
                sent = Some(portal);
                let turn = AgentTurn {
                    role: "user".into(),
                    text: req.prompt.clone(),
                    at: req.at,
                };
                if !self.agents.local_turns.contains_key(&portal) {
                    // A linear provider's request carries no history, and a new
                    // tail has no session yet: the stream the composer shows is
                    // what the whole train keeps displaying.
                    let same_stream = self
                        .doc()
                        .scene
                        .node(composer)
                        .and_then(slate_doc::agent_chat::agent)
                        .is_some_and(|a| a.session == session);
                    let seed = self
                        .agents
                        .sessions
                        .get(&portal)
                        .map(|s| s.turns.clone())
                        .or_else(|| {
                            same_stream
                                .then(|| self.agent_all_turns(composer))
                                .filter(|t| !t.is_empty())
                        })
                        .unwrap_or_else(|| req.history.clone());
                    self.agents.local_turns.insert(portal, seed);
                }
                let turns = self.agents.local_turns.get_mut(&portal).unwrap();
                if turns
                    .last()
                    .is_none_or(|t| t.role != "user" || t.text != turn.text)
                {
                    turns.push(turn);
                }
                let pending_turns = turns.clone();
                let siblings: Vec<_> = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .filter_map(|n| {
                        slate_doc::agent_chat::agent(n)
                            .filter(|a| a.session == session && n.id != portal)
                            .map(|_| n.id)
                    })
                    .collect();
                for id in siblings {
                    self.agents.bindings.insert(id, session.clone());
                    self.agents.local_turns.insert(id, pending_turns.clone());
                }
                self.agents.prompt_mut(portal).clear();
                self.agents.prompt_mut(composer).clear();
                self.agents.awaiting.insert(
                    portal,
                    AgentAwait::Sent {
                        at: Instant::now(),
                        req_at: req.at,
                    },
                );
                if provider == "codex" || provider == "ollama" || provider.starts_with("ollama/") {
                    // No provider runs in tests; the request is recorded instead.
                    #[cfg(test)]
                    self.agents.dispatched.push((portal, req.clone()));
                    #[cfg(not(test))]
                    {
                        let cwd = self.agent_folder_for(portal).unwrap_or_else(|| ws.clone());
                        let workbook = self.tab().path.clone();
                        let runtime =
                            self.agents.codex.entry(session.clone()).or_insert_with(|| {
                                atlas_ai::runtime::CodexLink::start_beside(
                                    dir.clone(),
                                    cwd,
                                    provider.clone(),
                                    workbook,
                                )
                            });
                        if let Err(error) = runtime.send(req.clone()) {
                            self.fail_agent_await(portal, error);
                        }
                    }
                } else {
                    self.ensure_agent_sidecar(portal, &session, &provider, &ws);
                }
            }
            Err(e) => {
                self.fail_agent_await(portal, format!("Could not write agent request: {e}"));
                self.toast(format!("Could not write agent request: {e}"));
            }
        }
        sent
    }
    fn ensure_agent_sidecar(
        &mut self,
        portal: NodeId,
        session: &str,
        provider: &str,
        ws: &std::path::Path,
    ) {
        if provider != "cursor" {
            return;
        } // configured external sidecars consume request.json
        if self.agents.sidecar_restart.remove(session) {
            self.detach_cursor_sidecar(session);
        }
        if self.agents.sidecar_spawned.contains(session)
            || self.agents.sidecar_booting.contains(session)
        {
            return;
        }
        let dir = self
            .agent_link_dir(portal, ws)
            .unwrap_or_else(|| atlas_ai::agent::agent_dir(ws, session));
        #[cfg(test)]
        self.record_sidecar_dir(session, dir);
        #[cfg(not(test))]
        {
            self.record_sidecar_dir(session, dir.clone());
            let cwd = self
                .agent_folder_for(portal)
                .unwrap_or_else(|| ws.to_path_buf());
            let ws = ws.to_path_buf();
            let policy = atlas_ai::sidecar::SidecarPolicy {
                read_only: self.crosstalk_policy(session) == atlas_agent::TurnPolicy::ReadOnly,
            };
            let session = session.to_string();
            let doc = self.tab().id;
            self.agents.sidecar_booting.insert(session.clone());
            let tx = self.sidecar_boot_tx();
            std::thread::spawn(move || {
                // A sidecar left by an earlier run must not write the same link folder.
                let _ = atlas_ai::sidecar::stop(&dir);
                let result =
                    atlas_ai::sidecar::spawn_cursor_sidecar_with(&ws, &session, &cwd, &dir, policy);
                let _ = tx.send(SidecarBoot {
                    doc,
                    portal,
                    session,
                    result,
                });
            });
        }
    }
    #[cfg(not(test))]
    pub(super) fn sidecar_boot_tx(&mut self) -> Sender<SidecarBoot> {
        if let Some(tx) = &self.agents.sidecar_boot_tx {
            return tx.clone();
        }
        let (tx, rx) = unbounded();
        self.agents.sidecar_boot_rx = Some(rx);
        self.agents.sidecar_boot_tx = Some(tx.clone());
        tx
    }
    pub(super) fn pump_sidecar_boot(&mut self, ctx: &egui::Context) {
        loop {
            let msg = {
                let Some(rx) = &self.agents.sidecar_boot_rx else {
                    return;
                };
                match rx.try_recv() {
                    Ok(msg) => msg,
                    Err(crossbeam_channel::TryRecvError::Empty) => return,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        self.agents.sidecar_boot_rx = None;
                        self.agents.sidecar_boot_tx = None;
                        return;
                    }
                }
            };
            self.agents.sidecar_booting.remove(&msg.session);
            match msg.result {
                // Its document closed while it booted.
                Ok(mut child) if !self.agent_session_open(&msg.session) => {
                    let _ = child.kill();
                    let _ = child.try_wait();
                    self.forget_sidecar_dir(&msg.session);
                }
                Ok(child) => {
                    self.agents.sidecar_spawned.insert(msg.session.clone());
                    self.agents.sidecar_child.insert(msg.session, child);
                }
                Err(e) => self.fail_agent_await_in(msg.doc, msg.portal, e),
            }
            ctx.request_repaint();
        }
    }
    pub(crate) fn fail_agent_await(&mut self, portal: NodeId, reason: String) {
        let (reason, actions) = classify_agent_failure(reason);
        self.agents.awaiting.insert(
            portal,
            AgentAwait::Failed {
                reason: reason.clone(),
                actions,
            },
        );
        let turns = self.agents.local_turns.entry(portal).or_insert_with(|| {
            self.agents
                .sessions
                .get(&portal)
                .map(|s| s.turns.clone())
                .unwrap_or_default()
        });
        if turns
            .last()
            .is_none_or(|t| t.role != "system" || t.text != reason)
        {
            turns.push(AgentTurn {
                role: "system".into(),
                text: reason,
                at: atlas_ai::context::now_secs(),
            });
        }
    }
    pub(super) fn apply_agent_recover(&mut self, portal: NodeId, action: &AgentRecover) {
        match action {
            AgentRecover::OpenUrl { url, .. } => self.open_url(url),
            AgentRecover::OpenPath { path, .. } => Self::open_path(path),
            AgentRecover::PasteKey => {
                self.agents.key_entry = Some(portal);
                self.agents.key_focus = Some(portal);
                self.agents.key_draft.clear();
            }
            AgentRecover::PickWorkspace => {
                #[cfg(not(test))]
                self.ai.pick_workspace();
            }
        }
    }
    pub(crate) fn open_cursor_api_key_page(&mut self) -> bool {
        self.open_url(atlas_ai::cursor_key::DASHBOARD_URL);
        true
    }
    pub(crate) fn save_cursor_api_key(&mut self, portal: Option<NodeId>) -> bool {
        let key = self.agents.key_draft.trim().to_string();
        match atlas_ai::cursor_key::save(&key) {
            Ok(()) => {
                self.agents.key_draft.clear();
                self.agents.key_entry = None;
                self.agents.key_focus = None;
                self.agents.key_rect = None;
                if let Some(id) = portal.or(self.selected_agent_portal()) {
                    if let Some((session, _)) = self.agent_session_for(id) {
                        self.agents.sidecar_spawned.remove(&session);
                        self.agents.sidecar_child.remove(&session);
                        self.agents.sidecar_booting.remove(&session);
                    }
                    self.mark_agent_operative(id);
                    let retry = self
                        .visible_agent_turns(id)
                        .iter()
                        .any(|t| t.role == "user");
                    if retry {
                        self.retry_last_agent_prompt(id);
                        self.toast("API key saved. Sending the last message.");
                    } else {
                        self.toast("API key saved. This agent is ready.");
                    }
                } else {
                    self.toast("API key saved on this machine.");
                }
                true
            }
            Err(e) => {
                self.toast(e);
                false
            }
        }
    }
    /// A saved key ends the failure. The card is idle and the composer is ready.
    fn mark_agent_operative(&mut self, id: NodeId) {
        self.agents.awaiting.remove(&id);
        if let Some(turns) = self.agents.local_turns.get_mut(&id) {
            turns.retain(|t| t.role != "system");
        }
        if self
            .agents
            .local_turns
            .get(&id)
            .is_some_and(|t| t.is_empty())
        {
            self.agents.local_turns.remove(&id);
        }
        if let Some(existing) = self.agents.sessions.get(&id).cloned() {
            if matches!(existing.status, AgentStatus::Error(_)) {
                let mut idle = (*existing).clone();
                idle.status = AgentStatus::Idle;
                if let Some(ws) = self.ai.config.workspace_dir.clone() {
                    if let Some(dir) = self.agent_link_dir(id, &ws) {
                        let _ =
                            atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &idle);
                    }
                }
                self.agents.sessions.insert(id, std::sync::Arc::new(idle));
            }
        }
        self.agents.composer_focus = Some(id);
        self.agent_focus(id);
    }
    fn retry_last_agent_prompt(&mut self, portal: NodeId) {
        let text = self
            .visible_agent_turns(portal)
            .iter()
            .rev()
            .find(|t| t.role == "user")
            .map(|t| t.text.clone());
        if let Some(text) = text {
            *self.agents.prompt_mut(portal) = text;
            self.send_agent_prompt(portal);
        }
    }
}
