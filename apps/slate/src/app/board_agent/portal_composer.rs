//! Key entry, composer, stop, and channel load.

use super::*;

impl SlateApp {
    pub(super) fn paint_agent_key_entry(&mut self, ui: &egui::Ui, id: NodeId, strip: Rect, z: f32) {
        self.agents.key_rect = Some((id, strip));
        let mut key_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("agent-key", id.0)),
            egui::UiBuilder::new()
                .layer_id(egui::LayerId::new(
                    egui::Order::Foreground,
                    Id::new(("agent-key-layer", id.0)),
                ))
                .max_rect(strip),
        );
        key_ui.set_clip_rect(strip.intersect(ui.clip_rect()));
        key_ui.style_mut().visuals = self.palette().visuals();
        key_ui.style_mut().override_font_id = Some(FontId::proportional(14.0 * z));
        let focus_id = Id::new(("agent-key-field", id.0));
        let mut save = false;
        let mut focused = false;
        key_ui.horizontal(|key_ui| {
            let response = key_ui.add(
                egui::TextEdit::singleline(&mut self.agents.key_draft)
                    .id(focus_id)
                    .password(true)
                    .hint_text("Paste API key")
                    .desired_width((strip.width() - 96.0 * z).max(40.0)),
            );
            focused = response.has_focus();
            if self.agents.key_focus == Some(id) {
                response.request_focus();
            }
            if key_ui.button("Save").clicked() {
                save = true;
            }
        });
        if self.agents.key_focus == Some(id) && focused {
            self.agents.key_focus = None;
        }
        if save
            || (focused && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)))
        {
            self.save_cursor_api_key(Some(id));
        }
    }
    pub(crate) fn paint_agent_composer(
        &mut self,
        ui: &egui::Ui,
        id: NodeId,
        field: Rect,
        z: f32,
        minimal: bool,
    ) {
        if !minimal && self.agent_has_child(id) {
            return;
        }
        self.agents
            .composer_rects
            .insert(id, field.intersect(ui.clip_rect()));
        let take_focus = self.agents.composer_focus == Some(id);
        let editing = self.agents.composer_editing == Some(id) || take_focus;
        if take_focus {
            self.web_release_keyboard();
        }
        let mut text = self.agents.prompt_mut(id).clone();
        let field_out = atlas_shell::selection_tools::prompt_field(
            ui,
            Id::new(("agent-composer", id.0)),
            field,
            z,
            &mut text,
            self.composer_hint(id),
            editing,
            take_focus,
            self.palette(),
        );
        if !editing {
            if self.agent_connecting(id) {
                self.paint_agent_connecting(ui, field, z);
            }
            return;
        }
        if take_focus && field_out.focused {
            self.agents.composer_focus = None;
        }
        if field_out.focused {
            self.agents.composer_editing = Some(id);
            self.agent_focus(id);
        } else if self.agents.composer_editing == Some(id) {
            self.agents.composer_editing = None;
        }
        if field_out.changed {
            *self.agents.prompt_mut(id) = text;
            self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
        }
        if field_out.submit {
            self.send_agent_prompt(id);
        }
        if self.agent_connecting(id) {
            self.paint_agent_connecting(ui, field, z);
        }
    }
    /// Stop takes the output circle's place while a reply streams: the card
    /// grows downward, so it stays under the pointer.
    pub(super) fn paint_agent_stop(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        n: &Node,
        pointer: Option<Pos2>,
    ) {
        let z = xf.z;
        let at = output_circle_center(xf.rect_w2s(n.rect), z);
        let palette = self.palette();
        let hot = pointer.is_some_and(|p| p.distance(at) <= canvas_scale::px(STOP_REACH, z));
        let side = canvas_scale::px(STOP_SIDE, z);
        painter.rect_filled(
            Rect::from_center_size(at, egui::vec2(side, side)),
            canvas_scale::px(1.5, z),
            palette.sub.gamma_multiply(if hot { 0.95 } else { 0.7 }),
        );
    }
    pub(super) fn visible_agent_turns(&self, id: NodeId) -> Vec<AgentTurn> {
        let turns = self
            .agents
            .local_turns
            .get(&id)
            .or_else(|| self.agents.sessions.get(&id).map(|s| &s.turns));
        let view = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| &a.chat);
        let start = self
            .doc()
            .scene
            .node(id)
            .and_then(|n| slate_doc::agent_chat::bundle_entry(&self.doc().scene, n))
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| a.chat.start)
            .unwrap_or(0);
        let end = view.and_then(|v| v.end);
        let Some(v) = turns else {
            return Vec::new();
        };
        let window: Vec<_> = v
            .iter()
            .take(end.unwrap_or(v.len()))
            .skip(start.min(v.len()))
            .cloned()
            .collect();
        window
            .into_iter()
            .map(|mut turn| {
                if turn.role == "user" {
                    turn.text = atlas_ai::agent::display_prompt(&turn.text).into();
                }
                turn
            })
            .collect()
    }
    pub(super) fn portal_has_new_reply(&self, id: NodeId, req_at: u64) -> bool {
        let local = self
            .agents
            .local_turns
            .get(&id)
            .map(|t| t.as_slice())
            .unwrap_or(&[]);
        let session = self
            .agents
            .sessions
            .get(&id)
            .map(|s| s.turns.as_slice())
            .unwrap_or(&[]);
        await_new_reply(local, req_at) || await_new_reply(session, req_at)
    }
    pub(crate) fn agent_failure_reason(&self, id: NodeId) -> Option<&str> {
        match self.agents.awaiting.get(&id) {
            Some(AgentAwait::Failed { reason, .. }) => Some(reason.as_str()),
            _ => None,
        }
    }
    #[cfg(test)]
    pub(crate) fn agent_is_awaiting(&self, id: NodeId) -> bool {
        matches!(
            self.agents.awaiting.get(&id),
            Some(
                AgentAwait::Sent { .. }
                    | AgentAwait::Thinking { .. }
                    | AgentAwait::Responding { .. }
            )
        )
    }
    pub(crate) fn open_agent_chat_picker(&mut self, portal: NodeId) {
        let Some(folder) = self.agent_folder_for(portal) else {
            self.toast("Bind a project folder before switching chats.");
            return;
        };
        self.agent_focus(portal);
        let provider = self
            .agent_session_for(portal)
            .map(|v| v.1)
            .unwrap_or_default();
        let cached = chat_key(&folder);
        self.agents
            .chats
            .remove(&(provider.clone(), cached.clone()));
        self.agents.chats_inflight.remove(&cached);
        self.request_agent_chats(folder, &provider);
        self.agents.pending_chat_pick = Some(portal);
    }
    pub(crate) fn dismiss_agent_picker(&mut self) -> bool {
        {
            let artifact = self.agents.artifact_popup.take().is_some();
            self.agents.artifact_popup_rect = None;
            self.agents.chat_picker.take().is_some() || artifact
        }
    }
    #[cfg(test)]
    pub(crate) fn present_agent_picker(&mut self, portal: NodeId, chats: Vec<CursorChat>) {
        self.finish_chat_pick(portal, PathBuf::from("test-agents"), chats);
    }
    #[cfg(test)]
    pub(crate) fn agent_picker_titles(&self) -> Option<Vec<String>> {
        self.agents
            .chat_picker
            .as_ref()
            .map(|p| p.chats.iter().map(|c| c.title.clone()).collect())
    }
    #[cfg(test)]
    pub(crate) fn pick_agent_from_list(&mut self, portal: NodeId, id: &str) -> bool {
        let found = self
            .agents
            .chat_picker
            .as_ref()
            .is_some_and(|p| p.portal == portal && p.chats.iter().any(|c| c.id == id));
        if !found {
            return false;
        }
        self.set_agent_channel(portal, Some(id.to_string()));
        self.agents.chat_picker = None;
        true
    }
    pub(crate) fn answer_agent_approval(&mut self, detail: Option<&str>) {
        let Some((raw, allow)) = detail.and_then(|s| serde_json::from_str::<(u64, bool)>(s).ok())
        else {
            return;
        };
        let node = NodeId(raw);
        let Some(approval) = self.agents.session(node).and_then(|s| s.approval.clone()) else {
            return;
        };
        let Some(ws) = self.ai.config.workspace_dir.as_ref() else {
            return;
        };
        let Some(dir) = self.agent_link_dir(node, ws) else {
            return;
        };
        std::thread::spawn(move || {
            let _ = atlas_ai::agent::atomic_write_json(
                &dir.join("approval.json"),
                &serde_json::json!({"id":approval.id,"allow":allow}),
            );
        });
    }
    pub(crate) fn set_agent_channel(&mut self, portal: NodeId, channel: Option<String>) {
        if self.agent_linear(portal)
            && self.agents.session(portal).is_some_and(|s| {
                !s.conversation.is_empty() && Some(s.conversation.as_str()) != channel.as_deref()
            })
        {
            self.toast(
                "Create a new agent portal for another conversation; this train keeps its history.",
            );
            return;
        }
        if self.agents.connection_rx.is_some() {
            self.toast("Wait for the selected conversation to finish loading.");
            return;
        }
        if self.agent_is_running(portal) {
            self.toast("Stop this response before switching conversations.");
            return;
        }
        let selected = channel.clone();
        let provider = self
            .agent_session_for(portal)
            .map(|(_, provider)| provider)
            .unwrap_or_default();
        if provider == "cursor" {
            if let Some((session, _)) = self.agent_session_for(portal) {
                self.detach_cursor_sidecar(&session);
            }
        }
        self.patch_nodes(&[portal], move |node| {
            if let Some(agent) = slate_doc::agent_chat::agent_mut(node) {
                agent.channel = channel.clone();
            }
        });
        if let Some(channel) = selected {
            #[cfg(not(test))]
            self.load_agent_connection(portal, channel, false);
            #[cfg(test)]
            let _ = channel;
        } else {
            if provider == "cursor" {
                self.forget_owned_thread(portal, "cursor-agent.txt");
            } else if provider == "codex" {
                if let Some((session, _)) = self.agent_session_for(portal) {
                    self.agents.codex.remove(&session);
                }
                self.forget_owned_thread(portal, "codex-thread.txt");
            }
            self.agents.composer_focus = Some(portal);
        }
    }
    /// Read the provider's copy of the conversation off-thread. A background
    /// load refreshes a conversation already on the board and may only add
    /// to it (`atlas_ai::agent::reloaded`).
    pub(super) fn load_agent_connection(
        &mut self,
        portal: NodeId,
        channel: String,
        background: bool,
    ) {
        let Some((session, provider)) = self.agent_session_for(portal) else {
            return;
        };
        let Some(cwd) = self.agent_folder_for(portal) else {
            return;
        };
        let Some(ws) = self.ai.config.workspace_dir.clone() else {
            self.toast("Set an AI workspace to cache this conversation.");
            return;
        };
        let Some(dir) = self.agent_link_dir(portal, &ws) else {
            return;
        };
        if self.agents.connection_rx.is_some() {
            self.toast("A conversation is already loading.");
            return;
        }
        let (tx, rx) = unbounded();
        self.agents.connection_rx = Some(rx);
        self.agents.connection_pending = Some(portal);
        self.agents.connection_background = background;
        self.agents.connection_tick = Some(Instant::now());
        #[cfg(test)]
        {
            let _ = (&provider, &cwd);
            if self.agents.life.hold_loads {
                self.agents.life.loads.push(life::TestLoad {
                    portal,
                    session,
                    channel,
                    dir,
                    tx,
                });
            } else {
                let _ = tx.send((portal, session, Err("No provider runs in tests.".into())));
            }
        }
        #[cfg(not(test))]
        std::thread::spawn(move || {
            let result =
                atlas_ai::runtime::read_conversation(&provider, &cwd, &channel).and_then(|state| {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    if provider == "codex" {
                        std::fs::write(dir.join("codex-thread.txt"), &channel)
                            .map_err(|e| e.to_string())?;
                    } else if provider == "cursor" {
                        std::fs::write(dir.join("cursor-agent.txt"), &channel)
                            .map_err(|e| e.to_string())?;
                    }
                    if background {
                        return atlas_ai::agent::store_reloaded(&dir, state);
                    }
                    atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &state)
                        .map_err(|e| e.to_string())?;
                    Ok(state)
                });
            let _ = tx.send((portal, session, result));
        });
    }
}
