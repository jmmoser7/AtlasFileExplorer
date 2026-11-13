//! Agent pump and session pump.

use super::*;
mod context;
pub(super) mod image_import;
#[cfg(test)]
mod text_answer_tests;
pub(super) use context::ContextPublications;

impl SlateApp {
    /// Provider cancellation may acknowledge late. Only the current generator
    /// request can update its displayed source or active model memory.
    pub(super) fn generator_session_is_current(&self, id: NodeId, session: &AgentSession) -> bool {
        if self
            .doc()
            .scene
            .node(id)
            .is_some_and(slate_doc::agent_chat::is_chat_host)
        {
            return true;
        }
        self.agents.cancelled_generations.get(&id) != Some(&session.request)
            && (session.request.is_empty()
                || !self.agents.cancelled_generations.contains_key(&id)
                || self.agents.requests.contains_key(&id))
            && (session.request.is_empty()
                || self
                    .agents
                    .requests
                    .get(&id)
                    .is_none_or(|request| request == &session.request))
    }
    /// Only completed, current, uncancelled requests can author a passage link.
    pub(crate) fn finish_text_answer_requests(&mut self) {
        let doc = self.tab().id;
        let pending: Vec<_> = self
            .agents
            .flow
            .text_answer_requests
            .iter()
            .filter(|((workbook, _, _), _)| *workbook == doc)
            .map(|(key, snapshot)| (key.clone(), snapshot.clone()))
            .collect();
        for (key, snapshot) in pending {
            let (_, id, ref request) = key;
            let queued = self
                .agents
                .comfy_queue
                .get(&id)
                .is_some_and(|queue| queue.iter().any(|q| q.request.id == *request));
            let current = self.agents.requests.get(&id) == Some(request);
            let cancelled = self.agents.cancelled_generations.get(&id) == Some(request);
            let unchanged = self.text_answer_source(id).is_some_and(|(text, revision)| {
                text == snapshot.source && revision == snapshot.source_revision
            });
            if cancelled || (!unchanged && !queued) || (!queued && !current) {
                self.agents.flow.text_answer_requests.remove(&key);
                continue;
            }
            let Some(session) = self.agents.sessions.get(&id).cloned() else {
                continue;
            };
            if !current || session.request != *request {
                continue;
            }
            if matches!(session.status, atlas_agent::AgentStatus::Thinking) {
                continue;
            }
            self.agents.flow.text_answer_requests.remove(&key);
            if !matches!(session.status, atlas_agent::AgentStatus::Idle)
                || self
                    .agent_session_for(id)
                    .is_none_or(|(name, _)| name != snapshot.session)
            {
                continue;
            }
            let Some((index, answer)) = session
                .turns
                .iter()
                .enumerate()
                .rev()
                .find(|(_, turn)| turn.role == "assistant" && !turn.text.trim().is_empty())
            else {
                continue;
            };
            let selected_review = if snapshot.review {
                self.record_text_review(
                    id,
                    request,
                    &snapshot,
                    answer.text.clone(),
                    slate_doc::text_answers::TextAnswerRef {
                        session: snapshot.session.clone(),
                        through: index + 1,
                    },
                )
            } else {
                true
            };
            if snapshot.selected && selected_review {
                self.record_text_answer(
                    id,
                    &snapshot.source,
                    snapshot.range.clone(),
                    slate_doc::text_answers::TextAnswerRef {
                        session: snapshot.session,
                        through: index + 1,
                    },
                    snapshot.question,
                    answer.text.clone(),
                );
                if snapshot.review {
                    self.link_text_review(id, request, &snapshot.range);
                }
            }
        }
    }

    pub(crate) fn agent_pump(&mut self, ctx: &egui::Context) {
        self.ai.packs.begin_frame();
        if let Some((node, response)) = self.take_text_answer_navigation(ctx) {
            self.agents.flow.text_answer_navigation = Some((self.tab().id, node, response));
        }
        self.apply_text_answer_navigation();
        self.paint_pack_key_entry(ctx);
        self.sync_agent_doc();
        self.pump_agent_approval_deliveries(ctx);
        let front_span = atlas_core::session_log::span("slate.agents.front");
        if !self.agents.codex.is_empty() {
            let _span = atlas_core::session_log::span("slate.agents.front.links");
            let current = self.tab().id;
            let topology = self.agent_topology();
            let mut existing = HashSet::with_capacity(self.agents.codex.len());
            if self.tabs.iter().any(|tab| tab.id == current) {
                existing.extend(
                    self.agents
                        .codex
                        .keys()
                        .filter(|id| topology.non_train_sessions.contains(*id))
                        .cloned(),
                );
            }
            existing.extend(
                self.tabs
                    .iter()
                    .filter(|tab| tab.id != current)
                    .flat_map(|tab| tab.doc.scene.nodes.iter())
                    .filter_map(slate_doc::agent_chat::agent)
                    .filter(|agent| {
                        !agent.chat.train && self.agents.codex.contains_key(&agent.session)
                    })
                    .map(|agent| agent.session.clone()),
            );
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
        }
        self.ensure_agent_programs();
        self.ensure_agent_recents(ctx);
        self.pump_cursor_ide(ctx);
        self.pump_agent_chats(ctx);
        self.paint_schedule_dialog(ctx);
        self.pump_agent_connection(ctx);
        self.pump_agent_connecting();
        {
            let _span = atlas_core::session_log::span("slate.agents.front.rejoin");
            self.rejoin_agent_chats();
        }
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
        self.pump_known_agent_image_imports(ctx);
        let tail_span = atlas_core::session_log::span("slate.agents.tail");
        self.pump_agent_awaits(ctx, &ws);
        self.pump_grasshopper_authoring(ctx);
        self.paint_grasshopper_confirmation(ctx);
        self.pump_crosstalk(ctx);
        self.pump_pending_chat_sends(ctx);
        self.pump_comfy_queue();
        self.pump_artifact_generations(ctx);
        if self
            .agents
            .comfy_queue
            .values()
            .any(|queue| !queue.is_empty())
        {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
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
        let maintenance_span = atlas_core::session_log::span("slate.agents.sessions.maintain");
        let topology = self.agent_topology();
        let portals = &topology.cards;
        let live = &topology.card_ids;
        // Linked sessions can answer from outside Slate (the sidecar writes
        // session.json), so an open board with agents keeps a slow poll.
        if self.agent_background_pending() {
            ctx.request_repaint_after(Duration::from_millis(250));
        } else if !self.at_home && !portals.is_empty() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }

        self.agents.sessions.retain(|id, _| live.contains(id));
        self.relocate_absorbed_drafts(live);
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
        self.agents
            .image_imports
            .note_candidates(&topology.local_images);
        drop(maintenance_span);

        let batch = self.agent_session_batch(ctx, ws, portals);
        let links_span = atlas_core::session_log::span("slate.agents.sessions.links");
        let mut updated = !batch.changed_dirs.is_empty();
        if updated {
            self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
            for id in &batch.changed_dirs {
                self.agents.sessions.remove(id);
                self.agents.image_cache.borrow_mut().remove(id);
                self.agents.cover_focus.remove(id);
            }
        }
        for (id, agent) in portals {
            let id = *id;
            let Some(agent) = agent else {
                continue;
            };
            let previous = self.agents.bindings.get(&id);
            let rebound = previous.is_some_and(|session| session != &agent.session);
            let changed_dir = batch.changed_dirs.contains(&id);
            let link = batch.links.by_card.get(&id);
            if link.is_some() && previous != Some(&agent.session) {
                self.agents.bindings.insert(id, agent.session.clone());
                self.agents.image_cache.borrow_mut().remove(&id);
                self.agents.sessions.remove(&id);
                self.agents.local_turns.remove(&id);
                self.agents.awaiting.remove(&id);
                self.agents.cover_focus.remove(&id);
            }
            let Some(link) = link else { continue };
            if !rebound
                && !changed_dir
                && !batch.poll_due(id, link)
                && self.agents.focused != Some(id)
                && self.portal_chrome.maximized != Some(id)
                && !self.board_sel.contains(&id)
                && !self.agent_connecting(id)
                && !self.agent_is_running(id)
            {
                continue;
            }
            let dir = &link.dir;
            if !self.agent_has_child(id) {
                if let Some(raw) = self.agents.sources.take_place(dir) {
                    self.place_atlas_request(id, dir, &raw);
                }
                self.agent_output_roots(id, dir);
            }
            let context = if let Some(cards) = batch.publish.get(&id) {
                let mut context =
                    self.agent_context_for(&agent.session, &agent.provider, agent.context);
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
            if let Some(session) = self.agents.sources.poll(dir, context.as_ref()) {
                if !self.generator_session_is_current(id, &session) {
                    continue;
                }
                self.sync_chat_train_provider_channel(id, &session);
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
                updated = true;
            }
        }
        if updated {
            ctx.request_repaint();
        }
        drop(links_span);
        let _aux_span = atlas_core::session_log::span("slate.agents.sessions.aux");
        self.finish_text_answer_requests();
        let mut shown_sources = topology
            .shown_text
            .iter()
            .map(|locator| slate_doc::scene::resolve_source(self.tab().path.as_deref(), locator))
            .filter_map(|path| path.parent().map(std::path::Path::to_path_buf))
            .collect::<Vec<_>>();
        shown_sources.extend(
            self.agents
                .flow
                .viewed_dialogues
                .iter()
                .filter(|((tab, _), _)| *tab == self.tab().id)
                .map(|(_, reference)| {
                    slate_doc::scene::resolve_source(
                        self.tab().path.as_deref(),
                        &reference.source.locator,
                    )
                })
                .filter_map(|path| path.parent().map(std::path::Path::to_path_buf)),
        );
        if let Some((tab, _, reference)) = self
            .agents
            .flow
            .text_answer_navigation
            .as_ref()
            .filter(|(tab, _, _)| *tab == self.tab().id)
        {
            let _ = tab;
            let path = slate_doc::scene::resolve_source(
                self.tab().path.as_deref(),
                &reference.source.locator,
            );
            if let Some(dir) = path.parent() {
                shown_sources.push(dir.to_path_buf());
            }
        }
        let mut auxiliary_dirs = HashSet::new();
        for dir in shown_sources {
            self.agents.sources.poll(&dir, None);
            auxiliary_dirs.insert(dir);
        }
        self.retain_model_task_session_sources(&mut auxiliary_dirs);
        if auxiliary_dirs.is_subset(&batch.links.dirs) {
            self.agents.sources.retain(&batch.links.dirs);
        } else {
            let mut live_dirs = batch.links.dirs.clone();
            live_dirs.extend(auxiliary_dirs);
            self.agents.sources.retain(&live_dirs);
        }
        self.pump_agent_text_outputs();
    }
}
