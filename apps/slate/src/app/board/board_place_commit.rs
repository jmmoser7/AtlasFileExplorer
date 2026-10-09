//! Draw-rect resolve and click-place.

use super::*;

impl SlateApp {
    /// Images that ended a move inside a tagged frame inherit its tags.
    pub(crate) fn inherit_frame_tags_after_move(&mut self, ids: &[NodeId]) {
        let mut per_frame: BTreeMap<NodeId, Vec<ItemId>> = BTreeMap::new();
        for id in ids {
            let Some(n) = self.doc().scene.node(*id) else {
                continue;
            };
            let NodeKind::Image(img) = &n.kind else {
                continue;
            };
            let (cx, cy) = n.rect.center();
            if let Some(frame) = self.doc().scene.frame_at(cx, cy) {
                per_frame.entry(frame).or_default().push(img.item);
            }
        }
        for (frame, items) in per_frame {
            self.apply_frame_tags(frame, &items);
        }
    }

    /// Snap a DragScale live/commit rect. F8 ortho does not apply (area
    /// place; Shift is aspect). Object snap on the moving corner wins;
    /// otherwise the growing rect's free edges smart-guide like a resize
    /// so the cursor leaving a target's row does not silence the forcefield.
    pub(crate) fn resolve_draw_rect(
        &mut self,
        start: Pos2,
        raw_end: Pos2,
        tool: BoardTool,
        shift: bool,
        from_center: bool,
    ) -> WorldRect {
        if self.alt_down {
            self.board_osnap_hit = None;
            self.board_point_snap = Some(raw_end);
            let r = self.draw_world_rect(start, raw_end, tool, shift, from_center);
            self.board_draw_rect = Some(r);
            return r;
        }

        let set = self.board_osnap;
        let radius = self.osnap_radius_world();
        if let Some(hit) = board_osnap::pick(
            &self.doc().scene,
            raw_end,
            radius,
            set,
            &[],
            Some(start),
            self.board_wire_routing,
        ) {
            self.board_osnap_hit = Some(hit);
            self.board_point_snap = Some(hit.point);
            let r = self.draw_world_rect(start, hit.point, tool, shift, from_center);
            self.board_draw_rect = Some(r);
            return r;
        }
        self.board_osnap_hit = None;

        let proposed = self.draw_world_rect(start, raw_end, tool, shift, from_center);
        if self.board_smart_guides {
            let all = self.board_node_rects();
            if from_center {
                let (snapped, guides) = board_snap::snap_centered_rect_scoped(
                    start,
                    proposed,
                    &[],
                    &all,
                    self.snap_scope(),
                    shift
                        && matches!(
                            tool,
                            BoardTool::RectShape | BoardTool::Ellipse | BoardTool::Polygon
                        ),
                );
                if !guides.is_empty() {
                    self.board_snap_guides = guides;
                    self.board_point_snap =
                        Some(board_snap::center_corner(start, raw_end, snapped));
                    self.board_draw_rect = Some(snapped);
                    return snapped;
                }
            } else {
                let edges = board_snap::ResizeSnapEdges::for_draw(start, proposed);
                let (snapped, guides) = board_snap::snap_resize_rect_scoped(
                    proposed,
                    &[],
                    &all,
                    self.snap_scope(),
                    edges,
                );
                if !guides.is_empty() {
                    self.board_snap_guides = guides;
                    let end = board_snap::draw_end_from_rect(start, snapped);
                    self.board_point_snap = Some(end);
                    self.board_draw_rect = Some(snapped);
                    return snapped;
                }
            }
        }

        let end = if self.board_snap_grid {
            let g = board_snap::GRID_WORLD;
            Pos2::new((raw_end.x / g).round() * g, (raw_end.y / g).round() * g)
        } else {
            raw_end
        };
        self.board_point_snap = Some(end);
        let r = self.draw_world_rect(start, end, tool, shift, from_center);
        self.board_draw_rect = Some(r);
        r
    }

    /// DragScale world rect for preview and commit. One `PlaceConstraint`
    /// table so future DragRect tools reuse the same Shift / aspect rules.
    pub(super) fn draw_world_rect(
        &self,
        start: Pos2,
        end: Pos2,
        tool: BoardTool,
        shift: bool,
        from_center: bool,
    ) -> WorldRect {
        let frame_aspect = self.board_frame_preset.aspect();
        match board_place::constraint_for(tool, frame_aspect) {
            Some(c) => board_place::place_rect(start, end, c, shift, from_center),
            None => WorldRect::new(start.x, start.y, end.x - start.x, end.y - start.y).normalized(),
        }
    }

    pub(super) fn draw_preview_screen_rect(
        &self,
        xf: &BoardXf,
        start: Pos2,
        end: Pos2,
        tool: BoardTool,
        mods: egui::Modifiers,
    ) -> Rect {
        xf.rect_w2s(self.draw_world_rect(
            start,
            end,
            tool,
            mods.shift,
            board_place::draws_from_center(tool, mods.ctrl),
        ))
    }

    /// Click-to-place the armed DragRect tool at its default size, centred
    /// on `center`. Drag-to-size still goes through [`Self::finish_draw`].
    pub(crate) fn place_default_at(&mut self, tool: BoardTool, center: Pos2) {
        match tool {
            BoardTool::Frame => self.place_frame_at(center),
            BoardTool::AgentPortal => self.place_agent_portal_at(center),
            BoardTool::WebPortal => self.place_web_portal_at(center),
            BoardTool::AtlasPortal => self.place_atlas_portal_at(center),
            BoardTool::SlatePortal => self.place_slate_portal_at(center),
            BoardTool::RectShape => self.place_from_recipe(
                tool,
                center,
                (
                    board_place::place_tokens::RECT_DEFAULT_W,
                    board_place::place_tokens::RECT_DEFAULT_H,
                ),
            ),
            BoardTool::Ellipse => self.place_from_recipe(
                tool,
                center,
                (
                    board_place::place_tokens::ELLIPSE_DEFAULT_W,
                    board_place::place_tokens::ELLIPSE_DEFAULT_H,
                ),
            ),
            BoardTool::Polygon => self.place_from_recipe(tool, center, (160.0, 160.0)),
            _ => {
                self.board_tool = BoardTool::Select;
            }
        }
    }

    /// Click-to-place default frame (Frame tool click, or the canvas
    /// palette placing at its invocation point).
    pub(crate) fn place_frame_at(&mut self, center: Pos2) {
        // Frames alone take their click size from the app's frame preset rather
        // than the recipe — the preset is a live UI choice, not a tool default.
        let (w, h) = self.board_frame_preset.size();
        self.place_from_recipe(BoardTool::Frame, center, (w, h));
    }

    /// Click-to-place default Agent portal (host-class local agent link).
    pub(crate) fn place_agent_portal_at(&mut self, center: Pos2) {
        if self.armed_kit_id.is_some() {
            // An agent portal is a chat card, not a document viewport, so it
            // does not take PORTAL_DEFAULT_W/H.
            self.place_from_recipe(BoardTool::AgentPortal, center, AGENT_PORTAL_SIZE);
            return;
        }
        let (w, h) = self.agent_portal_size();
        let rect = WorldRect::new(center.x - w * 0.5, center.y - h * 0.5, w, h);
        self.add_agent_portal(rect, "placed");
    }

    /// The size an unarmed click or a wire-drop spawn gives a new agent
    /// portal: its program grid once the installed programs are known, so
    /// the grid fit never moves it off its click or drop point. Before
    /// discovery the Agent tool's recipe (`core.slatekit`) owns it, and the
    /// fit that follows grows the portal from its top-left corner; the
    /// constant stands in only when no kit supplies one. An armed kit places
    /// at its own recipe's size, and then the grid fit applies as it does to
    /// any unbound agent portal.
    pub(crate) fn agent_portal_size(&self) -> (f32, f32) {
        self.agent_program_grid_size().unwrap_or_else(|| {
            self.kits
                .recipe_for(BoardTool::AgentPortal)
                .and_then(|r| r.default_size())
                .map_or(AGENT_PORTAL_SIZE, |[w, h]| (w, h))
        })
    }

    /// Click-to-place default File Atlas lens (960×540, unbound).
    pub(crate) fn place_atlas_portal_at(&mut self, center: Pos2) {
        self.place_from_recipe(
            BoardTool::AtlasPortal,
            center,
            (PORTAL_DEFAULT_W, PORTAL_DEFAULT_H),
        );
    }

    /// Click-to-place an unbound Slate board portal (960×540).
    pub(crate) fn place_slate_portal_at(&mut self, center: Pos2) {
        self.place_from_recipe(
            BoardTool::SlatePortal,
            center,
            (PORTAL_DEFAULT_W, PORTAL_DEFAULT_H),
        );
    }

    /// Click-to-place default web portal, bound to the start locator
    /// (P2.PortalPlace.click).
    pub(crate) fn place_web_portal_at(&mut self, center: Pos2) {
        if self.armed_kit_id.is_some() {
            self.place_from_recipe(
                BoardTool::WebPortal,
                center,
                (PORTAL_DEFAULT_W, PORTAL_DEFAULT_H),
            );
            return;
        }
        let rect = WorldRect::new(
            center.x - PORTAL_DEFAULT_W * 0.5,
            center.y - PORTAL_DEFAULT_H * 0.5,
            PORTAL_DEFAULT_W,
            PORTAL_DEFAULT_H,
        );
        self.add_web_portal(
            rect,
            Some(board_web::WEB_START_LOCATOR.to_string()),
            "placed",
        );
    }

    /// Place a tool's recipe centred on a point, at the recipe's own default
    /// size when it names one and `fallback` otherwise.
    pub(super) fn place_from_recipe(
        &mut self,
        tool: BoardTool,
        center: Pos2,
        fallback: (f32, f32),
    ) {
        let Some(recipe) = self.active_recipe(tool) else {
            self.disarm_create();
            return;
        };
        let [w, h] = recipe.default_size().unwrap_or([fallback.0, fallback.1]);
        let rect = WorldRect::new(center.x - w * 0.5, center.y - h * 0.5, w, h);
        let nodes = self.instantiate_recipe_nodes(&recipe, rect);
        if nodes.is_empty() {
            self.disarm_create();
            return;
        }
        if let Some(n) = nodes.first() {
            self.note_last_style(n);
        }
        let ids = self.commit_created_nodes(nodes);
        self.select_created_nodes(ids);
        self.disarm_create();
        if let Some(id) = Self::draw_command_id(tool) {
            self.push_history(atlas_commands::CommandId(id), Some("placed".into()));
        }
    }

    pub(super) fn instantiate_recipe_nodes(
        &mut self,
        recipe: &slate_kit::Recipe,
        rect: WorldRect,
    ) -> Vec<Node> {
        let ctx = kits::build_ctx(
            to_rgba(self.palette().accent),
            self.doc().scene.next_frame_order(),
        );
        recipe
            .instantiate(rect, &ctx)
            .into_iter()
            .map(|s| {
                let mut node = self.doc_mut().scene.build_node(s.rect, s.kind);
                if recipe.inherits_create_style() {
                    let closed = recipe.inherits_closed_shape_style();
                    self.apply_inherited_style(&mut node, closed);
                }
                node
            })
            .collect()
    }
}
