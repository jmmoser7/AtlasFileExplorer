//! Undo, redo, duplicate, and placing items into frames.

use super::*;

impl SlateApp {
    /// Commit a prepared command group through the tab journal.
    pub fn commit_scene(&mut self, cmds: Vec<SceneCmd>) -> bool {
        self.commit_scene_as(cmds, slate_doc::scene::CmdAuthor::Human)
    }

    pub(crate) fn commit_scene_as(
        &mut self,
        cmds: Vec<SceneCmd>,
        author: slate_doc::scene::CmdAuthor,
    ) -> bool {
        let _span = atlas_core::session_log::span("slate.scene.commit");
        atlas_core::session_log::count("slate.scene.cmds", cmds.len() as u32);
        if self.refuse_read_only_edit() {
            return false;
        }
        self.seed_document_colors();
        let colors =
            super::super::board_color::committed_colors(cmds.iter().filter_map(|c| match c {
                SceneCmd::Add { node, .. } => Some((None, node)),
                SceneCmd::Patch { before, after } => Some((Some(before.as_ref()), after.as_ref())),
                SceneCmd::Remove { .. } => None,
                SceneCmd::LayerNodeAdd { node, .. } | SceneCmd::LayerNodeRemove { node, .. } => {
                    Some((None, node))
                }
                SceneCmd::LayerNodePatch { before, after, .. } => {
                    Some((Some(before.as_ref()), after.as_ref()))
                }
            }));
        self.brush_tiles.note_ids(cmds.iter().map(cmd_node_id));
        let tab = self.tab_mut();
        tab.dirty = true;
        let doc = &mut tab.doc;
        let ok = tab.journal.commit_as(&mut doc.scene, cmds, author);
        if ok {
            self.remember_document_colors(colors);
            self.paint_layer_texture_cache.clear();
            let tab = self.tab_mut();
            tab.edits.push(BoardMark::Scene);
            tab.edit_redo.clear();
        }
        self.note_scene_change();
        ok
    }

    /// Fold `cmds` into journal group `token` when it is still the board's
    /// newest undo step, so the step it already is reverts them too; no
    /// new step. False, with nothing changed, otherwise.
    pub(crate) fn amend_scene_group(
        &mut self,
        token: slate_doc::scene::GroupToken,
        cmds: Vec<SceneCmd>,
    ) -> bool {
        let tab = self.tab();
        if tab.read_only
            || !matches!(tab.edits.last(), Some(BoardMark::Scene))
            || !tab.edit_redo.is_empty()
        {
            return false;
        }
        let ids: Vec<NodeId> = cmds.iter().map(cmd_node_id).collect();
        let tab = self.tab_mut();
        let doc = &mut tab.doc;
        let author = slate_doc::scene::CmdAuthor::Human;
        if !tab.journal.amend_top(&mut doc.scene, token, &author, cmds) {
            return false;
        }
        tab.dirty = true;
        self.brush_tiles.note_ids(ids);
        self.paint_layer_texture_cache.clear();
        self.note_scene_change();
        true
    }

    pub(super) fn undo_scene_journal(&mut self) -> Option<usize> {
        let depth_before = self.tab().journal.undo_depth();
        let tab = self.tab_mut();
        if tab.journal.undo(&mut tab.doc.scene) {
            tab.dirty = true;
            Some(depth_before)
        } else {
            None
        }
    }

    pub(super) fn redo_scene_journal(&mut self) -> Option<usize> {
        let tab = self.tab_mut();
        if tab.journal.redo(&mut tab.doc.scene) {
            tab.dirty = true;
            Some(tab.journal.undo_depth())
        } else {
            None
        }
    }

    pub fn board_undo(&mut self) {
        let _span = atlas_core::session_log::span("slate.scene.undo");
        if self.refuse_read_only_edit() {
            return;
        }
        if self.bezier_draft_undo() {
            return;
        }
        if self.undo_brush_setting() {
            return;
        }
        self.sheet_edit = None;
        match self.tab_mut().edits.pop() {
            Some(BoardMark::Sheet(mark)) => self.revert_sheet_mark(mark, true),
            Some(BoardMark::Locators(mark)) => self.revert_locator_mark(mark, true),
            Some(BoardMark::Scene) => {
                let depth_before = self.undo_scene_journal();
                if depth_before.is_none() && self.tab().journal.can_undo() {
                    self.tab_mut().edits.push(BoardMark::Scene);
                    self.toast("Couldn't undo that step");
                    return;
                }
                self.tab_mut().edit_redo.push(BoardMark::Scene);
                if let Some(depth) = depth_before {
                    self.deck.note_scene_undo(depth);
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
            None => {
                let depth_before = self.undo_scene_journal();
                if depth_before.is_none() && self.tab().journal.can_undo() {
                    self.toast("Couldn't undo that step");
                    return;
                }
                self.tab_mut().edit_redo.clear();
                if let Some(depth) = depth_before {
                    self.deck.note_scene_undo(depth);
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
        }
    }

    pub fn board_redo(&mut self) {
        let _span = atlas_core::session_log::span("slate.scene.redo");
        if self.refuse_read_only_edit() {
            return;
        }
        if self.bezier_draft_redo() {
            return;
        }
        self.sheet_edit = None;
        match self.tab_mut().edit_redo.pop() {
            Some(BoardMark::Sheet(mark)) => self.revert_sheet_mark(mark, false),
            Some(BoardMark::Locators(mark)) => self.revert_locator_mark(mark, false),
            Some(BoardMark::Scene) => {
                if let Some(depth) = self.redo_scene_journal() {
                    self.tab_mut().edits.push(BoardMark::Scene);
                    self.deck.note_scene_redo(depth);
                } else if self.tab().journal.can_redo() {
                    self.tab_mut().edit_redo.push(BoardMark::Scene);
                    self.toast("Couldn't redo that step");
                    return;
                } else {
                    self.tab_mut().edits.push(BoardMark::Scene);
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
            None => {
                if let Some(depth) = self.redo_scene_journal() {
                    self.deck.note_scene_redo(depth);
                } else if self.tab().journal.can_redo() {
                    self.toast("Couldn't redo that step");
                    return;
                }
                self.last_board_edit = None;
                self.note_scene_change();
            }
        }
    }

    pub(super) fn revert_sheet_mark(&mut self, mark: SheetMark, to_redo: bool) {
        let Some(prior) =
            atlas_core::table::revert_sheet_cell(&mark.path, mark.row, mark.col, &mark.prior)
        else {
            self.toast("Couldn't change that spreadsheet");
            let tab = self.tab_mut();
            if to_redo {
                tab.edits.push(BoardMark::Sheet(mark));
            } else {
                tab.edit_redo.push(BoardMark::Sheet(mark));
            }
            return;
        };
        self.sheets.remove(&mark.item);
        let inverted = SheetMark { prior, ..mark };
        let tab = self.tab_mut();
        if to_redo {
            tab.edit_redo.push(BoardMark::Sheet(inverted));
        } else {
            tab.edits.push(BoardMark::Sheet(inverted));
        }
    }

    pub(super) fn revert_locator_mark(&mut self, mark: slate_doc::RewriteLocators, to_redo: bool) {
        let inverse = mark.inverted();
        if !inverse.apply(self.doc_mut()) {
            self.toast("Couldn't restore those asset links");
            let tab = self.tab_mut();
            if to_redo {
                tab.edits.push(BoardMark::Locators(mark));
            } else {
                tab.edit_redo.push(BoardMark::Locators(mark));
            }
            return;
        }
        let tab = self.tab_mut();
        tab.dirty = true;
        if to_redo {
            tab.edit_redo.push(BoardMark::Locators(inverse));
        } else {
            tab.edits.push(BoardMark::Locators(inverse));
        }
        self.note_scene_change();
    }

    /// Duplicate nodes in place with a small offset; selects the copies.
    pub fn duplicate_board_nodes(&mut self, ids: &[NodeId], dx: f32, dy: f32) -> Vec<NodeId> {
        let sources: Vec<Node> = ids
            .iter()
            .filter_map(|id| self.doc().scene.node(*id).cloned())
            .collect();
        if sources.is_empty() {
            return Vec::new();
        }
        let dups: Vec<Node> = {
            let scene = &mut self.doc_mut().scene;
            let mut dups: Vec<Node> = sources
                .iter()
                .map(|n| scene.build_duplicate(n, dx, dy))
                .collect();
            // Copies form their own groups (scene-flags spec).
            super::super::board_flags::remap_dup_group_keys(scene, &mut dups);
            dups
        };
        let new_ids = self.add_nodes(dups);
        if !new_ids.is_empty() {
            self.board_sel = new_ids.iter().copied().collect();
        }
        new_ids
    }

    /// Alt held at the start of a scale copies, then the gesture edits the
    /// copies. Ctrl+Alt+Shift stays the group layout-scale chord and does
    /// not copy.
    pub(crate) fn alt_scale_copies(&self) -> bool {
        self.alt_down && !(self.ctrl_down && self.shift_down)
    }

    /// Insert copies above their sources without journaling. Alt-drag and
    /// Alt-scale journal the Adds on release, at the final geometry.
    /// Selects the copies.
    pub(crate) fn stage_unjournaled_duplicates(
        &mut self,
        sources: &[Node],
    ) -> (Vec<NodeId>, Vec<Node>) {
        let mut ids = Vec::new();
        let mut before = Vec::new();
        if sources.is_empty() {
            return (ids, before);
        }
        let scene = &mut self.doc_mut().scene;
        let mut dups: Vec<Node> = sources
            .iter()
            .map(|n| scene.build_duplicate(n, 0.0, 0.0))
            .collect();
        super::super::board_flags::remap_dup_group_keys(scene, &mut dups);
        for d in dups {
            ids.push(d.id);
            before.push(d.clone());
            scene.nodes.push(d);
        }
        let sources_sel = std::mem::replace(&mut self.board_sel, ids.iter().copied().collect());
        self.staged_dup_sel = Some(sources_sel);
        (ids, before)
    }

    /// Esc during a drag that edits nodes live. egui drops its drag on Esc,
    /// so no release follows: every node returns to its press-time state,
    /// staged Alt copies leave the scene with the selection going back to
    /// their sources, and nothing is journaled (P0.1). False when the drag
    /// edits no nodes.
    pub(crate) fn cancel_node_drag(&mut self) -> bool {
        if !self
            .board_drag
            .as_ref()
            .is_some_and(BoardDrag::edits_nodes_live)
        {
            return false;
        }
        let Some(drag) = self.board_drag.take() else {
            return false;
        };
        let is_move = matches!(drag, BoardDrag::Move { .. });
        let Some((before, dup)) = drag.into_press_nodes() else {
            return false;
        };
        let bumped = is_move && self.bumper.dragging();
        let sources_sel = self.staged_dup_sel.take();
        self.restore_press_nodes(before, dup, sources_sel);
        if bumped {
            // The bodies the move pushed go back too.
            self.cancel_bumper_drag(Vec::new());
        }
        if is_move {
            self.image_drop = None;
        }
        true
    }

    /// Undo a node drag's live edits without journaling: staged Alt copies
    /// leave the scene and the selection returns to their sources
    /// (`sources_sel`); any other node takes its press-time state. Release
    /// paths that commit something else (a saved view, an image drop) call
    /// this first, so only their own command reaches the journal.
    pub(crate) fn restore_press_nodes(
        &mut self,
        before: Vec<Node>,
        dup: bool,
        sources_sel: Option<HashSet<NodeId>>,
    ) {
        if dup {
            let copies: HashSet<NodeId> = before.iter().map(|n| n.id).collect();
            self.doc_mut()
                .scene
                .nodes
                .retain(|n| !copies.contains(&n.id));
            if let Some(sel) = sources_sel {
                self.board_sel = sel;
            }
        } else {
            let scene = &mut self.doc_mut().scene;
            for node in before {
                if let Some(live) = scene.node_mut(node.id) {
                    *live = node;
                }
            }
        }
        self.note_scene_change();
    }

    pub(super) fn journal_alt_copies(&mut self, ids: &[NodeId], note: String) {
        let cmds: Vec<SceneCmd> = ids
            .iter()
            .filter_map(|id| {
                let index = self.doc().scene.index_of(*id)?;
                let node = self.doc().scene.node(*id)?.clone();
                Some(SceneCmd::Add { index, node })
            })
            .collect();
        if cmds.is_empty() {
            return;
        }
        self.tab_mut().journal.record(cmds);
        self.tab_mut().dirty = true;
        self.push_history(atlas_commands::CommandId("board.duplicate"), Some(note));
    }

    pub(super) fn journal_resize_patches(&mut self, ids: &[NodeId], before: Vec<Node>) {
        let cmds: Vec<SceneCmd> = ids
            .iter()
            .zip(before)
            .filter_map(|(id, b)| {
                let after = self.doc().scene.node(*id)?.clone();
                (after != b).then(|| SceneCmd::Patch {
                    before: Box::new(b),
                    after: Box::new(after),
                })
            })
            .collect();
        if !cmds.is_empty() {
            self.tab_mut().journal.record(cmds);
            self.tab_mut().dirty = true;
        }
    }

    /// Place image nodes for pool items at a world position, one undo group.
    /// A single item lands centered on the drop point; 2+ items are laid out
    /// in a grid (max 10 columns) centered on it. Items whose center lands
    /// inside a tagged frame inherit its tags.
    pub fn place_items_on_board(&mut self, items: &[ItemId], at: Pos2) {
        if items.is_empty() {
            return;
        }
        let sizes = self.image_natural_sizes(items);
        let rects = grid_drop_rects(&sizes, at);
        let mut nodes = Vec::new();
        {
            let scene = &mut self.doc_mut().scene;
            for (i, item) in items.iter().enumerate() {
                let img = ImageNode::new(*item);
                nodes.push(scene.build_node(rects[i], NodeKind::Image(img)));
            }
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.iter().copied().collect();
        self.inherit_frame_tags_after_move(&ids);
    }

    /// Place pool items as image nodes arranged inside a frame, inheriting
    /// its tags (frame toolbar "+ images" and Atlas drops onto frames).
    pub fn place_items_in_frame(&mut self, frame: NodeId, items: &[ItemId]) {
        let Some(rect) = self.doc().scene.node(frame).map(|n| n.rect) else {
            // Frame vanished — fall back to a plain board drop at origin.
            self.place_items_on_board(items, Pos2::new(0.0, 0.0));
            return;
        };
        if items.is_empty() {
            return;
        }
        let pad = 24.0f32;
        let cols = (items.len() as f32).sqrt().ceil().max(1.0) as usize;
        let cell_w = ((rect.w - pad * 2.0) / cols as f32).clamp(60.0, IMAGE_W);
        let cell_h = cell_w * (IMAGE_H / IMAGE_W);
        let mut nodes = Vec::new();
        {
            let scene = &mut self.doc_mut().scene;
            for (i, item) in items.iter().enumerate() {
                let col = (i % cols) as f32;
                let row = (i / cols) as f32;
                let r = WorldRect::new(
                    rect.x + pad + col * (cell_w + 8.0),
                    rect.y + pad + row * (cell_h + 8.0),
                    cell_w,
                    cell_h,
                );
                let img = ImageNode::new(*item);
                nodes.push(scene.build_node(r, NodeKind::Image(img)));
            }
        }
        let ids = self.add_nodes(nodes);
        self.board_sel = ids.iter().copied().collect();
        self.apply_frame_tags(frame, items);
    }

    /// Apply a frame's tag assignments to pool items (drop inheritance).
    pub fn apply_frame_tags(&mut self, frame_id: NodeId, items: &[ItemId]) {
        let tags: Vec<slate_doc::TagId> = match self.doc().scene.node(frame_id).map(|n| &n.kind) {
            Some(NodeKind::Frame(f)) => f.assignments.values().copied().collect(),
            _ => return,
        };
        if tags.is_empty() {
            return;
        }
        for item in items {
            for tag in &tags {
                self.doc_mut().assign(*item, *tag);
            }
        }
        self.publish_session_tags();
    }

    /// Selection expanded so selected frames carry their members. Hidden and
    /// locked members stay put, and connectors never ride along (their
    /// geometry is derived from their endpoints — frame membership does not
    /// apply to them).
    pub(crate) fn expand_with_members(&self, ids: &[NodeId]) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = ids.to_vec();
        for id in ids {
            if self.doc().scene.node(*id).map(|n| n.is_frame()) == Some(true) {
                for m in self.doc().scene.members_of(*id) {
                    let skip = self.doc().scene.node(m).is_none_or(|n| {
                        n.hidden || n.locked || matches!(n.kind, NodeKind::Connector(_))
                    });
                    if !skip && !out.contains(&m) {
                        out.push(m);
                    }
                }
            }
        }
        out
    }
}
