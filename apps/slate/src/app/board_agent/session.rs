//! Agent pump and session pump.

use super::*;

impl SlateApp {
    pub(crate) fn agent_pump(&mut self, ctx: &egui::Context) {
        self.ai.packs.begin_frame();
        self.paint_pack_key_entry(ctx);
        self.sync_agent_doc();
        let front_span = atlas_core::session_log::span("slate.agents.front");
        let mut existing: HashSet<String> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.doc.scene.nodes.iter())
            .filter_map(|n| {
                slate_doc::agent_chat::agent(n)
                    .filter(|a| !a.chat.train)
                    .map(|a| a.session.clone())
            })
            .collect();
        existing.extend(
            self.agents
                .awaiting
                .iter()
                .filter(|(_, s)| !matches!(s, AgentAwait::Failed { .. }))
                .filter_map(|(id, _)| self.agent_session_for(*id).map(|(s, _)| s)),
        );
        existing.extend(self.parked_agent_sessions());
        self.agents
            .codex
            .retain(|session, _| existing.contains(session));
        self.ensure_agent_programs();
        self.ensure_agent_recents(ctx);
        self.pump_cursor_ide(ctx);
        self.pump_agent_chats(ctx);
        self.paint_schedule_dialog(ctx);
        self.pump_agent_connection(ctx);
        self.pump_agent_connecting();
        self.rejoin_agent_chats();
        self.refresh_agent_connection();
        self.pump_agent_models();
        if let Some(id) = self.agents.focused {
            if !self.board_sel.contains(&id) && self.portal_chrome.maximized != Some(id) {
                self.agent_blur();
            }
        }
        drop(front_span);
        let ws = self.ai.config.workspace_dir.clone().unwrap_or_default();
        self.pump_agent_sessions(ctx, &ws);
        let tail_span = atlas_core::session_log::span("slate.agents.tail");
        self.pump_agent_awaits(ctx, &ws);
        self.pump_crosstalk(ctx);
        self.pump_comfy_queue();
        self.pump_live_generators();
        if !self.agents.live_settle.is_empty() {
            ctx.request_repaint_after(LIVE_TYPING_SETTLE);
        }
        self.pump_generation_previews(ctx);
        drop(tail_span);

        let _stage_span = atlas_core::session_log::span("slate.agents.stage");
        let proposals = self.agents.stage.poll(&ws);
        if !proposals.is_empty() {
            for proposal in proposals {
                if let Some(existing) = self.agents.pending.iter_mut().find(|p| p.id == proposal.id)
                {
                    *existing = proposal;
                } else {
                    self.agents.pending.push(proposal);
                }
            }
            ctx.request_repaint();
        }
    }
    /// Each card's linked session and published context, once a frame.
    pub(crate) fn pump_agent_sessions(&mut self, ctx: &egui::Context, ws: &std::path::Path) {
        let _sessions_span = atlas_core::session_log::span("slate.agents.sessions");
        let ws = ws.to_path_buf();
        let portals: Vec<(NodeId, Option<slate_doc::scene::AgentPortalRef>)> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::is_agent_node(n))
            .map(|n| (n.id, slate_doc::agent_chat::agent(n).cloned()))
            .collect();
        let live: std::collections::HashSet<NodeId> = portals.iter().map(|(id, _)| *id).collect();
        // Linked sessions can answer from outside Slate (the sidecar writes
        // session.json), so an open board with agents keeps a slow poll.
        if self.agent_background_pending() {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if !self.at_home && !portals.is_empty() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        self.agents.sessions.retain(|id, _| live.contains(id));
        self.relocate_absorbed_drafts(&live);
        self.agents.prompts.retain(|id, _| live.contains(id));
        self.agents.pick_scroll.retain(|id, _| live.contains(id));
        self.agents.awaiting.retain(|id, _| live.contains(id));
        self.agents.local_turns.retain(|id, _| live.contains(id));
        self.agents
            .summary_cache
            .borrow_mut()
            .retain(|id, _| live.contains(id));
        if self.agents.focused.is_some_and(|id| !live.contains(&id)) {
            self.agent_blur();
        }

        let publish = self
            .agents
            .context_tick
            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1));
        if publish {
            self.agents.context_tick = Some(Instant::now());
        }
        let mut published = HashSet::new();
        let mut live_dirs = HashSet::new();
        let mut trains: HashMap<PathBuf, Vec<NodeId>> = HashMap::new();
        if publish {
            for (id, agent) in &portals {
                if let Some(agent) = agent.as_ref().filter(|a| !a.provider.is_empty()) {
                    let dir = self
                        .agent_link_dir(*id, &ws)
                        .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &agent.session));
                    trains.entry(dir).or_default().push(*id);
                }
            }
        }
        for (id, agent) in portals {
            let Some(agent) = agent else {
                continue;
            };
            if agent.provider.is_empty() {
                continue;
            }
            if self.agents.bindings.get(&id) != Some(&agent.session) {
                self.agents.bindings.insert(id, agent.session.clone());
                self.agents.image_cache.borrow_mut().remove(&id);

                self.agents.sessions.remove(&id);
                self.agents.local_turns.remove(&id);
                self.agents.awaiting.remove(&id);
                self.agents.cover_focus.remove(&id);
            }
            let dir = self
                .agent_link_dir(id, &ws)
                .unwrap_or_else(|| atlas_ai::agent::agent_dir(&ws, &agent.session));
            live_dirs.insert(dir.clone());
            if !self.agent_has_child(id) {
                if let Some(raw) = self.agents.sources.take_place(&dir) {
                    self.place_atlas_request(id, &dir, &raw);
                }
                self.agent_output_roots(id, &dir);
            }
            let context = if publish && !ws.as_os_str().is_empty() && published.insert(dir.clone())
            {
                let mut context =
                    self.agent_context_for(&agent.session, &agent.provider, agent.context);
                let single = [id];
                let cards = trains.get(&dir).map_or(&single[..], Vec::as_slice);
                match self.published_train_inputs(cards) {
                    Some(Ok(inputs)) => {
                        context.selection = inputs
                            .context
                            .iter()
                            .map(|item| format!("node:{}", item.node))
                            .collect();
                        context.board_summary = serde_json::to_string(&inputs).unwrap_or_default();
                        Some(context)
                    }
                    Some(Err(_)) => Some(context),
                    None => None,
                }
            } else {
                None
            };
            if let Some(session) = self.agents.sources.poll(&dir, context.as_ref()) {
                if self
                    .agents
                    .sessions
                    .get(&id)
                    .is_some_and(|previous| std::sync::Arc::ptr_eq(previous, &session))
                {
                    continue;
                }
                // A sidecar rewrites unchanged state (at boot, on a repeated
                // error); there is nothing new to lay out.
                if self
                    .agents
                    .sessions
                    .get(&id)
                    .is_some_and(|previous| **previous == *session)
                {
                    self.agents.sessions.insert(id, session);
                    continue;
                }
                if let Some(local) = self.agents.local_turns.get(&id).cloned() {
                    let systems: Vec<AgentTurn> = local
                        .iter()
                        .filter(|t| t.role == "system")
                        .cloned()
                        .collect();
                    let authored = local.iter().filter(|t| t.role != "system").count();
                    let echoed =
                        local
                            .iter()
                            .rev()
                            .find(|t| t.role != "system")
                            .is_none_or(|tail| {
                                session
                                    .turns
                                    .iter()
                                    .any(|s| s.role == tail.role && s.text == tail.text)
                            });
                    if !echoed {
                        let mut merged = local.clone();
                        for turn in &session.turns {
                            if turn.role == "user" {
                                continue;
                            }
                            if !merged
                                .iter()
                                .any(|t| t.role == turn.role && t.text == turn.text)
                            {
                                merged.push(turn.clone());
                            }
                        }
                        if merged.len() != local.len() {
                            self.agents.local_turns.insert(id, merged);
                        }
                    } else if session.turns.len() >= authored {
                        if systems.is_empty() {
                            self.agents.local_turns.remove(&id);
                        } else {
                            let mut merged = session.turns.clone();
                            for turn in systems {
                                if !merged
                                    .iter()
                                    .any(|t| t.role == "system" && t.text == turn.text)
                                {
                                    merged.push(turn);
                                }
                            }
                            self.agents.local_turns.insert(id, merged);
                        }
                    }
                }
                self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
                let previous = self
                    .agents
                    .sessions
                    .get(&id)
                    .map(|s| s.bundle.images.len())
                    .unwrap_or(0);
                let arrived = session.bundle.images.len() > previous;
                self.agents.sessions.insert(id, session);
                if arrived {
                    let count = self.agent_images(id).len();
                    self.agents.cover_focus.insert(id, count.saturating_sub(1));
                }
                self.sync_agent_text_output(id);
                ctx.request_repaint();
            }
        }
        self.agents.sources.retain(&live_dirs);
        self.pump_agent_text_outputs();
    }
    fn agent_context_for(
        &self,
        session: &str,
        provider: &str,
        _scope: AgentContextScope,
    ) -> AgentContext {
        let tab = self.tab();
        AgentContext {
            app: "slate",
            session: session.to_string(),
            provider: provider.to_string(),
            workbook: tab.path.clone(),
            format_version: tab.doc.format_version,
            scope: "wired".into(),
            selection: vec![],
            viewport: None,
            board_summary: String::new(),
            generated_at: atlas_ai::context::now_secs(),
        }
    }
    pub(super) fn consume_agent_context(&mut self, portal: NodeId) {
        // A generator or text block rereads its wires on every run; they stay rewirable.
        if slate_doc::agent_inputs::is_flow_node(&self.doc().scene, portal) {
            return;
        }
        let ids: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|n| {
                let NodeKind::Connector(c) = &n.kind else {
                    return None;
                };
                let binding = c.binding.as_ref()?;
                if binding.consumed {
                    return None;
                }
                // TWIN: slate_doc::agent_inputs::WireBinding::target 2026-09-25
                let target = if binding.input_b { &c.b } else { &c.a };
                slate_doc::agent_inputs::endpoint_node(target)
                    .filter(|id| *id == portal)
                    .map(|_| n.id)
            })
            .collect();
        if ids.is_empty() {
            return;
        }
        self.patch_nodes(&ids, |n| {
            if let NodeKind::Connector(c) = &mut n.kind {
                if let Some(binding) = &mut c.binding {
                    binding.consumed = true;
                }
            }
        });
    }
}
