//! Text drafts, finish-draw, and portal builders.

use super::*;

impl SlateApp {
    pub(crate) fn text_compose_active(&self) -> bool {
        self.text_edit.is_some() || self.text_box_draft.is_some() || self.text_doc_edit.is_some()
    }

    /// Folder for text documents typed on this board: `slate-outputs/<board>/text`
    /// beside the saved workbook, else in the AI workspace for an unsaved one.
    pub(super) fn text_document_dir(&self) -> Option<PathBuf> {
        let (base, board) = match self.tab().path.as_deref() {
            Some(path) => (
                path.parent()?.to_path_buf(),
                path.file_stem()?.to_string_lossy().into_owned(),
            ),
            None => (
                self.ai.config.valid_workspace()?.to_path_buf(),
                "untitled-board".to_string(),
            ),
        };
        Some(base.join("slate-outputs").join(board).join("text"))
    }

    /// Grasshopper panel entry: link a fresh blank `.txt` card at `world`
    /// (one undo step) and put the caret in it.
    pub(crate) fn place_text_document_at(&mut self, world: Pos2) -> bool {
        let Some(dir) = self.text_document_dir() else {
            self.toast("Save the workbook or choose an AI workspace to create a text document");
            return false;
        };
        self.finish_text_compose_on_click_away();
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let name = format!("text-{millis}.txt");
        let path = dir.join(&name);
        let key = atlas_core::thumbs::cache_key(&path.to_string_lossy(), 0, 0);
        let item = self.doc_mut().add_item(path.clone(), name, 0, 0, key);
        self.snippets.insert(item, Some(String::new()));
        self.board_sel.clear();
        self.place_items_on_board(&[item], world);
        let Some(node) = self.board_sel.iter().next().copied() else {
            return false;
        };
        self.agents
            .text_output
            .write_now(node, item, path.clone(), String::new());
        self.text_doc_edit = Some(TextDocEdit {
            node,
            item,
            path,
            buffer: String::new(),
            claim_focus: true,
        });
        self.push_history(
            atlas_commands::CommandId("board.media.text_new"),
            Some("placed".into()),
        );
        true
    }

    /// Write the typed words to the linked file and leave the editor.
    pub(crate) fn commit_text_doc_edit(&mut self) {
        let Some(edit) = self.text_doc_edit.take() else {
            return;
        };
        self.snippets.insert(edit.item, Some(edit.buffer.clone()));
        self.agents
            .text_output
            .write_now(edit.node, edit.item, edit.path, edit.buffer);
    }

    pub(super) fn default_text_click_rect(anchor: Pos2) -> WorldRect {
        WorldRect::new(
            anchor.x,
            anchor.y - TEXT_BOX_DEFAULT_SIZE * 0.75,
            MIN_DRAW.max(2.0),
            TEXT_BOX_DEFAULT_SIZE * 1.25,
        )
    }

    /// Start composing a text box at `anchor`. Any prior non-empty draft commits first.
    pub(crate) fn begin_text_box_draft(
        &mut self,
        _anchor: Pos2,
        rect: WorldRect,
        fixed_width: bool,
    ) {
        self.commit_text_edit();
        if let Some(draft) = self.text_box_draft.take() {
            if draft.buffer.is_empty() {
                // Replace the empty compose.
            } else {
                self.text_box_draft = Some(draft);
                self.commit_text_box_draft();
            }
        }
        let color = self.color_for_new_text();
        self.text_box_draft = Some(TextBoxDraft {
            rect,
            buffer: String::new(),
            color,
            family: Typeface::Sans,
            size: TEXT_BOX_DEFAULT_SIZE,
            align: TextAlign::Left,
            fixed_width,
        });
        self.board_sel.clear();
    }

    pub(crate) fn cancel_text_box_draft(&mut self) {
        self.text_box_draft = None;
    }

    /// Journaled add when the draft buffer is non-empty; no-op otherwise.
    pub(crate) fn commit_text_box_draft(&mut self) {
        let Some(draft) = self.text_box_draft.take() else {
            return;
        };
        if draft.buffer.is_empty() {
            return;
        }
        let node = self.doc_mut().scene.build_node(
            draft.rect,
            NodeKind::Text(TextNode {
                text: draft.buffer,
                family: draft.family,
                size: draft.size,
                color: draft.color,
                align: draft.align,
                fill: None,
                stroke: Default::default(),
                agent: None,
            }),
        );
        let ids = self.commit_created_nodes(vec![node]);
        self.select_created_nodes(ids);
        self.board_tool = BoardTool::Select;
        self.push_history(
            atlas_commands::CommandId("board.tool.text"),
            Some("placed".into()),
        );
    }

    pub(super) fn finish_text_box_place_gesture(
        &mut self,
        start_world: Pos2,
        end_world: Pos2,
        start_screen: Pos2,
        pointer: Option<Pos2>,
        mods: egui::Modifiers,
    ) {
        let travel_px = pointer
            .map(|p| (p - start_screen).length())
            .unwrap_or_else(|| (end_world - start_world).length() * self.board_xf().z);
        if travel_px <= board_place::place_tokens::DRAG_THRESHOLD {
            self.begin_text_box_draft(
                start_world,
                Self::default_text_click_rect(start_world),
                false,
            );
            return;
        }
        let rect = self.resolve_draw_rect(
            start_world,
            end_world,
            BoardTool::Text,
            mods.shift,
            board_place::draws_from_center(BoardTool::Text, mods.ctrl),
        );
        if rect.w < MIN_DRAW && rect.h < MIN_DRAW {
            return;
        }
        self.begin_text_box_draft(start_world, rect, true);
    }

    pub(super) fn refresh_text_box_draft_rect(
        &mut self,
        ctx: &egui::Context,
        _xf: &BoardXf,
        draft: &TextBoxDraft,
    ) {
        if draft.fixed_width || draft.buffer.is_empty() {
            return;
        }
        let layout = canvas_text::world_layout(
            ctx,
            &draft.buffer,
            typeface_font(draft.family, draft.size),
            f32::INFINITY,
            egui_align(draft.align),
        );
        // Two points of padding on each side, in world units.
        let w = (layout.width + 4.0).max(MIN_DRAW.max(2.0));
        let h = (layout.height + 4.0).max(TEXT_BOX_DEFAULT_SIZE * 1.25);
        if !(w.is_finite() && h.is_finite()) {
            debug_assert!(false, "text draft measured a non-finite size: {w} x {h}");
            return;
        }
        if let Some(live) = self.text_box_draft.as_mut() {
            live.rect.w = w;
            live.rect.h = h;
        }
    }

    pub(super) fn paint_text_box_draft(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        draft: &TextBoxDraft,
    ) {
        let sr = xf.rect_w2s(draft.rect);
        let stroke = rgba32(to_rgba(self.palette().ink)).gamma_multiply(0.55);
        let w = canvas_scale::px(1.0, xf.z);
        painter.rect_stroke(sr, 0.0, EStroke::new(w, stroke), egui::StrokeKind::Inside);
    }

    /// Click-to-compose text at a world point (Text tool / palette).
    pub(crate) fn place_text_at(&mut self, world: Pos2) {
        self.begin_text_box_draft(world, Self::default_text_click_rect(world), false);
    }

    #[cfg(test)]
    pub(crate) fn finish_draw(&mut self, a: Pos2, b: Pos2, tool: BoardTool, mods: egui::Modifiers) {
        let r = self.draw_world_rect(
            a,
            b,
            tool,
            mods.shift,
            board_place::draws_from_center(tool, mods.ctrl),
        );
        self.commit_draw_rect(r, tool);
    }

    pub(super) fn commit_draw_rect(&mut self, r: WorldRect, tool: BoardTool) {
        if r.w < MIN_DRAW && r.h < MIN_DRAW {
            self.disarm_create();
            return;
        }
        // A catalog duplicate uses its own recipe; built-in web/agent keep
        // their dedicated place paths (start locator, agent session).
        if self.armed_kit_id.is_some() {
            if let Some(recipe) = self.active_recipe(tool) {
                let nodes = self.instantiate_recipe_nodes(&recipe, r);
                if nodes.is_empty() {
                    self.disarm_create();
                    return;
                }
                if let Some(n) = nodes.first() {
                    self.note_last_style(n);
                }
                let ids = self.commit_created_nodes(nodes);
                self.select_created_nodes(ids);
                if let Some(id) = Self::draw_command_id(tool) {
                    self.push_history(atlas_commands::CommandId(id), Some("drawn".into()));
                }
                self.disarm_create();
                return;
            }
        }
        if tool == BoardTool::AgentPortal {
            self.add_agent_portal(r, "drawn");
            return;
        }
        if tool == BoardTool::WebPortal {
            self.add_web_portal(r, Some(board_web::WEB_START_LOCATOR.to_string()), "drawn");
            return;
        }
        // What the gesture produces is the tool's recipe, read from the kit
        // registry. Nothing here knows what a rectangle looks like.
        let Some(recipe) = self.kits.recipe_for(tool).cloned() else {
            self.disarm_create();
            return;
        };
        let nodes = self.instantiate_recipe_nodes(&recipe, r);
        if nodes.is_empty() {
            self.disarm_create();
            return;
        }
        if let Some(n) = nodes.first() {
            self.note_last_style(n);
        }
        let ids = self.commit_created_nodes(nodes);
        self.select_created_nodes(ids);
        if let Some(id) = Self::draw_command_id(tool) {
            self.push_history(atlas_commands::CommandId(id), Some("drawn".into()));
        }
        self.disarm_create();
    }

    /// Journal entry a completed draw is recorded under.
    pub(super) fn draw_command_id(tool: BoardTool) -> Option<&'static str> {
        match tool {
            BoardTool::Frame => Some("board.tool.frame"),
            BoardTool::AgentPortal => Some("board.portal.agent"),
            BoardTool::WebPortal => Some("board.portal.web"),
            BoardTool::AtlasPortal => Some("board.portal.atlas"),
            BoardTool::SlatePortal => Some("board.portal.slate"),
            BoardTool::RectShape => Some("board.tool.rect"),
            BoardTool::Ellipse => Some("board.tool.ellipse"),
            BoardTool::Polygon => Some("board.tool.polygon"),
            _ => None,
        }
    }

    /// An agent portal before a program is chosen: it shows the program grid.
    pub(crate) fn build_agent_portal(&mut self, rect: WorldRect) -> Node {
        self.doc_mut().scene.build_node(
            rect,
            NodeKind::Portal(PortalNode::unbound_agent("Agent portal", "")),
        )
    }

    pub(super) fn add_agent_portal(&mut self, rect: WorldRect, detail: &'static str) {
        let node = self.build_agent_portal(rect);
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.disarm_create();
        self.push_history(
            atlas_commands::CommandId("board.portal.agent"),
            Some(detail.into()),
        );
    }

    /// Commit one web portal. Default place/draw and the drop-in entry paths
    /// bind at placement; `None` is kept for explicit "clear source" paths.
    pub(crate) fn add_web_portal(
        &mut self,
        rect: WorldRect,
        locator: Option<String>,
        detail: &'static str,
    ) -> NodeId {
        self.add_web_portal_with_entry(rect, locator, None, detail)
    }

    pub(crate) fn build_web_portal(
        &mut self,
        rect: WorldRect,
        locator: Option<String>,
        entry: Option<String>,
    ) -> Node {
        let mut portal = match &locator {
            Some(locator) => {
                let title = slate_doc::scene::web_display_locator(locator);
                PortalNode::bound_web(title, locator.clone())
            }
            None => PortalNode::unbound_web("Web portal"),
        };
        if let Some(entry) = entry {
            if let Some(web) = &mut portal.web {
                web.entry = entry;
            }
        }
        // Dropping or pasting a page is the human permitting its origin, the
        // same as binding one from the inspector (D32).
        if let Some(locator) = &locator {
            self.grant_web_consent(locator);
        }
        self.doc_mut()
            .scene
            .build_node(rect, NodeKind::Portal(portal))
    }

    /// Like [`Self::add_web_portal`], with an explicit directory entry file
    /// (`index.html` / `index.htm`) so a dropped dashboard folder binds to the
    /// file it actually holds.
    pub(crate) fn add_web_portal_with_entry(
        &mut self,
        rect: WorldRect,
        locator: Option<String>,
        entry: Option<String>,
        detail: &'static str,
    ) -> NodeId {
        let node = self.build_web_portal(rect, locator, entry);
        let id = node.id;
        self.add_nodes(vec![node]);
        self.board_sel.clear();
        self.board_sel.insert(id);
        self.disarm_create();
        self.push_history(
            atlas_commands::CommandId("board.portal.web"),
            Some(detail.into()),
        );
        id
    }
}
