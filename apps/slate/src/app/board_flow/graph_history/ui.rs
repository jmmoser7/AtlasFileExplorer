//! History handles use the shared wire gesture and the one Studio popup renderer.
use super::*;
use crate::app::board_flow::graph_ui::GraphMenu;

impl SlateApp {
    pub(crate) fn image_history_at(
        &self,
        point: Pos2,
        xf: &BoardXf,
    ) -> Option<(NodeId, bool, Rect)> {
        let world = xf.s2w(point);
        let id = self.board_pick_node_with_locked(world.x, world.y, true)?;
        let node = self.doc().scene.node(id)?;
        for incoming in [true, false] {
            if let Some(rect) = self
                .image_history_handle(id, incoming, xf.rect_w2s(node.rect), xf.z)
                .filter(|r| r.contains(point))
            {
                return Some((id, incoming, rect));
            }
        }
        None
    }
    pub(crate) fn history_handle_input(&mut self, ui: &egui::Ui) -> bool {
        if self.agents.flow.graph_menu.is_some() {
            return false;
        }
        let pointer = ui.ctx().pointer_latest_pos();
        if ui.input(|i| i.pointer.primary_pressed())
            && pointer.is_some_and(|p| {
                ui.ctx()
                    .layer_id_at(p)
                    .is_some_and(|l| l.order >= atlas_shell::selection_tools::POPUP_ORDER)
            })
        {
            return false;
        }
        if ui.input(|i| i.pointer.primary_pressed()) {
            if let Some((id, incoming, _)) =
                pointer.and_then(|p| self.image_history_at(p, &self.board_xf()))
            {
                self.agents.flow.history_press =
                    Some((self.tab().id, id, incoming, pointer.unwrap()));
                return true;
            }
        }
        let Some((tab, id, incoming, origin)) = self.agents.flow.history_press else {
            return false;
        };
        if tab != self.tab().id || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.agents.flow.history_press = None;
            return true;
        }
        if let Some(point) = pointer {
            if point.distance(origin) > CLICK_SLOP
                && !incoming
                && ui.input(|i| i.pointer.primary_down())
            {
                self.agents.flow.history_press = None;
                if self.tab().read_only || self.doc().scene.node(id).is_none_or(|n| n.locked) {
                    return true;
                }
                self.board_drag = Some(BoardDrag::Wire(crate::app::board_wire::WireDrag {
                    mode: crate::app::board_wire::WireMode::Add {
                        from: (id, Side::Right, OUTPUT_T),
                    },
                    cursor: self.board_xf().s2w(point),
                    snap: None,
                }));
                return true;
            }
            if ui.input(|i| i.pointer.primary_released()) {
                self.agents.flow.history_press = None;
                if point.distance(origin) <= CLICK_SLOP {
                    self.agents.flow.graph_menu = Some(GraphMenu {
                        tab,
                        target: id,
                        slot: None,
                        incoming,
                        at: point + egui::vec2(0.0, 7.5),
                    });
                }
                return true;
            }
        }
        true
    }
    pub(crate) fn image_history_popup(&mut self, ui: &egui::Ui, menu: GraphMenu) -> bool {
        if menu.tab != self.tab().id || !self.image_history_host(menu.target) {
            return false;
        }
        use atlas_shell::selection_tools::menu_selector::{
            studio_action_popup, StudioActionRow, StudioActionStyle,
        };
        let mutable = !self.tab().read_only
            && self
                .doc()
                .scene
                .node(menu.target)
                .is_some_and(|n| !n.locked);
        let view_error = self.image_history_ready(menu.target).err();
        let mut actions = vec![(
            "View carried history",
            "view",
            view_error.is_none(),
            view_error,
        )];
        if let Some((_, _, target)) = self.image_history_edge(menu.target, menu.incoming) {
            if agent_inputs::generator_history_is_terminal(&self.doc().scene, target) {
                let editable = mutable && self.doc().scene.node(target).is_some_and(|n| !n.locked);
                actions.push(("Cut model history", "cut", editable, None));
            }
            if menu.incoming {
                actions.push(("Refresh inherited history", "refresh", mutable, None));
            }
        } else if self
            .doc()
            .scene
            .node(menu.target)
            .and_then(slate_doc::agent_chat::agent)
            .is_some_and(|a| a.generator.history_source.is_some())
        {
            actions.push(("Reconnect chat history", "reconnect", mutable, None));
        }
        if !menu.incoming {
            actions.push(("New downstream image", "downstream", mutable, None));
        }
        let rows = actions
            .iter()
            .map(|(label, _, enabled, hint)| StudioActionRow {
                label,
                checked: None,
                interactive: *enabled,
                danger: false,
                divider_before: false,
                icon: None,
                hint: hint.as_deref(),
            })
            .collect::<Vec<_>>();
        let mut open = true;
        let mut popup_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("image-history-menu-owner", menu.tab)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(ui.max_rect()),
        );
        let response = studio_action_popup(
            &mut popup_ui,
            Id::new(("image-history-context", menu.tab, menu.target.0)),
            menu.at,
            &rows,
            &mut open,
            StudioActionStyle::Plain,
            self.palette(),
        );
        let captured = response.picked.is_some()
            || response
                .popup
                .is_some_and(|r| ui.ctx().pointer_latest_pos().is_some_and(|p| r.contains(p)));
        if let Some((_, action, _, _)) = response.picked.and_then(|i| actions.get(i)) {
            self.dispatch(ui.ctx(),atlas_commands::CommandId("board.image.history"),Some(serde_json::json!({"node":menu.target.0,"incoming":menu.incoming,"action":action}).to_string()));
        } else if open {
            self.agents.flow.graph_menu = Some(menu);
        }
        captured
    }
}
