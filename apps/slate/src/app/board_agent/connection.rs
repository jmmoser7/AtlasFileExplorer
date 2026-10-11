//! Connection pump, fork payload, and chat pump.

use super::*;

impl SlateApp {
    pub(super) fn pump_agent_connection(&mut self, ctx: &egui::Context) {
        let Some(result) = self
            .agents
            .connection_rx
            .as_ref()
            .and_then(|rx| rx.try_recv().ok())
        else {
            return;
        };
        self.agents.connection_rx = None;
        self.agents.connection_pending = None;
        let background = std::mem::take(&mut self.agents.connection_background);
        let (id, session, result) = result;
        if !self.agent_session_for(id).is_some_and(|v| v.0 == session) {
            return;
        }
        let failure = result.as_ref().err().cloned();
        let result = match (result, self.agents.sessions.get(&id).cloned()) {
            (Ok(state), Some(old)) if background => {
                let mut shown = (*old).clone();
                shown.turns = self.agent_all_turns(id);
                match atlas_ai::agent::reloaded(&shown, state) {
                    Some(next) => Ok(next),
                    None => {
                        self.settle_agent_connecting(id, &session, None);
                        return;
                    }
                }
            }
            (result, _) => result,
        };
        match result {
            Ok(state) => {
                self.agents.bindings.insert(id, session.clone());
                self.agents.local_turns.remove(&id);
                let changed = self.agents.session(id).is_none_or(|old| {
                    old.turns != state.turns
                        || old.artifacts != state.artifacts
                        || old.status != state.status
                });
                self.agents.sessions.insert(id, std::sync::Arc::new(state));
                if changed {
                    self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
                }
                if !background {
                    self.bundle_agent_history(ctx, id);
                }
            }
            Err(_) if background => {}
            Err(error) => self.fail_agent_await(
                id,
                format!("Could not connect to this conversation: {error}"),
            ),
        }
        self.settle_agent_connecting(id, &session, failure);
        ctx.request_repaint();
    }
    pub(super) fn finish_chat_pick(
        &mut self,
        portal: NodeId,
        path: PathBuf,
        chats: Vec<CursorChat>,
    ) {
        self.agents.pending_chat_pick = None;
        let provider = self
            .agent_session_for(portal)
            .map(|v| v.1)
            .unwrap_or_default();
        self.agents
            .chats
            .insert((provider, path.clone()), chats.clone());
        self.agents.chat_picker = Some(ChatPicker {
            portal,
            folder: path,
            chats,
        });
    }
    /// Pasting a copied chat train seeds a new linked source per conversation
    /// session; provider handles are not copied.
    pub(crate) fn fork_agent_train_payload(&mut self, payload: &mut [Node]) -> bool {
        use std::collections::HashMap;
        if !slate_doc::agent_chat::paste_is_train_fork(payload) {
            return false;
        }
        let Some(ws) = self.ai.config.valid_workspace().map(|p| p.to_path_buf()) else {
            self.toast("Set an AI workspace before pasting a chat train.");
            return false;
        };
        let base = self.tab().path.as_deref();
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, node) in payload.iter().enumerate() {
            let Some(a) = slate_doc::agent_chat::agent(node) else {
                continue;
            };
            if !a.chat.train {
                continue;
            }
            groups.entry(a.session.clone()).or_default().push(index);
        }
        for (_old_session, indices) in groups {
            // A copied root forks its source. A coding card pasted back onto
            // its host is another branch: its own session, seeded through the
            // fork point. A model card keeps its session until send.
            let mut rooted = Vec::new();
            let mut linear_forks = Vec::new();
            for index in indices {
                let external = slate_doc::agent_chat::agent(&payload[index])
                    .and_then(|a| a.chat.parent)
                    .is_some_and(|parent| payload.iter().all(|n| n.id != parent));
                if !external {
                    rooted.push(index);
                    continue;
                }
                let linear = slate_doc::agent_chat::agent(&payload[index]).is_some_and(|a| {
                    a.chat.linear || atlas_ai::runtime::linear_provider(&a.provider)
                });
                if linear {
                    linear_forks.push(index);
                }
            }
            if let Some(agent) = rooted
                .first()
                .and_then(|&index| slate_doc::agent_chat::agent(&payload[index]).cloned())
            {
                let turns = self.fork_source_turns(&agent, base).unwrap_or_default();
                self.write_forked_session(payload, &rooted, &agent, turns, &ws, base);
            }
            for index in linear_forks {
                let Some(agent) = slate_doc::agent_chat::agent(&payload[index]).cloned() else {
                    continue;
                };
                let turns = self.fork_source_turns(&agent, base).unwrap_or_default();
                let seeded = atlas_ai::agent::checkpoint(&turns, agent.chat.end)
                    .map(|turns| turns.to_vec())
                    .unwrap_or(turns);
                self.write_forked_session(payload, &[index], &agent, seeded, &ws, base);
            }
        }
        true
    }
    /// One new session file for a pasted train, the owner of paste-as-fork.
    fn write_forked_session(
        &self,
        payload: &mut [Node],
        indices: &[usize],
        agent: &slate_doc::scene::AgentPortalRef,
        turns: Vec<AgentTurn>,
        ws: &std::path::Path,
        base: Option<&std::path::Path>,
    ) {
        if indices.is_empty() {
            return;
        }
        let new_session = slate_doc::scene::new_agent_session_id();
        let dir = atlas_ai::agent::agent_dir(ws, &new_session);
        let _ = std::fs::create_dir_all(&dir);
        let session = atlas_ai::agent::AgentSession {
            usage: None,
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: atlas_ai::agent::AgentStatus::Idle,
            provider: agent.provider.clone(),
            turns,
            updated_at: atlas_ai::context::now_secs(),
            bundle: Default::default(),
            request: String::new(),
        };
        let _ = atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &session);
        let manifest = slate_doc::SourceUri {
            locator: super::super::board_portal::source_locator(base, &dir.join("session.json")),
        };
        for &index in indices {
            let node = &mut payload[index];
            if let Some(a) = slate_doc::agent_chat::agent_mut(node) {
                a.session = new_session.clone();
                a.channel = None;
                a.bundle = Some(manifest.clone());
            }
            if slate_doc::agent_chat::agent(node).is_some_and(|a| a.chat.parent.is_none()) {
                if let NodeKind::Portal(p) = &mut node.kind {
                    if !p.title.contains("Forked from") {
                        let name = p.title.trim();
                        p.title = if name.is_empty() {
                            "Forked from conversation · replayed".into()
                        } else {
                            format!("Forked from {name} · replayed")
                        };
                    }
                }
            }
        }
    }
    /// A pasted chat whose parent stayed on the board is another fork of that host.
    pub(crate) fn keep_external_chat_hosts(&self, nodes: &mut [Node], sources: &[Node]) {
        for (node, src) in nodes.iter_mut().zip(sources.iter()) {
            let Some(parent) = slate_doc::agent_chat::agent(src).and_then(|a| a.chat.parent) else {
                continue;
            };
            if sources.iter().any(|n| n.id == parent) || self.doc().scene.node(parent).is_none() {
                continue;
            }
            if let Some(a) = slate_doc::agent_chat::agent_mut(node) {
                a.chat.parent = Some(parent);
            }
        }
    }
    fn fork_source_turns(
        &self,
        agent: &slate_doc::scene::AgentPortalRef,
        base: Option<&std::path::Path>,
    ) -> Option<Vec<AgentTurn>> {
        let path = agent
            .bundle
            .as_ref()
            .map(|uri| resolve_source(base, &uri.locator))?;
        let text = std::fs::read_to_string(path).ok()?;
        let session: atlas_ai::agent::AgentSession = serde_json::from_str(&text).ok()?;
        Some(session.turns)
    }
    /// The shared source resolver owns path semantics; the session manifest owns
    /// the link directory, including after a global AI workspace change.
    pub(super) fn agent_link_dir(
        &self,
        portal: NodeId,
        workspace: &std::path::Path,
    ) -> Option<PathBuf> {
        let agent = slate_doc::agent_chat::agent(self.doc().scene.node(portal)?)?;
        if let Some(uri) = &agent.bundle {
            return resolve_source(self.tab().path.as_deref(), &uri.locator)
                .parent()
                .map(PathBuf::from);
        }
        (!workspace.as_os_str().is_empty())
            .then(|| atlas_ai::agent::agent_dir(workspace, &agent.session))
    }
    pub(super) fn agent_folder_for(&self, portal: NodeId) -> Option<PathBuf> {
        let node = self.doc().scene.node(portal)?;
        let NodeKind::Portal(p) = &node.kind else {
            return None;
        };
        let loc = p.source.as_ref()?.locator.as_str();
        Some(resolve_source(self.tab().path.as_deref(), loc))
    }
    pub(super) fn request_agent_chats(&mut self, folder: PathBuf, provider: &str) {
        let folder = chat_key(&folder);
        if self
            .agents
            .chats
            .contains_key(&(provider.into(), folder.clone()))
        {
            return;
        }
        if self.agents.chats_rx.is_some() {
            return;
        }
        if self
            .agents
            .chats_inflight
            .get(&folder)
            .is_some_and(|t| t.elapsed() < Duration::from_secs(8))
        {
            return;
        }
        self.agents
            .chats_inflight
            .insert(folder.clone(), Instant::now());
        let (tx, rx) = unbounded();
        self.agents.chats_rx = Some(rx);
        let provider = provider.to_string();
        std::thread::spawn(move || {
            let chats = atlas_ai::runtime::conversations(&provider, &folder);
            let _ = tx.send((provider, folder, chats));
        });
    }
    pub(super) fn pump_agent_chats(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.agents.chats_rx else {
            return;
        };
        match rx.try_recv() {
            Ok((provider, path, result)) => {
                self.agents.catalog_error = result.as_ref().err().cloned();
                let chats = result.unwrap_or_default();
                let path = chat_key(&path);
                self.agents.chats_inflight.remove(&path);
                self.agents.chats_rx = None;
                self.agents
                    .chats
                    .insert((provider.clone(), path.clone()), chats.clone());
                if let Some(portal) = self.agents.pending_chat_pick {
                    if self.agent_folder_for(portal).as_ref().map(|p| chat_key(p))
                        == Some(path.clone())
                        && self
                            .agent_session_for(portal)
                            .is_some_and(|v| v.1 == provider)
                    {
                        self.finish_chat_pick(portal, path, chats);
                    } else if let Some(folder) = self.agent_folder_for(portal) {
                        let provider = self
                            .agent_session_for(portal)
                            .map(|v| v.1)
                            .unwrap_or_default();
                        self.request_agent_chats(folder, &provider);
                    }
                }
                ctx.request_repaint();
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {}
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.agents.chats_rx = None;
            }
        }
    }
}
