//! Train projection, rechunk, and wire retarget.

use super::*;

impl SlateApp {
    /// Materialize references to a linked transcript; never copy text into .slate.
    pub(crate) fn agent_show_train(&mut self) -> bool {
        self.agent_project_cards(1)
    }
    pub(crate) fn agent_show_pairs(&mut self) -> bool {
        self.agent_project_cards(2)
    }
    /// `stride` 1 is one turn per card. `stride` 2 keeps a user line with its
    /// reply. Every linear path of the conversation is re-chunked from its
    /// first turn to its last, so a switch mid-conversation reconfigures all
    /// cards and branches, and one Undo returns the previous presentation.
    fn agent_project_cards(&mut self, stride: usize) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let ids = slate_doc::agent_chat::conversation(&self.doc().scene, id);
        if ids.iter().any(|id| self.agent_is_running(*id)) {
            self.toast("Wait for the response before changing its presentation.");
            return true;
        }
        let detail = if stride > 1 {
            slate_doc::agent_chat::Detail::Pair
        } else {
            slate_doc::agent_chat::Detail::Summary
        };
        let mut commands = Vec::new();
        let mut add_index = self.doc().scene.nodes.len();
        let mut moved = HashMap::new();
        let mut anchor = None;
        for path in slate_doc::agent_chat::segments(&self.doc().scene, id) {
            let Some((cards, draft)) = self.agent_path_split(&path) else {
                continue;
            };
            anchor = anchor.or(cards.last().copied());
            if !cards.is_empty() {
                self.agent_rechunk_path(&cards, stride, &mut add_index, &mut commands, &mut moved);
            }
            if let Some(draft) = draft {
                self.restyle_agent_draft(draft, true, detail, &mut commands);
            }
        }
        let Some(anchor) = anchor else {
            return true;
        };
        self.retarget_agent_wires(&moved, &mut commands);
        let mut removed: Vec<_> = moved
            .keys()
            .filter_map(|id| {
                let index = self.doc().scene.index_of(*id)?;
                Some((index, self.doc().scene.node(*id)?.clone()))
            })
            .collect();
        removed.sort_by_key(|(index, _)| std::cmp::Reverse(*index));
        commands.extend(
            removed
                .into_iter()
                .map(|(index, node)| slate_doc::SceneCmd::Remove { index, node }),
        );
        self.arrange_agent_projection(anchor, &mut commands);
        self.follow_crosswires(&mut commands);
        if self.commit_scene(commands) {
            let gone: Vec<_> = moved.keys().copied().collect();
            self.forget_deleted_agent_cards(&gone);
            let selected = moved.get(&id).copied().unwrap_or(id);
            self.board_sel.clear();
            self.board_sel.insert(selected);
            self.agents.projection_settle =
                Some((anchor, self.tab().journal.undo_depth(), self.scene_gen));
        }
        true
    }
    /// The chat cards of one linear path and the unsent draft trailing them.
    /// Anything else that is not a card may only trail. A path holding only
    /// a draft is a fork's draft.
    pub(super) fn agent_path_split(
        &self,
        path: &[NodeId],
    ) -> Option<(Vec<NodeId>, Option<NodeId>)> {
        let scene = &self.doc().scene;
        let draft = |id: &NodeId| {
            scene
                .node(*id)
                .filter(|n| matches!(n.kind, NodeKind::Portal(_)))
                .and_then(slate_doc::agent_chat::agent)
                .map(|a| a.chat.draft)
        };
        let n = path
            .iter()
            .take_while(|id| draft(id) == Some(false))
            .count();
        let rest = &path[n..];
        if rest.iter().any(|id| draft(id) == Some(false)) {
            return None;
        }
        let trailing = rest.iter().copied().find(|id| draft(id) == Some(true));
        (n > 0 || trailing.is_some()).then(|| (path[..n].to_vec(), trailing))
    }
    /// An unsent draft takes the new presentation's form where it hangs; the
    /// patch also lets the relayout place it in line.
    pub(super) fn restyle_agent_draft(
        &self,
        id: NodeId,
        train: bool,
        detail: slate_doc::agent_chat::Detail,
        commands: &mut Vec<slate_doc::SceneCmd>,
    ) {
        let Some(before) = self.doc().scene.node(id).cloned() else {
            return;
        };
        let mut after = before.clone();
        after.hidden = false;
        if let Some(a) = slate_doc::agent_chat::agent_mut(&mut after) {
            a.chat.train = train;
            a.chat.detail = detail;
        }
        commands.push(slate_doc::SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        });
    }
    /// A single chat window's composer takes the unsent text of the draft
    /// card it absorbed, and the caret with it.
    pub(super) fn absorb_agent_drafts(&mut self, absorbed: &[(NodeId, NodeId)]) {
        for &(draft, host) in absorbed {
            match self.agents.prompts.remove(&draft) {
                Some(text) => self.agents.prompts.insert(host, text),
                None => self.agents.prompts.remove(&host),
            };
            if self.agents.composer_editing == Some(draft) {
                self.agents.composer_editing = Some(host);
            }
            if self.agents.composer_focus == Some(draft) {
                self.agents.composer_focus = Some(host);
            }
            self.agents.composer_rects.remove(&draft);
            self.agents.absorbed_drafts.insert(draft, host);
        }
        self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
    }
    /// The unsent text of a draft a switch absorbed follows whichever of the
    /// draft card and its window is on the board, through Undo and Redo.
    pub(super) fn relocate_absorbed_drafts(&mut self, live: &HashSet<NodeId>) {
        if self.agents.absorbed_drafts.is_empty() {
            return;
        }
        let scene = &self.doc().scene;
        let unsent = |id: &NodeId| {
            scene
                .node(*id)
                .and_then(slate_doc::agent_chat::agent)
                .is_some_and(|a| a.chat.draft)
        };
        let moves: Vec<(NodeId, NodeId)> = self
            .agents
            .absorbed_drafts
            .iter()
            .filter(|(draft, host)| live.contains(host) && (unsent(draft) || !live.contains(draft)))
            .map(|(&draft, &host)| {
                if live.contains(&draft) {
                    (host, draft)
                } else {
                    (draft, host)
                }
            })
            .collect();
        self.agents
            .absorbed_drafts
            .retain(|draft, _| moves.iter().any(|(a, b)| a == draft || b == draft));
        for (from, to) in moves {
            if let Some(text) = self.agents.prompts.remove(&from) {
                self.agents.prompts.insert(to, text);
                self.agents.prompt_epoch = self.agents.prompt_epoch.wrapping_add(1);
            }
        }
    }
    /// Re-chunks one linear path's turns, `stride` 1 per turn or a user line
    /// with its reply. A card whose range ends where a chunk ends shows that
    /// chunk; a chunk ending inside a card gets a new view of that card's
    /// transcript. Cards that show nothing leave, recorded in `moved` with
    /// the card that now shows their first turn.
    fn agent_rechunk_path(
        &mut self,
        path: &[NodeId],
        stride: usize,
        add_index: &mut usize,
        commands: &mut Vec<slate_doc::SceneCmd>,
        moved: &mut HashMap<NodeId, NodeId>,
    ) {
        use slate_doc::agent_chat::{agent, agent_mut, Detail};
        let members: Vec<Node> = path
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        let turns: Vec<_> = path.iter().map(|id| self.agent_all_turns(*id)).collect();
        let ranges: Vec<(usize, usize)> = members
            .iter()
            .zip(&turns)
            .map(|(m, turns)| {
                let chat = &agent(m).unwrap().chat;
                (
                    chat.start,
                    chat.end.unwrap_or(turns.len()).max(chat.start + 1),
                )
            })
            .collect();
        let last = members.len() - 1;
        let owner = |i: usize| {
            ranges
                .iter()
                .position(|(s, e)| *s <= i && i < *e)
                .unwrap_or(last)
        };
        let (start, end) = (ranges[0].0, ranges[last].1);
        let mut cuts = vec![start];
        for i in start + 1..end {
            let cut = stride == 1
                || match turns[owner(i)].get(i) {
                    Some(turn) => turn.role == "user",
                    None => (i - start) % 2 == 0,
                };
            if cut {
                cuts.push(i);
            }
        }
        cuts.push(end);
        let mut parent = agent(&members[0]).unwrap().chat.parent;
        let mut hosts = Vec::new();
        let mut kept = HashSet::new();
        for chunk in cuts.windows(2) {
            let (a, b) = (chunk[0], chunk[1]);
            let k = owner(b - 1);
            let reuse = ranges[k].1 == b && kept.insert(k);
            let mut node = if reuse {
                members[k].clone()
            } else {
                self.doc_mut().scene.build_duplicate(&members[k], 0.0, 0.0)
            };
            node.hidden = false;
            node.rect.w = slate_doc::agent_chat::CARD_WIDTH;
            node.rect.h = if stride > 1 {
                slate_doc::agent_chat::PAIR_HEIGHT
            } else {
                slate_doc::agent_chat::CARD_HEIGHT
            };
            if let Some(binding) = agent_mut(&mut node) {
                let chat = &mut binding.chat;
                chat.train = true;
                chat.bundled.clear();
                chat.bundle_layers.clear();
                chat.detail = if stride > 1 {
                    Detail::Pair
                } else {
                    Detail::Summary
                };
                chat.parent = parent;
                chat.start = a;
                if !reuse {
                    chat.end = Some(b);
                }
            }
            parent = Some(node.id);
            hosts.push((a, node.id));
            if reuse {
                commands.push(slate_doc::SceneCmd::Patch {
                    before: Box::new(members[k].clone()),
                    after: Box::new(node),
                });
            } else {
                commands.push(slate_doc::SceneCmd::Add {
                    index: *add_index,
                    node,
                });
                *add_index += 1;
            }
        }
        for (k, member) in members.iter().enumerate() {
            if kept.contains(&k) {
                continue;
            }
            let host = hosts
                .iter()
                .rev()
                .find(|(a, _)| *a <= ranges[k].0)
                .unwrap_or(&hosts[0])
                .1;
            moved.insert(member.id, host);
        }
    }
    /// Views of `original`'s turns from its start to `end`, `stride` turns
    /// each, chained by parent. The original keeps the last range and its end.
    pub(super) fn agent_projection_views(
        &mut self,
        original: &Node,
        stride: usize,
        end: usize,
        add_index: &mut usize,
        commands: &mut Vec<slate_doc::SceneCmd>,
    ) {
        let binding = slate_doc::agent_chat::agent(original).unwrap();
        let mut parent = binding.chat.parent;
        let mut i = binding.chat.start;
        while i < end {
            let chunk_end = (i + stride).min(end);
            let last = chunk_end == end;
            let mut node = if last {
                original.clone()
            } else {
                self.doc_mut().scene.build_duplicate(original, 0.0, 0.0)
            };
            node.hidden = false;
            node.rect.w = slate_doc::agent_chat::CARD_WIDTH;
            node.rect.h = if stride > 1 {
                slate_doc::agent_chat::PAIR_HEIGHT
            } else {
                slate_doc::agent_chat::CARD_HEIGHT
            };
            if let NodeKind::Portal(p) = &mut node.kind {
                if let Some(a) = &mut p.agent {
                    a.chat.train = true;
                    a.chat.bundled.clear();
                    a.chat.bundle_layers.clear();
                    a.chat.detail = if stride > 1 {
                        slate_doc::agent_chat::Detail::Pair
                    } else {
                        slate_doc::agent_chat::Detail::Summary
                    };
                    a.chat.parent = parent;
                    a.chat.start = i;
                    if !last {
                        a.chat.end = Some(chunk_end);
                    }
                }
            }
            parent = Some(node.id);
            i = chunk_end;
            if last {
                commands.push(slate_doc::SceneCmd::Patch {
                    before: Box::new(original.clone()),
                    after: Box::new(node),
                });
            } else {
                commands.push(slate_doc::SceneCmd::Add {
                    index: *add_index,
                    node,
                });
                *add_index += 1;
            }
        }
    }
    /// Wires anchored to a card that leaves follow the card that now shows
    /// its first turn, in the same step.
    pub(super) fn retarget_agent_wires(
        &self,
        moved: &HashMap<NodeId, NodeId>,
        commands: &mut Vec<slate_doc::SceneCmd>,
    ) {
        if moved.is_empty() {
            return;
        }
        for n in &self.doc().scene.nodes {
            let NodeKind::Connector(_) = &n.kind else {
                continue;
            };
            let mut after = n.clone();
            let NodeKind::Connector(c) = &mut after.kind else {
                continue;
            };
            let mut changed = false;
            for end in [&mut c.a, &mut c.b] {
                if let slate_doc::scene::ConnectorEnd::Anchored { node, .. } = end {
                    if let Some(host) = moved.get(node) {
                        *node = *host;
                        changed = true;
                    }
                }
            }
            if changed {
                commands.push(slate_doc::SceneCmd::Patch {
                    before: Box::new(n.clone()),
                    after: Box::new(after),
                });
            }
        }
    }
    /// Cards a presentation switch just laid out fit their text over the next
    /// frames. Until anything else edits the board, that settling and the
    /// relayout it needs fold into the switch's journal step, so one Undo
    /// still returns the previous presentation.
    pub(super) fn settle_agent_projection(&mut self, updates: Vec<Node>) -> Vec<Node> {
        let Some((anchor, depth, generation)) = self.agents.projection_settle else {
            return updates;
        };
        let journal = &self.tab().journal;
        if depth != journal.undo_depth() || generation != self.scene_gen || journal.can_redo() {
            self.agents.projection_settle = None;
            return updates;
        }
        let mut rest = Vec::new();
        let mut folded = Vec::new();
        for after in updates {
            if self.tab_mut().journal.fold_into_last(&after) {
                folded.push(after);
            } else {
                rest.push(after);
            }
        }
        if !folded.is_empty() {
            for after in &folded {
                if let Some(n) = self.doc_mut().scene.node_mut(after.id) {
                    *n = after.clone();
                }
            }
            let positions = slate_doc::agent_chat::projection_positions(&self.doc().scene, anchor);
            for (id, p) in positions {
                let Some(mut after) = self.doc().scene.node(id).cloned() else {
                    continue;
                };
                if after.rect.x == p[0] && after.rect.y == p[1] {
                    continue;
                }
                after.rect.x = p[0];
                after.rect.y = p[1];
                if self.tab_mut().journal.fold_into_last(&after) {
                    if let Some(n) = self.doc_mut().scene.node_mut(id) {
                        *n = after.clone();
                    }
                    folded.push(after);
                }
            }
            self.brush_tiles.note_ids(folded.iter().map(|n| n.id));
            self.note_scene_change();
        }
        self.agents.projection_settle = rest.is_empty().then_some((anchor, depth, self.scene_gen));
        rest
    }
    pub(super) fn arrange_agent_projection(
        &self,
        id: NodeId,
        commands: &mut Vec<slate_doc::SceneCmd>,
    ) {
        let mut projected = self.doc().scene.clone();
        for command in commands.iter() {
            projected.apply(command);
        }
        let positions = slate_doc::agent_chat::projection_positions(&projected, id);
        let touched: HashSet<_> = commands
            .iter()
            .filter_map(|c| match c {
                slate_doc::SceneCmd::Patch { after, .. } => Some(after.id),
                slate_doc::SceneCmd::Add { node, .. } => Some(node.id),
                _ => None,
            })
            .collect();
        for id in positions.keys().filter(|id| !touched.contains(id)) {
            let before = self.doc().scene.node(*id).unwrap().clone();
            commands.push(slate_doc::SceneCmd::Patch {
                before: Box::new(before.clone()),
                after: Box::new(before),
            });
        }
        for command in commands {
            let node = match command {
                slate_doc::SceneCmd::Patch { after, .. } => after.as_mut(),
                slate_doc::SceneCmd::Add { node, .. } => node,
                _ => continue,
            };
            if let Some(p) = positions.get(&node.id) {
                node.rect.x = p[0];
                node.rect.y = p[1];
            }
        }
    }
}
