//! Context pocket, retract, and fork.

use super::*;

impl SlateApp {
    pub(crate) fn begin_context_retract(&mut self, portals: &[NodeId]) {
        self.retract_context(portals, &HashSet::new());
    }
    /// Context shrinks back into the agent it belongs to. A provenance card is
    /// removed. Context a chat card already sent is pocketed (hidden, wires kept)
    /// into every card that used it. `leaving` cards are deleted by the same action.
    pub(crate) fn retract_context(&mut self, ids: &[NodeId], leaving: &HashSet<NodeId>) {
        let mut drop = Vec::new();
        let mut rest = Vec::new();
        for id in ids {
            let Some(node) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let Some((wire, anchor)) = self.machine_context_link(*id) else {
                rest.push(*id);
                continue;
            };
            let now = Instant::now();
            if matches!(node.kind, NodeKind::Frame(_)) {
                let members = self.doc().scene.members_of(*id);
                let ghosts: Vec<_> = members
                    .iter()
                    .filter_map(|m| self.doc().scene.node(*m))
                    .filter(|m| !matches!(m.kind, NodeKind::Connector(_)))
                    .map(|m| (m.clone(), anchor, now))
                    .collect();
                self.agents.retract_ghosts.push((node, anchor, now));
                self.agents.retract_ghosts.extend(ghosts);
                drop.extend(members);
            } else {
                self.agents.retract_ghosts.push((node, anchor, now));
            }
            drop.push(wire);
            drop.push(*id);
            self.agents.spawned.retain(|_, n| n != id);
        }
        for id in self.pocket_context(&rest, leaving) {
            if self
                .doc()
                .scene
                .node(id)
                .is_some_and(|n| matches!(n.kind, NodeKind::Frame(_)))
            {
                drop.extend(self.doc().scene.members_of(id));
            }
            drop.push(id);
            self.agents.spawned.retain(|_, n| *n != id);
        }
        drop.sort();
        drop.dedup();
        if !drop.is_empty() {
            self.delete_board_nodes_now(&drop);
        }
    }
    /// Cards other than `leaving` that already sent `id` as context.
    fn context_consumers(&self, id: NodeId, leaving: &HashSet<NodeId>) -> Vec<NodeId> {
        let mut cards: Vec<_> = slate_doc::agent_inputs::consumed_by(&self.doc().scene, id)
            .into_iter()
            .map(|(_, card)| card)
            .filter(|card| !leaving.contains(card))
            .collect();
        cards.sort();
        cards.dedup();
        cards
    }
    /// Hide each used context in one journal step, shrinking a ghost into every
    /// visible consuming card's input. Returns the ids no card had used.
    fn pocket_context(&mut self, ids: &[NodeId], leaving: &HashSet<NodeId>) -> Vec<NodeId> {
        use slate_doc::scene::Side;
        let mut unused = Vec::new();
        let mut commands = Vec::new();
        let now = Instant::now();
        for id in ids {
            let cards = self.context_consumers(*id, leaving);
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            if cards.is_empty() {
                unused.push(*id);
                continue;
            }
            if before.hidden {
                continue;
            }
            for card in cards {
                if let Some(host) = self.doc().scene.node(card).filter(|n| !n.hidden) {
                    let anchor = slate_doc::connector_anchor_on(host, Side::Left, 0.5);
                    self.agents.retract_ghosts.push((
                        before.clone(),
                        Pos2::new(anchor[0], anchor[1]),
                        now,
                    ));
                }
            }
            let mut after = before.clone();
            after.hidden = true;
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        if !commands.is_empty() && self.commit_scene(commands) {
            for id in ids {
                self.board_sel.remove(id);
            }
            for open in self.agents.pocket_open.values_mut() {
                open.retain(|id| !ids.contains(id));
            }
        }
        unused
    }
    /// Hidden context, or context a handle click brought out of this card.
    pub(crate) fn agent_has_pocket(&self, card: NodeId) -> bool {
        if self
            .agents
            .pocket_open
            .get(&card)
            .is_some_and(|open| !open.is_empty())
        {
            return true;
        }
        let revision = (self.scene_gen, self.doc().scene.scene_gen());
        let mut cache = self.agents.pocket_cache.borrow_mut();
        if cache.0 != revision {
            *cache = (
                revision,
                slate_doc::agent_inputs::pocket_holders(&self.doc().scene)
                    .into_iter()
                    .collect(),
            );
        }
        cache.1.contains(&card)
    }
    /// The human context handle: bring the card's pocketed context out beside
    /// it, or pocket what that click brought out. One journal step each way.
    pub(crate) fn agent_toggle_pocket(&mut self, detail: Option<&str>) -> bool {
        let card = detail
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(NodeId)
            .or_else(|| self.selected_agent_portal())
            .filter(|id| slate_doc::agent_inputs::is_chat_card(&self.doc().scene, *id));
        let Some(card) = card else {
            return false;
        };
        let Some(host) = self.doc().scene.node(card).map(|n| n.rect) else {
            return false;
        };
        let hidden = slate_doc::agent_inputs::pocketed(&self.doc().scene, card);
        if hidden.is_empty() {
            let open: Vec<_> = self
                .agents
                .pocket_open
                .remove(&card)
                .unwrap_or_default()
                .into_iter()
                .filter(|id| {
                    self.doc().scene.node(*id).is_some_and(|n| !n.hidden)
                        && self.context_consumers(*id, &HashSet::new()).contains(&card)
                })
                .collect();
            if open.is_empty() {
                self.toast("Nothing is pocketed in this card.");
                return false;
            }
            self.pocket_context(&open, &HashSet::new());
            return true;
        }
        let mut y = host.y;
        let mut commands = Vec::new();
        for id in &hidden {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let placed = self.clear_spawn_rect(slate_doc::WorldRect::new(
                host.x - before.rect.w - 72.0,
                y,
                before.rect.w,
                before.rect.h,
            ));
            y = placed.y + placed.h + 48.0;
            let mut after = before.clone();
            after.hidden = false;
            after.rect.x = placed.x;
            after.rect.y = placed.y;
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        if !self.commit_scene(commands) {
            return false;
        }
        self.agents.pocket_open.insert(card, hidden);
        true
    }
    pub(crate) fn paint_context_retract(
        &mut self,
        ui: &egui::Ui,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        // 75% faster than the previous 0.42s suck.
        const SECS: f32 = 0.24;
        if self.agents.retract_ghosts.is_empty() {
            return;
        }
        self.agents
            .retract_ghosts
            .retain(|(_, _, at)| at.elapsed().as_secs_f32() < SECS);
        let ghosts = self.agents.retract_ghosts.clone();
        for (node, anchor, at) in ghosts {
            let t = (at.elapsed().as_secs_f32() / SECS).clamp(0.0, 1.0);
            let ease = t * t;
            let mut ghost = node;
            let cx = ghost.rect.x + ghost.rect.w * 0.5;
            let cy = ghost.rect.y + ghost.rect.h * 0.5;
            let scale = (1.0 - ease * 0.92).max(0.04);
            let nx = cx + (anchor.x - cx) * ease;
            let ny = cy + (anchor.y - cy) * ease;
            ghost.rect.w *= scale;
            ghost.rect.h *= scale;
            ghost.rect.x = nx - ghost.rect.w * 0.5;
            ghost.rect.y = ny - ghost.rect.h * 0.5;
            ghost.opacity *= 1.0 - ease;
            ghost.hidden = false;
            self.paint_board_node(ui, painter, xf, &ghost, false);
        }
        if !self.agents.retract_ghosts.is_empty() {
            ui.ctx().request_repaint();
        }
    }
    pub(super) fn agent_has_child(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .nodes
            .iter()
            .any(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
    }
    pub(super) fn agent_linear_terminal(&self, id: NodeId) -> bool {
        self.agent_linear(id)
            && !self
                .doc()
                .scene
                .nodes
                .iter()
                .any(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
    }
    pub(super) fn agent_linear(&self, id: NodeId) -> bool {
        self.doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.chat.linear || atlas_ai::runtime::linear_provider(&a.provider))
    }
    pub(crate) fn agent_fork_selected(&mut self, detail: Option<&str>) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        if self.agent_linear(id) {
            self.toast("Coding agents use one conversation stream.");
            return true;
        }
        if self.agent_is_running(id) {
            self.toast("Wait for a completed checkpoint before forking.");
            return true;
        }
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(binding) = slate_doc::agent_chat::agent(&original).cloned() else {
            return false;
        };
        if binding.chat.draft || (binding.chat.train && !binding.chat.bundled.is_empty()) {
            return false;
        }
        let turns = self.agent_all_turns(id);
        let end = binding.chat.end.unwrap_or(turns.len());
        if atlas_ai::agent::checkpoint(&turns, Some(end)).is_err() {
            self.toast("Wait for this history to load.");
            return true;
        }
        let position = detail
            .and_then(|s| serde_json::from_str::<[f32; 2]>(s).ok())
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .unwrap_or_else(|| self.next_fork_origin(id));
        let mut next = self.doc_mut().scene.build_duplicate(&original, 0.0, 0.0);
        next.rect = slate_doc::WorldRect::new(
            position[0],
            position[1],
            self.agent_draft_width(id),
            slate_doc::agent_chat::DRAFT_HEIGHT,
        );
        if let NodeKind::Portal(p) = &mut next.kind {
            if let Some(a) = &mut p.agent {
                a.chat = slate_doc::agent_chat::ChatView {
                    linear: false,
                    train: true,
                    parent: Some(id),
                    start: end,
                    end: Some(end),
                    detail: binding.chat.detail,
                    custom_fill: binding.chat.custom_fill,
                    stroke: binding.chat.stroke,
                    draft: true,
                    ..Default::default()
                };
            }
        }
        let next_id = next.id;
        if self.add_nodes(vec![next]).is_empty() {
            return false;
        }
        if let Some(turns) = self.agents.local_turns.get(&id).cloned() {
            self.agents.local_turns.insert(next_id, turns);
        } else if let Some(session) = self.agents.sessions.get(&id).cloned() {
            self.agents.sessions.insert(next_id, session);
        }
        self.board_sel.clear();
        self.board_sel.insert(next_id);
        self.agent_focus(next_id);
        self.agents.composer_focus = Some(next_id);
        true
    }
    fn next_fork_origin(&self, id: NodeId) -> [f32; 2] {
        let Some(node) = self.doc().scene.node(id) else {
            return [0.0, 0.0];
        };
        let below = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
            .count() as f32;
        [
            node.rect.x + node.rect.w + slate_doc::agent_chat::CARD_GAP,
            node.rect.y
                + below * (slate_doc::agent_chat::DRAFT_HEIGHT + slate_doc::agent_chat::LANE_GAP),
        ]
    }
}
