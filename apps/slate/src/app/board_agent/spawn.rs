//! Spawn command and spawn-input hit targets.

use super::*;

impl SlateApp {
    pub(crate) fn agent_spawn_command(&mut self, detail: Option<&str>) -> bool {
        let Some(id) = self.selected_agent_portal() else {
            return false;
        };
        let Some(original) = self.doc().scene.node(id).cloned() else {
            return false;
        };
        let Some(binding) = slate_doc::agent_chat::agent(&original).cloned() else {
            return false;
        };
        if (self.agent_linear(id) && !self.agent_linear_terminal(id))
            || self.agent_is_running(id)
            || binding.chat.draft
        {
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
            .unwrap_or([
                original.rect.x + original.rect.w + slate_doc::agent_chat::CARD_GAP,
                original.rect.y,
            ]);
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
                    linear: self.agent_linear(id),
                    train: binding.chat.train,
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
        let fork = !self.agent_linear(id) && binding.chat.forks_on_output();
        let mut nodes = vec![next];
        if fork {
            let mut sibling = self.doc_mut().scene.build_duplicate(
                &nodes[0],
                0.0,
                slate_doc::agent_chat::DRAFT_HEIGHT + slate_doc::agent_chat::LANE_GAP,
            );
            if let NodeKind::Portal(p) = &mut sibling.kind {
                p.agent.as_mut().unwrap().chat.parent = Some(id);
            }
            nodes.push(sibling);
        }
        if self.agent_linear(id) {
            let mut after = original.clone();
            if let NodeKind::Portal(p) = &mut after.kind {
                p.agent.as_mut().unwrap().chat.end = Some(end);
            }
            let base = self.doc().scene.nodes.len();
            let mut commands = vec![slate_doc::SceneCmd::Patch {
                before: Box::new(original),
                after: Box::new(after),
            }];
            commands.extend(nodes.into_iter().enumerate().map(|(i, node)| {
                slate_doc::SceneCmd::Add {
                    index: base + i,
                    node,
                }
            }));
            if !self.commit_scene(commands) {
                return false;
            }
        } else {
            self.add_nodes(nodes);
        }
        self.board_sel.clear();
        self.board_sel.insert(next_id);
        self.agent_focus(next_id);
        self.agents.composer_focus = Some(next_id);
        true
    }
    /// Where a card spawned from `origin` lands: beside it for a click, at
    /// `drop` for a drag, then snapped to the board and the agent datum.
    pub(crate) fn agent_spawn_rect(
        &mut self,
        origin: NodeId,
        drop: Option<Pos2>,
        size: egui::Vec2,
        zoom: f32,
    ) -> Option<slate_doc::WorldRect> {
        let from = self.doc().scene.node(origin)?.rect;
        let world = drop.unwrap_or(Pos2::new(
            from.x + from.w + slate_doc::agent_chat::CARD_GAP,
            from.y,
        ));
        let resolved = self.resolve_point_snap(world, &[origin], None, false, false);
        let rect = slate_doc::WorldRect::new(resolved.x, resolved.y, size.x, size.y);
        Some(if self.alt_down {
            rect
        } else {
            super::super::board_snap::agent_datum(rect, &[origin], &self.doc().scene, zoom)
        })
    }
    pub(crate) fn agent_output_at(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| {
                !n.hidden
                    && !n.locked
                    && slate_doc::agent_chat::agent(n).is_some_and(|a| {
                        !a.chat.draft && !a.provider.is_empty() && a.chat.parent.is_some()
                    })
                    && !self.agent_in_choose_phase(n.id)
                    && !self.agent_is_running(n.id)
                    && screen.distance(
                        xf.rect_w2s(n.rect).right_top()
                            + egui::vec2(
                                -slate_doc::agent_chat::PORT_INSET,
                                slate_doc::agent_chat::RAIL_INSET,
                            ) * xf.z,
                    ) <= 7.0 * xf.z
            })
            .map(|n| n.id)
    }
    /// A chat card streaming a reply shows Stop on its top output circle.
    pub(super) fn agent_stop_shown(&self, n: &Node) -> bool {
        !n.hidden
            && slate_doc::agent_chat::agent(n).is_some_and(|a| {
                a.view == atlas_ai::agent::PortalView::Chat
                    && !a.chat.draft
                    && !a.provider.is_empty()
            })
            && self.agent_is_running(n.id)
    }
    /// The streaming chat card whose Stop is under `screen`.
    pub(crate) fn agent_stop_at(&self, screen: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.doc()
            .scene
            .nodes
            .iter()
            .rev()
            .find(|n| {
                self.agent_stop_shown(n)
                    && screen.distance(output_circle_center(xf.rect_w2s(n.rect), xf.z))
                        <= canvas_scale::px(STOP_REACH, xf.z)
            })
            .map(|n| n.id)
    }
    /// Runs before canvas gestures; output handles own their press until release.
    pub(crate) fn agent_spawn_input(&mut self, ui: &egui::Ui, xf: &BoardXf) -> bool {
        if self.flow_input(ui, xf) {
            // Hovering the spawn menu only shields this frame. Holding the
            // press guard until a release would eat the first click after
            // Esc closes the menu. Only a primary release clears the guard,
            // so only a primary press arms it.
            if ui.input(|i| i.pointer.primary_pressed()) {
                self.board_align_eat_press = true;
            }
            return true;
        }
        if self.agent_output_drag_input(ui, xf) {
            self.board_align_eat_press = true;
            return true;
        }
        if let Some(pointer) = ui.ctx().pointer_latest_pos() {
            // Blisters, the Send pill, the capsule and its editor take their
            // own presses, not the moves and release of a gesture that began
            // elsewhere. Only a primary press arms the guard: a hover or a
            // pan press that held it until a primary release would eat the
            // next board click.
            let gesture = self.board_drag.is_some()
                || self.brush_straight.is_some()
                || self.pen_straight.is_some()
                || self.agents.spawn_drag.is_some()
                || self.agents.artifact_drag.is_some()
                || self.agents.stop_press.is_some();
            if !gesture && self.crosstalk_captures(pointer) {
                if ui.input(|i| i.pointer.primary_pressed()) {
                    self.board_align_eat_press = true;
                }
                return true;
            }
            if !gesture && self.board_tool == super::super::board::BoardTool::Select {
                let picker = self
                    .doc()
                    .scene
                    .nodes
                    .iter()
                    .rev()
                    .find(|n| {
                        !n.hidden
                            && !n.locked
                            && slate_doc::agent_chat::agent(n).is_some_and(|_a| {
                                self.agents.project_picker == Some(n.id)
                                    || self
                                        .agents
                                        .chat_picker
                                        .as_ref()
                                        .is_some_and(|p| p.portal == n.id)
                            })
                            && xf.rect_w2s(n.rect).shrink(8.0 * xf.z).contains(pointer)
                    })
                    .map(|n| n.id);
                if let Some(id) = picker {
                    if ui.input(|i| i.pointer.primary_pressed()) {
                        self.board_sel = std::iter::once(id).collect();
                        self.agent_focus(id);
                        self.board_align_eat_press = true;
                    }
                    return true;
                }
            }
            if !gesture
                && [self.agents.artifact_popup_rect, self.agents.model_menu_rect]
                    .into_iter()
                    .flatten()
                    .any(|r| r.contains(pointer))
            {
                if ui.input(|i| i.pointer.primary_pressed()) {
                    self.board_align_eat_press = true;
                }
                return true;
            }
            let on_dot = self.context_auto_under(pointer, xf);
            let on_pocket = self.agent_manual_context_at(pointer, xf).filter(|id| {
                self.agent_has_pocket(*id)
                    && self.doc().scene.node(*id).is_some_and(|n| {
                        xf.rect_w2s(n.rect)
                            .expand(canvas_scale::px(12.0, xf.z))
                            .contains(pointer)
                    })
            });
            if ui.input(|i| i.pointer.primary_pressed()) {
                self.agents.context_press = on_dot.map(|id| (id, pointer));
                self.agents.pocket_press = on_pocket.map(|id| (id, pointer));
                if on_dot.is_none() && on_pocket.is_none() {
                    if let Some(id) = self.composer_under(pointer, xf) {
                        self.board_sel = std::iter::once(id).collect();
                        self.agents.composer_focus = Some(id);
                        self.board_align_eat_press = true;
                        return true;
                    }
                }
            }
            if ui.input(|i| i.pointer.primary_released()) {
                if let Some((id, origin)) = self.agents.context_press.take() {
                    if pointer.distance(origin) < 4.0 && on_dot == Some(id) {
                        self.pin_agent_context(id);
                        self.agents.context_preview = None;
                        self.agents.context_preview_rect = None;
                    }
                }
                if let Some((id, origin)) = self.agents.pocket_press.take() {
                    if pointer.distance(origin) < 4.0 && on_pocket == Some(id) {
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId("portal.agent.pocket"),
                            Some(id.0.to_string()),
                        );
                    }
                }
            }
        }
        if self.tab().read_only {
            return false;
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.agents.spawn_drag = None;
            self.agents.artifact_drag = None;
            self.agents.preview_ask = None;
            self.agents.preview_ask_ready = false;
            return false;
        }
        let pointer = ui.ctx().pointer_latest_pos();
        if let Some((id, press)) = self.agents.artifact_drag {
            if let Some(p) = pointer {
                if ui.input(|i| i.pointer.any_released()) {
                    self.agents.artifact_drag = None;
                    let existing = self.provenance_portals(id, true);
                    if (p - press).length() > 4.0 {
                        let shift = ui.input(|i| i.modifiers.shift);
                        let at = self.resolve_point_snap(xf.s2w(p), &[], None, shift, false);
                        if let Some(node) = existing.last().copied() {
                            self.move_node_to(node, at);
                        } else {
                            self.dispatch(
                                ui.ctx(),
                                atlas_commands::CommandId("portal.agent.spawn_outputs"),
                                Some(
                                    serde_json::json!({ "portal": id.0, "at": [at.x, at.y] })
                                        .to_string(),
                                ),
                            );
                        }
                    } else if self.agent_has_outputs(id) {
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId("portal.agent.artifacts"),
                            Some(serde_json::to_string(&(id.0, true)).unwrap()),
                        );
                    }
                }
            }
            self.board_align_eat_press = true;
            return true;
        }
        let pointer = ui.ctx().pointer_latest_pos();
        if let Some((id, press)) = self.agents.spawn_drag {
            if let Some(p) = pointer {
                let moving = (p - press).length() > 4.0;
                let size = egui::vec2(
                    self.agent_draft_width(id),
                    slate_doc::agent_chat::DRAFT_HEIGHT,
                );
                let drop =
                    moving.then(|| xf.s2w(p) - egui::vec2(0.0, slate_doc::agent_chat::RAIL_INSET));
                if let Some(rect) = self.agent_spawn_rect(id, drop, size, xf.z) {
                    self.board_point_snap = Some(Pos2::new(rect.x, rect.y));
                    if ui.input(|i| i.pointer.any_released()) {
                        self.agents.spawn_drag = None;
                        self.board_sel.clear();
                        self.board_sel.insert(id);
                        let fork = moving
                            && self
                                .doc()
                                .scene
                                .node(id)
                                .and_then(slate_doc::agent_chat::agent)
                                .is_some_and(|a| {
                                    !a.chat.linear
                                        && !atlas_ai::runtime::linear_provider(&a.provider)
                                });
                        self.dispatch(
                            ui.ctx(),
                            atlas_commands::CommandId(if fork {
                                "portal.agent.fork"
                            } else {
                                "portal.agent.continue"
                            }),
                            Some(serde_json::to_string(&[rect.x, rect.y]).unwrap()),
                        );
                    }
                }
            }
            self.board_align_eat_press = true;
            return true;
        }
        if let Some(id) = self.agents.stop_press {
            if ui.input(|i| i.pointer.any_released()) {
                self.agents.stop_press = None;
                if pointer.is_some_and(|p| self.agent_stop_at(p, xf) == Some(id)) {
                    self.board_sel = std::iter::once(id).collect();
                    self.dispatch(
                        ui.ctx(),
                        atlas_commands::CommandId("portal.agent.stop"),
                        None,
                    );
                }
            }
            self.board_align_eat_press = true;
            return true;
        }
        if self.board_drag.is_some() || self.board_tool != super::super::board::BoardTool::Select {
            return false;
        }
        if let Some(id) = pointer.and_then(|p| self.agent_stop_at(p, xf)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if ui.input(|i| i.pointer.primary_pressed()) {
                self.agents.stop_press = Some(id);
                self.board_align_eat_press = true;
                return true;
            }
        }
        let ids: Vec<_> = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter(|n| !n.hidden && !n.locked)
            .filter(|n| {
                slate_doc::agent_chat::agent(n).is_some_and(|a| {
                    !a.chat.draft && !a.provider.is_empty() && a.chat.parent.is_some()
                })
            })
            .map(|n| n.id)
            .collect();
        for id in ids {
            if self.agent_in_choose_phase(id) {
                continue;
            }
            let n = self.doc().scene.node(id).unwrap();
            let r = xf.rect_w2s(n.rect);
            let handle = Pos2::new(
                r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                r.top() + slate_doc::agent_chat::RAIL_INSET * xf.z,
            );
            if self.agent_has_outputs(id) {
                let docs = Pos2::new(
                    r.right() - slate_doc::agent_chat::PORT_INSET * xf.z,
                    r.center().y,
                );
                if pointer.is_some_and(|p| p.distance(docs) <= 10.0 * xf.z)
                    && ui.input(|i| i.pointer.primary_pressed())
                {
                    self.agents.artifact_drag = Some((id, pointer.unwrap()));
                    self.board_align_eat_press = true;
                    return true;
                }
            }
            if self.agent_is_running(id) {
                continue;
            }
            let hit = Rect::from_center_size(handle, egui::vec2(8.0, 8.0) * xf.z);
            if pointer.is_some_and(|p| p.distance(handle) <= 7.0 * xf.z) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
            }
            if pointer.is_some_and(|p| hit.contains(p)) && ui.input(|i| i.pointer.primary_pressed())
            {
                self.agents.spawn_drag = Some((id, pointer.unwrap()));
                self.board_align_eat_press = true;
                return true;
            }
        }
        false
    }
}
