//! Fit agent cards to the measured transcript.

use super::*;

impl SlateApp {
    pub(crate) fn stop_pruned_agent_runs(&self, ids: &[NodeId]) {
        for id in ids {
            if let Some((session, _)) = self.agent_session_for(*id) {
                if let Some(link) = self.agents.codex.get(&session) {
                    link.stop();
                }
            }
        }
    }
    pub(super) fn agent_draft_width(&self, id: NodeId) -> f32 {
        self.agents
            .title_widths
            .get(&id)
            .copied()
            .unwrap_or(slate_doc::agent_chat::MIN_CARD_WIDTH)
    }
    /// Content measurements are cached on change; geometry uses the existing
    /// coalescing journal path, so typing/streaming cannot bypass undo.
    pub(crate) fn fit_agent_cards(&mut self, ctx: &egui::Context) {
        use std::hash::{Hash, Hasher};
        self.settle_stream_open();
        if self.tab().read_only || self.board_drag.is_some() {
            return;
        }
        // Respect Undo until new typing or provider output actually changes the content.
        if self.tab().journal.can_redo()
            && self
                .agents
                .fit_revision
                .is_none_or(|r| r.1 == self.agents.output_epoch && r.2 == self.agents.prompt_epoch)
        {
            return;
        }
        let revision = (
            self.scene_gen,
            self.agents.output_epoch,
            self.agents.prompt_epoch,
            self.tab().id,
            self.agents.project_picker,
            self.agents.chat_picker.as_ref().map(|p| p.portal),
            self.agents.pending_chat_pick,
            self.agents.measure_epoch,
        );
        if self.agents.fit_revision == Some(revision) {
            return;
        }
        let nodes: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !n.locked)
            .filter(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| {
                    !a.provider.is_empty() && a.view == atlas_ai::agent::PortalView::Chat
                })
            })
            .cloned()
            .collect();
        let mut updates = Vec::new();
        let mut measured = 0;
        let mut more = false;
        for node in nodes {
            if self.agents.project_picker == Some(node.id)
                || self
                    .agents
                    .chat_picker
                    .as_ref()
                    .is_some_and(|p| p.portal == node.id)
                || self.agents.pending_chat_pick == Some(node.id)
            {
                continue;
            }
            let NodeKind::Portal(p) = &node.kind else {
                continue;
            };
            let a = p.agent.as_ref().unwrap();
            let turns = self.visible_agent_turns(node.id);
            let prompt = self
                .agents
                .prompts
                .get(&node.id)
                .cloned()
                .unwrap_or_default();
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                self.tab().id,
                &p.title,
                &a.model,
                a.chat.train,
                a.chat.draft,
                a.chat.start,
                a.chat.end,
                &a.chat.bundled,
                &a.chat.bundle_layers,
                a.chat.window_height,
                format!("{:?}", a.chat.detail),
                &prompt,
            )
                .hash(&mut hash);
            (
                a.chat.collapsed,
                a.chat.partial,
                self.agents.stream_open.contains(&node.id),
                a.chat.size.map(|s| [s[0].to_bits(), s[1].to_bits()]),
                self.agent_has_child(node.id),
            )
                .hash(&mut hash);
            (self.agents.key_entry == Some(node.id)).hash(&mut hash);
            self.agents
                .content_heights
                .get(&node.id)
                .map(|(w, h, s)| (w.to_bits(), h.to_bits(), *s))
                .hash(&mut hash);
            node.rect.w.to_bits().hash(&mut hash);
            node.rect.h.to_bits().hash(&mut hash);
            for t in &turns {
                t.text.hash(&mut hash);
                t.role.hash(&mut hash);
            }
            let key = hash.finish();
            if self.agents.fit_keys.get(&node.id) == Some(&key) {
                continue;
            }
            if measured >= 8 {
                more = true;
                break;
            }
            measured += 1;
            self.agents.fit_keys.insert(node.id, key);
            let after = self.fitted_agent_card(ctx, &node);
            if (after.rect.w - node.rect.w).abs() > 0.5
                || (after.rect.h - node.rect.h).abs() > 0.5
                || after.kind != node.kind
            {
                updates.push(after);
            }
        }
        for after in self.settle_agent_projection(updates) {
            if self.fold_into_agent_step(&after) {
                continue;
            }
            self.patch_nodes(&[after.id], |n| {
                *n = after.clone();
            });
        }
        self.agents.fit_revision = if more {
            None
        } else {
            Some((
                self.scene_gen,
                self.agents.output_epoch,
                self.agents.prompt_epoch,
                self.tab().id,
                self.agents.project_picker,
                self.agents.chat_picker.as_ref().map(|p| p.portal),
                self.agents.pending_chat_pick,
                self.agents.measure_epoch,
            ))
        };
        if more {
            ctx.request_repaint();
        }
    }
    /// What `paint_agent_bound` stacks in a card's transcript. A painted height
    /// only describes the card while this is unchanged.
    pub(super) fn transcript_signature(&self, id: NodeId, turns: &[AgentTurn]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for t in turns {
            t.text.hash(&mut hash);
            t.role.hash(&mut hash);
        }
        self.agents
            .awaiting
            .get(&id)
            .map(std::mem::discriminant)
            .hash(&mut hash);
        self.agents
            .session(id)
            .is_some_and(|s| s.approval.is_some())
            .hash(&mut hash);
        self.agent_has_child(id).hash(&mut hash);
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .map(|a| std::mem::discriminant(&a.chat.detail))
            .hash(&mut hash);
        hash.finish()
    }
    /// The transcript height paint laid out for this card at `width`, while it
    /// still shows the same content.
    fn measured_transcript(&self, id: NodeId, width: f32, turns: &[AgentTurn]) -> Option<f32> {
        let (w, h, signature) = *self.agents.content_heights.get(&id)?;
        ((w - width).abs() <= 0.5 && signature == self.transcript_signature(id, turns)).then_some(h)
    }
    /// `node` with the rect its content, presentation and authored size give it.
    pub(super) fn fitted_agent_card(&mut self, ctx: &egui::Context, node: &Node) -> Node {
        use slate_doc::agent_chat::{CARD_WIDTH, DRAFT_HEIGHT, MIN_CARD_WIDTH};
        let NodeKind::Portal(p) = &node.kind else {
            return node.clone();
        };
        let Some(a) = p.agent.as_ref() else {
            return node.clone();
        };
        let turns = self.visible_agent_turns(node.id);
        let prompt = self
            .agents
            .prompts
            .get(&node.id)
            .cloned()
            .unwrap_or_default();
        {
            let measure = |text: String, width: f32, size: f32| {
                ctx.fonts(|f| {
                    f.layout(
                        text,
                        FontId::proportional(size),
                        Color32::WHITE,
                        width.max(1.0),
                    )
                    .size()
                })
            };
            let title = if a.provider == "codex"
                || a.provider == "cursor"
                || a.provider.starts_with("ollama")
            {
                format!(
                    "{} · {} ▾",
                    p.title,
                    atlas_ai::agent::model_label(
                        a.model
                            .as_deref()
                            .or_else(|| a.provider.strip_prefix("ollama/"))
                            .unwrap_or(model_fallback(&a.provider))
                    )
                )
            } else {
                p.title.clone()
            };
            let title_w = (measure(title, f32::INFINITY, 13.0).x + 88.0).max(MIN_CARD_WIDTH);
            self.agents.title_widths.insert(node.id, title_w);
            let mut after = node.clone();
            if a.chat.train && !a.chat.bundled.is_empty() {
                let count = a.chat.bundled.len() + 1;
                after.rect.w = (count.min(6) as f32 * 76.0 + 24.0).max(title_w);
                after.rect.h = 36.0 + count.div_ceil(6) as f32 * 88.0;
            } else if a.chat.draft {
                after.rect.w = title_w
                    .max(measure(prompt.clone(), f32::INFINITY, 14.0).x + 24.0)
                    .min(CARD_WIDTH.max(title_w));
                let prompt_h =
                    composer_text_height(ctx, &prompt, composer_wrap(after.rect.w), 14.0);
                after.rect.h = hugging_composer_card(prompt_h);
            } else if a.chat.collapsed || a.chat.partial || a.chat.size.is_some() {
                // Collapse owns the height; a partial open is twice that capsule;
                // a person's size owns the rest.
                let partial = matches!(
                    card_fold(&a.chat, self.agents.stream_open.contains(&node.id)),
                    train_ux::CardFold::Partial
                );
                after.rect.w = a.chat.size.map_or(node.rect.w, |s| s[0]);
                after.rect.h = if !a.chat.collapsed && !a.chat.partial {
                    a.chat.size.map(|s| s[1]).unwrap_or(node.rect.h)
                } else {
                    let text = turns
                        .iter()
                        .map(|t| t.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n");
                    let mut h = collapsed_card_height(ctx, text, after.rect.w);
                    if partial {
                        h = train_ux::stream_card_height(h);
                    }
                    if !self.agent_has_child(node.id) {
                        h += composer_text_height(ctx, &prompt, composer_wrap(after.rect.w), 14.0)
                            + COMPOSER_BOTTOM;
                    }
                    h
                };
            } else if a.chat.train {
                let text = turns
                    .iter()
                    .map(|t| t.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                after.rect.w = title_w
                    .max(measure(text.clone(), f32::INFINITY, 13.0).x + 24.0)
                    .min(CARD_WIDTH.max(title_w));
                let tail = !self.agent_has_child(node.id);
                // Sent Summary cards use the summary painter; `paint_agent_bound`
                // draws every other train card, composer only on the tail.
                after.rect.h = if tail || a.chat.detail != slate_doc::agent_chat::Detail::Summary {
                    let transcript_h = self
                        .measured_transcript(node.id, after.rect.w, &turns)
                        .unwrap_or_else(|| {
                            let pairs = a.chat.detail == slate_doc::agent_chat::Detail::Pair;
                            let mut h = 0.0;
                            for (i, t) in turns.iter().enumerate() {
                                if pairs
                                    && i > 0
                                    && turns[i - 1].role == "user"
                                    && t.role == "assistant"
                                {
                                    h += 22.0;
                                }
                                let ratio = if t.role == "user" { 0.78 } else { 0.94 };
                                h += measure(
                                    t.text.clone(),
                                    (after.rect.w - 24.0) * ratio - 20.0,
                                    14.0,
                                )
                                .y + 32.0;
                            }
                            if self.agents.awaiting.contains_key(&node.id) {
                                h += 30.0;
                            }
                            h
                        });
                    if tail {
                        let prompt_h =
                            composer_text_height(ctx, &prompt, composer_wrap(after.rect.w), 14.0);
                        conversation_card_height(transcript_h, prompt_h)
                    } else {
                        (COMPOSER_TOP + transcript_h + COMPOSER_BOTTOM).max(DRAFT_HEIGHT)
                    }
                } else {
                    let text_h = measure(text, after.rect.w - 24.0, 13.0).y;
                    (COMPOSER_TOP + text_h + CARD_TEXT_PAD).max(DRAFT_HEIGHT)
                };
            } else {
                after.rect.w = node.rect.w.max(title_w);
                let cap = a
                    .chat
                    .window_height
                    .unwrap_or(node.rect.h.max(112.0) as u32)
                    .max(112);
                let text_h: f32 = turns
                    .iter()
                    .map(|t| {
                        measure(
                            t.text.clone(),
                            (after.rect.w - 24.0) * if t.role == "user" { 0.78 } else { 0.94 }
                                - 20.0,
                            14.0,
                        )
                        .y + 32.0
                    })
                    .sum();
                let prompt_h =
                    composer_text_height(ctx, &prompt, composer_wrap(after.rect.w), 14.0);
                after.rect.h = conversation_card_height(text_h, prompt_h)
                    .min(cap as f32)
                    .max(112.0);
                if let NodeKind::Portal(p) = &mut after.kind {
                    p.agent.as_mut().unwrap().chat.window_height = Some(cap);
                }
            }
            if self.agents.key_entry == Some(node.id)
                && !a.chat.collapsed
                && a.chat.size.is_none()
                && (a.chat.draft
                    || !a.chat.train
                    || a.chat.detail == slate_doc::agent_chat::Detail::Full
                    || self.agent_linear_terminal(node.id))
            {
                after.rect.h += 72.0;
            }
            after
        }
    }
}
