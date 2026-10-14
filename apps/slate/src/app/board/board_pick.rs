//! Board click and double-click.

use super::*;

impl SlateApp {
    /// Egui chooses a click target from the previous frame's widgets. The
    /// sub-object chord can arrive with the press, after a number chip was
    /// hovered, so this explicit canvas gesture owns ordered press/release.
    pub(super) fn curve_subselect_input(
        &mut self,
        ui: &egui::Ui,
        xf: &BoardXf,
        pointer: Option<Pos2>,
        start_allowed: bool,
    ) {
        if let Some(p) = pointer.filter(|p| {
            start_allowed
                && self.canvas_rect.contains(*p)
                && ui.clip_rect().contains(*p)
                && ui.ctx().layer_id_at(*p).is_some_and(|layer| {
                    layer == ui.layer_id()
                        || (layer == atlas_shell::selection_tools::selection_chrome_layer()
                            && !self.property_editor_open()
                            && self.curve_subselect_under(*p))
                })
                && !ui.ctx().memory(|m| m.any_popup_open())
        }) {
            if self.board_drag.is_none()
                && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                && self.curve_subselect_under(p)
                && self.hovered_vertex(p).is_none()
            {
                self.board_drag = Some(BoardDrag::Direct(
                    super::super::board_direct::DirectDrag::Marquee {
                        start_screen: p,
                        add: true,
                        edges: true,
                    },
                ));
            }
        }
        if matches!(
            self.board_drag,
            Some(BoardDrag::Direct(
                super::super::board_direct::DirectDrag::Marquee { edges: true, .. }
            ))
        ) {
            self.board_align_eat_press = true;
            if ui.input(|i| i.pointer.button_released(egui::PointerButton::Primary)) {
                if let Some(p) = pointer {
                    self.end_gesture(xf.s2w(p), Some(p), ui.input(|i| i.modifiers));
                } else {
                    self.board_drag = None;
                }
            }
        }
    }

    pub(super) fn board_click(&mut self, world: Pos2, mods: egui::Modifiers) {
        if self.board_tool == BoardTool::Deck {
            return;
        }
        // A dropped toolbar click is handled by try_dock_embed_click so
        // an armed Text / Sticky / click-place tool cannot commit on the
        // same click that picked an icon.
        if self.dock_embed_node_at(world.x, world.y).is_some() {
            return;
        }
        // Crop mode: a click on any selected croppable image or on a crop
        // handle's slop changes nothing. A click anywhere else finishes the
        // crop and passes through to normal selection.
        if self.board_crop.is_some() {
            if self.crop_owns_pointer(self.board_xf().w2s(world)) {
                return;
            }
            self.board_crop = None;
        }
        match self.board_tool {
            BoardTool::Text => {
                if matches!(self.board_drag, Some(BoardDrag::Draw { .. })) {
                    return;
                }
                let world = self.resolve_point_snap(world, &[], None, false, false);
                self.place_text_at(world);
                return;
            }
            BoardTool::Sticky => {
                let world = self.resolve_point_snap(world, &[], None, false, false);
                self.place_sticky_at(world);
                return;
            }
            tool if tool.places_by_drag_rect() => {
                // Safety net: a click that never started Draw (press
                // missed the canvas response) still ClickPlaces. If Draw
                // is live, release owns the commit so we do not place twice.
                if matches!(self.board_drag, Some(BoardDrag::Draw { .. })) {
                    return;
                }
                let world = self.resolve_point_snap(world, &[], None, false, false);
                self.place_default_at(tool, world);
                return;
            }
            BoardTool::Polyline | BoardTool::Arc => {
                self.path_tool_click(world);
                return;
            }
            BoardTool::Line => {
                // The whole grammar lives in the gesture path (press /
                // release); the click event must not fall through to
                // selection.
                return;
            }
            BoardTool::Brush => {
                // Spring-loaded eyedropper (samples into fg). Shift clicks
                // belong to the ordered press / release path.
                if mods.alt {
                    self.eyedropper_click(world, false);
                }
                return;
            }
            BoardTool::Eyedropper => {
                self.eyedropper_click(world, mods.alt);
                return;
            }
            BoardTool::DirectSelect => {
                let screen = self.board_xf().w2s(world);
                self.direct_click(screen, world, mods.shift);
                return;
            }
            BoardTool::Trim | BoardTool::Split => {
                self.trim_click_modifiers(world, mods.shift, mods.ctrl);
                return;
            }
            BoardTool::Select => {
                let screen = self.board_xf().w2s(world);
                // Ctrl+Shift+click on a curve segment picks that edge; on
                // a grip it adds or removes the point like Shift.
                if mods.ctrl
                    && mods.shift
                    && self.hovered_vertex(screen).is_none()
                    && self.pick_curve_edge(world)
                {
                    return;
                }
                if self.pick_curve_grip_point(screen, mods.shift) {
                    return;
                }
            }
            _ => {}
        }
        // Ctrl+Shift+click: sub-object select — a single group member, or a
        // locked node (force-selected for one-off edits). No expansion.
        if mods.ctrl && mods.shift {
            let hit = board_path::board_pick_node_routed(
                &self.doc().scene,
                world.x,
                world.y,
                self.tab().cam.z,
                true,
                self.board_wire_routing,
            );
            match hit {
                Some(id) => {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                None => self.board_sel.clear(),
            }
            return;
        }
        match self.board_pick_node(world.x, world.y) {
            Some(id) => {
                self.apply_select_pick(id, mods, false);
                if self.board_tool == BoardTool::Select && !mods.shift && !mods.ctrl && !mods.alt {
                    self.video_click(id);
                }
            }
            None => {
                if !mods.shift && !mods.ctrl {
                    self.board_sel.clear();
                }
            }
        }
    }

    /// P1.node.select: click replaces; Shift/Ctrl add; Shift/Ctrl+click on an
    /// already-selected node toggles it off. A press that starts a drag never
    /// toggles off so the set stays selected to move.
    pub(super) fn apply_select_pick(
        &mut self,
        id: NodeId,
        mods: egui::Modifiers,
        from_press: bool,
    ) {
        if mods.ctrl && mods.shift {
            if !self.board_sel.contains(&id) {
                self.board_sel.clear();
                self.board_sel.insert(id);
            }
            return;
        }
        let group_ids =
            super::super::board_flags::expand_selection_to_groups(&self.doc().scene, &[id]);
        if mods.shift || mods.ctrl {
            if self.board_sel.contains(&id) {
                if !from_press {
                    for g in group_ids {
                        self.board_sel.remove(&g);
                    }
                }
            } else {
                self.board_sel.extend(group_ids);
            }
        } else if !self.board_sel.contains(&id) {
            self.board_sel = group_ids.into_iter().collect();
        }
    }

    #[cfg(test)]
    pub(crate) fn board_click_for_test(&mut self, world: Pos2, mods: egui::Modifiers) {
        self.board_click(world, mods);
    }

    #[cfg(test)]
    pub(crate) fn board_double_click_for_test(&mut self, world: Pos2) {
        self.board_double_click(world);
    }

    pub(super) fn board_double_click(&mut self, world: Pos2) {
        let screen = self.board_xf().w2s(world);
        if self.sheet_add_hit(screen).is_some() {
            return;
        }
        if let Some(hit) = self
            .sheet_hits
            .iter()
            .find(|hit| !hit.add && hit.rect.contains(screen))
            .cloned()
        {
            self.enter_sheet(hit.node);
            if self.sheet_open == Some(hit.node) {
                self.open_sheet_cell(hit);
            }
            return;
        }
        if let Some(id) = self.board_pick_node(world.x, world.y) {
            if self.sheet_node(id) {
                self.enter_sheet(id);
                return;
            }
        }
        if self.board_tool == BoardTool::BezierSpan && !self.bezier_double_click_finishes() {
            return;
        }
        if self.board_tool.is_path_tool() && self.path_tool_try_finish() {
            return;
        }
        // Direct selection: double-click an anchor toggles corner ↔ smooth.
        if self.board_tool == BoardTool::DirectSelect {
            let screen = self.board_xf().w2s(world);
            if self.direct_double_click(screen) {
                return;
            }
        }
        let Some(id) = self
            .board_pick_node(world.x, world.y)
            .or_else(|| board_path::closed_text_target(&self.doc().scene, world.x, world.y))
        else {
            // Double-click on empty board = the canvas palette (Grasshopper
            // gesture): search + place/execute at this point. Navigation
            // tools only — draw tools keep their double-click semantics.
            if matches!(self.board_tool, BoardTool::Select | BoardTool::Pan)
                && self.board_crop.is_none()
                && !self.text_compose_active()
            {
                let screen = self.board_xf().w2s(world);
                self.open_board_palette(screen, world);
            }
            return;
        };
        let Some(node) = self.doc().scene.node(id).cloned() else {
            return;
        };
        match &node.kind {
            NodeKind::Text(t) => {
                self.shape_properties.panel = None;
                self.text_edit_rect = None;
                if t.fill.is_some() {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                }
                let text = self.agent_note_reply(id).unwrap_or_else(|| t.text.clone());
                self.text_edit = Some((id, text));
            }
            NodeKind::Portal(p) if p.kind == PortalKind::Slate => {
                self.open_slate_portal(id);
            }
            NodeKind::Portal(p) if p.kind == PortalKind::Web => {
                // Ctrl hands the page to the real browser instead (D22).
                if self.ctrl_down {
                    self.board_sel.clear();
                    self.board_sel.insert(id);
                    self.web_open_external();
                    return;
                }
                // Below the live threshold, zoom to fit first rather than
                // refusing — the page is not too small, the view is (D23).
                let screen_h = self.board_xf().rect_w2s(node.rect).height();
                if board_web::lod_for(screen_h) != board_web::WebLod::Eligible {
                    self.zoom_to_rect(node.rect);
                }
                self.web_focus(id);
            }
            NodeKind::Portal(_) => {
                self.portal_enter_interactive(id);
            }
            NodeKind::Connector(_) => {
                // Double-click a wire = edit its label at the midpoint.
                self.board_sel.clear();
                self.board_sel.insert(id);
                self.open_wire_label_edit(id);
            }
            NodeKind::Shape(s) if slate_doc::scene::shape_hosts_text(s) => {
                self.board_sel =
                    super::super::board_flags::expand_selection_to_groups(&self.doc().scene, &[id])
                        .into_iter()
                        .collect();
                let body = s.text.as_ref().map(|t| t.body.clone()).unwrap_or_default();
                self.text_edit = Some((id, body));
                self.shape_properties.panel = None;
            }
            NodeKind::Image(_) => {
                if self.begin_text_document_edit(&node) {
                    return;
                }
                if let Some(path) = self.image_item_path(id) {
                    if self
                        .model_node_info(id)
                        .is_some_and(|info| self.model3d.external.contains(&info.cache_key))
                    {
                        self.open_enscape_node(id);
                        return;
                    }
                    // Locked 3D viewports unlock into live navigation instead
                    // of opening the file (Esc, a click outside, or auto-lock
                    // re-locks them).
                    if slate_doc::media_kind(&path) == slate_doc::MediaKind::Model {
                        if !self.model3d.live.contains_key(&id) {
                            self.unlock_model(id);
                        }
                    } else if self.croppable_image(id) {
                        // InDesign/Figma convention: double-click enters crop
                        // mode. "Open file" stays in the right-click menu.
                        self.enter_crop_mode(id);
                    } else {
                        self.open_item_path(&path);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod input_guards {
    use super::*;
    use crate::app::tests::{edge_polyline, grip_board, mid, SUB_OBJECT};

    #[test]
    fn covering_area_and_open_editor_block_raw_edge_input() {
        // Exercise the ordered input owner with real press events and an actual
        // overlapping Area. No popup memory flag is set by these Areas.
        for editor in [false, true] {
            let mut h = grip_board("edge_layer_guards");
            let (_, pts) = edge_polyline(&mut h);
            let xf = h.app.board_xf();
            let at = xf.w2s(mid(pts[1], pts[2]));
            h.app.ctrl_down = true;
            h.app.shift_down = true;
            let layer = if editor {
                h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Stroke);
                atlas_shell::selection_tools::selection_chrome_layer()
            } else {
                egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new("unrelated_covering_area"),
                )
            };
            let ctx = egui::Context::default();
            for pressed in [None, None, Some(true)] {
                let mut input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                    modifiers: SUB_OBJECT,
                    ..Default::default()
                };
                input.events.push(egui::Event::PointerMoved(at));
                if let Some(pressed) = pressed {
                    input.events.push(egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: SUB_OBJECT,
                    });
                }
                let _ = ctx.run(input, |ctx| {
                    egui::Area::new(layer.id)
                        .order(layer.order)
                        .fixed_pos(at - Vec2::splat(20.0))
                        .fade_in(false)
                        .show(ctx, |ui| {
                            ui.allocate_space(Vec2::splat(40.0));
                        });
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if pressed.is_some() {
                            assert_eq!(ctx.layer_id_at(at), Some(layer));
                            assert!(!ctx.memory(|m| m.any_popup_open()));
                            h.app.curve_subselect_input(ui, &xf, Some(at), true);
                        }
                    });
                });
            }
            assert!(h.app.board_drag.is_none(), "editor={editor}");
            assert!(h.app.direct.grip_points.edges.is_empty());
        }
    }
}
