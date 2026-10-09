//! Journaled scene patch, add, and delete.

use super::*;

impl SlateApp {
    // ----- journaled mutations -------------------------------------------------

    /// Bump the cheap scene-content generation. Call at every journal
    /// commit/record/undo/redo site (and on tab switches) — it keys the
    /// minimap's cached texture and the search-match recompute.
    pub(crate) fn note_scene_change(&mut self) {
        self.scene_gen = self.scene_gen.wrapping_add(1);
        self.brush_tiles.note_unspecified();
    }

    /// Settles `after` into the newest journal step when that step already
    /// adds or patches the node and nothing waits to redo, and applies it to
    /// the scene, so one Undo still returns the whole gesture. `false`
    /// changes nothing; the caller journals the edit on its own.
    pub(crate) fn fold_into_last_step(&mut self, after: &Node) -> bool {
        if self.tab().journal.can_redo() || !self.tab_mut().journal.fold_into_last(after) {
            return false;
        }
        if let Some(node) = self.doc_mut().scene.node_mut(after.id) {
            *node = after.clone();
        }
        self.note_scene_change();
        true
    }

    /// Applies an edit to several nodes and journals one coalescible patch
    /// group (continuous slider scrubs collapse into a single undo step).
    pub fn patch_nodes(&mut self, ids: &[NodeId], f: impl Fn(&mut Node)) {
        let _span = atlas_core::session_log::span("slate.scene.patch");
        if self.refuse_read_only_edit() {
            return;
        }
        self.seed_document_colors();
        let mut befores = Vec::new();
        let mut afters = Vec::new();
        for id in ids {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let mut after = before.clone();
            f(&mut after);
            if after != before {
                befores.push(before);
                afters.push(after);
            }
        }
        if afters.is_empty() {
            return;
        }
        {
            let scene = &mut self.doc_mut().scene;
            for a in &afters {
                if let Some(n) = scene.node_mut(a.id) {
                    *n = a.clone();
                }
            }
        }
        let colors = super::super::board_color::committed_colors(
            befores.iter().zip(&afters).map(|(b, a)| (Some(b), a)),
        );
        let first = afters[0].id;
        let coalesce = matches!(
            self.last_board_edit,
            Some((id, t)) if id == first && t.elapsed() < COALESCE
        );
        let tab = self.tab_mut();
        let amended = coalesce && afters.len() == 1 && tab.journal.amend_last_patch(&afters[0]);
        if !amended {
            let cmds: Vec<SceneCmd> = befores
                .into_iter()
                .zip(afters.iter())
                .map(|(b, a)| SceneCmd::Patch {
                    before: Box::new(b),
                    after: Box::new(a.clone()),
                })
                .collect();
            tab.journal.record(cmds);
        }
        self.remember_document_colors(colors);
        self.last_board_edit = Some((first, Instant::now()));
        self.brush_tiles.note_ids(afters.iter().map(|n| n.id));
        self.note_scene_change();
        if afters.len() == 1 {
            self.note_last_style(&afters[0]);
        }
    }

    /// Drawing-tool commit: paint layer when hosting, else z-list `Add`.
    pub(crate) fn commit_created_nodes(&mut self, nodes: Vec<Node>) -> Vec<NodeId> {
        if self.image_paint.is_some() {
            return self.commit_paint_layer_nodes(nodes).unwrap_or_default();
        }
        self.add_nodes(nodes)
    }

    pub(crate) fn select_created_nodes(&mut self, ids: Vec<NodeId>) {
        self.board_sel.clear();
        if let Some(host) = self.paint_layer_host() {
            self.board_sel.insert(host);
        } else {
            self.board_sel.extend(ids);
        }
    }

    /// Insert new nodes as one undo group. Returns their ids.
    pub fn add_nodes(&mut self, nodes: Vec<Node>) -> Vec<NodeId> {
        if self.refuse_read_only_edit() {
            return Vec::new();
        }
        if nodes.is_empty() {
            return Vec::new();
        }
        let ids: Vec<NodeId> = nodes.iter().map(|n| n.id).collect();
        let base = self.doc().scene.nodes.len();
        let cmds: Vec<SceneCmd> = nodes
            .into_iter()
            .enumerate()
            .map(|(i, node)| SceneCmd::Add {
                index: base + i,
                node,
            })
            .collect();
        if self.commit_scene(cmds) {
            self.note_view_wires_added(&ids);
            ids
        } else {
            Vec::new()
        }
    }

    pub fn delete_board_nodes(&mut self, ids: &[NodeId]) {
        if self.refuse_read_only_edit() {
            return;
        }
        // Context an agent card already sent retracts into that card instead.
        let leaving = slate_doc::agent_chat::subtree(&self.doc().scene, ids);
        let mut suck = Vec::new();
        let mut plain = Vec::new();
        for id in ids {
            let used = slate_doc::agent_inputs::consumed_by(&self.doc().scene, *id)
                .iter()
                .any(|(_, card)| !leaving.contains(card));
            if self.machine_context_link(*id).is_some() || used {
                suck.push(*id);
            } else {
                plain.push(*id);
            }
        }
        if !suck.is_empty() {
            self.retract_context(&suck, &leaving);
        }
        if plain.is_empty() {
            return;
        }
        self.delete_board_nodes_now(&plain);
    }

    pub(crate) fn delete_board_nodes_now(&mut self, ids: &[NodeId]) {
        if self.refuse_read_only_edit() {
            return;
        }
        // A deleted crosswire ends its crosstalk there but stays, hidden, so
        // the message it carried keeps its provenance (crosstalk D17).
        let ended_wires: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.doc()
                    .scene
                    .node(*id)
                    .and_then(slate_doc::crosstalk::crosstalk)
                    .is_some()
            })
            .collect();
        let ids: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| !ended_wires.contains(id))
            .collect();
        let ids = &ids[..];
        let mut deleted = slate_doc::agent_chat::subtree(&self.doc().scene, ids);
        // Pocketed context whose every consuming card goes too would be an
        // invisible orphan. It leaves in the same commit, with those wires.
        let orphans: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| n.hidden && !deleted.contains(&n.id))
            .flat_map(|n| {
                let sent = slate_doc::agent_inputs::consumed_by(&self.doc().scene, n.id);
                let orphaned = !sent.is_empty() && sent.iter().all(|(_, c)| deleted.contains(c));
                orphaned
                    .then(|| std::iter::once(n.id).chain(sent.into_iter().map(|(wire, _)| wire)))
                    .into_iter()
                    .flatten()
            })
            .collect();
        deleted.extend(orphans);
        // A crosswire leaves with a card it is anchored to (views only; the
        // provider keeps the message).
        let crosswires: Vec<NodeId> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| slate_doc::crosstalk::crosstalk(n).is_some())
            .filter(|n| match &n.kind {
                NodeKind::Connector(c) => [&c.a, &c.b].into_iter().any(|end| {
                    slate_doc::agent_inputs::endpoint_node(end)
                        .is_some_and(|id| deleted.contains(&id))
                }),
                _ => false,
            })
            .map(|n| n.id)
            .collect();
        deleted.extend(crosswires);
        let ids: Vec<_> = deleted.iter().copied().collect();
        self.stop_pruned_agent_runs(&ids);
        self.agents.text_output.retire(&ids);
        // Surviving connectors anchored to a deleted node degrade to `Free`
        // at their last world position — same command group, so undo
        // restores the anchor (connectors spec).
        let mut cmds: Vec<SceneCmd> = Vec::new();
        for n in &self.doc().scene.nodes {
            if deleted.contains(&n.id) {
                continue;
            }
            let NodeKind::Connector(_) = &n.kind else {
                continue;
            };
            let mut after = n.clone();
            let NodeKind::Connector(ca) = &mut after.kind else {
                unreachable!();
            };
            let mut changed = false;
            for end in [&mut ca.a, &mut ca.b] {
                if let slate_doc::scene::ConnectorEnd::Anchored { node, side, t } = *end {
                    if deleted.contains(&node) {
                        let p = self
                            .doc()
                            .scene
                            .node(node)
                            .map(|nn| slate_doc::connector_anchor_on(nn, side, t))
                            .unwrap_or([0.0, 0.0]);
                        *end = slate_doc::scene::ConnectorEnd::Free { point: p };
                        changed = true;
                    }
                }
            }
            if changed {
                cmds.push(SceneCmd::Patch {
                    before: Box::new(n.clone()),
                    after: Box::new(after),
                });
            }
        }
        for id in &ended_wires {
            let Some(before) = self.doc().scene.node(*id).cloned() else {
                continue;
            };
            let mut after = before.clone();
            after.hidden = true;
            if let Some(x) = slate_doc::crosstalk::crosstalk_mut(&mut after) {
                x.ended = true;
            }
            cmds.push(SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(after),
            });
            self.board_sel.remove(id);
        }
        // Remove in descending index order so recorded indices stay valid on
        // revert (revert_all replays in reverse).
        let mut idx: Vec<(usize, Node)> = ids
            .iter()
            .filter_map(|id| {
                let i = self.doc().scene.index_of(*id)?;
                Some((i, self.doc().scene.node(*id)?.clone()))
            })
            .collect();
        idx.sort_by_key(|(i, _)| std::cmp::Reverse(*i));
        if idx.is_empty() && cmds.is_empty() {
            return;
        }
        cmds.extend(
            idx.into_iter()
                .map(|(index, node)| SceneCmd::Remove { index, node }),
        );
        if self.commit_scene(cmds) {
            self.forget_deleted_agent_cards(&ids);
        }
        for id in &ids {
            self.board_sel.remove(id);
        }
    }
}
