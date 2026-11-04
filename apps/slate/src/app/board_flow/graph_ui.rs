//! Actual typed-port context uses the existing shared popup controller.
use super::*;
#[cfg(test)]
mod tests;

pub(crate) struct GraphMenu {
    pub(super) tab: u64,
    pub(super) target: NodeId,
    pub(super) slot: Option<atlas_agent::InputSlot>,
    pub(super) incoming: bool,
    pub(super) at: Pos2,
}

impl SlateApp {
    pub(crate) fn board_context_click(
        &mut self,
        ctx: &egui::Context,
        point: Pos2,
        world: Pos2,
        xf: &BoardXf,
        body_allowed: bool,
    ) {
        if self.open_graph_context(ctx, point, xf) {
            return;
        }
        if body_allowed {
            if let Some(id) = self.board_pick_node(world.x, world.y) {
                self.board_menu = Some((id, point));
            } else {
                self.board_empty_menu = Some(point);
            }
        }
    }
    fn graph_input_at(
        &self,
        ctx: &egui::Context,
        point: Pos2,
        xf: &BoardXf,
    ) -> Option<(NodeId, atlas_agent::InputSlot)> {
        let world = xf.s2w(point);
        let foreground = self.board_pick_node_with_locked(world.x, world.y, true);
        for node in self.doc().scene.nodes.iter().rev() {
            if foreground.is_some_and(|id| id != node.id) || !self.flow_ports_visible(node, ctx, xf)
            {
                continue;
            }
            let ports = self.wire_input_ports(node);
            let sites = self.wire_input_host(node).ports();
            let radius = Self::flow_port_presentation(node, xf)?.radius;
            for (port, site) in ports.iter().zip(&sites) {
                if !self.source_agent_input_visible(node.id, port.slot) {
                    continue;
                }
                let host = self.wire_input_host(node);
                let outward = host.outward(site.side, site.t);
                let center = atlas_shell::selection_tools::hit_regions::wire_port_center(
                    xf.w2s(Pos2::new(site.point[0], site.point[1])),
                    egui::vec2(outward[0], outward[1]),
                    radius,
                );
                let height = self
                    .graft_input_count(node.id, port.slot)
                    .map_or(radius * 2.0, |n| {
                        slate_doc::wire_host::graft_capsule_height(n) * xf.z
                    });
                if Rect::from_center_size(center, egui::vec2(radius * 2.0, height))
                    .expand(atlas_shell::canvas_scale::px(2.0, xf.z))
                    .contains(point)
                {
                    return Some((node.id, port.slot));
                }
            }
        }
        None
    }

    /// Runs before the body context dispatcher; opening does not configure a program.
    pub(crate) fn open_graph_context(
        &mut self,
        ctx: &egui::Context,
        point: Pos2,
        xf: &BoardXf,
    ) -> bool {
        if let Some((target, incoming, rect)) = self.image_history_at(point, xf) {
            self.agents.flow.graph_menu = Some(GraphMenu {
                tab: self.tab().id,
                target,
                slot: None,
                incoming,
                at: rect.left_bottom() + egui::vec2(0.0, 4.0),
            });
            self.board_menu = None;
            self.board_empty_menu = None;
            return true;
        }
        let mut target = self.graph_input_at(ctx, point, xf);
        if target.is_none() {
            let world = xf.s2w(point);
            if let Some(source) = self
                .board_pick_node_with_locked(world.x, world.y, true)
                .filter(|id| {
                    self.doc()
                        .scene
                        .node(*id)
                        .and_then(slate_doc::agent_chat::agent)
                        .is_some_and(|a| a.generator.input_source_kind.is_some())
                })
            {
                target = self.doc().scene.nodes.iter().find_map(|node| {
                    let NodeKind::Connector(conn) = &node.kind else {
                        return None;
                    };
                    let binding = conn.binding.as_ref()?;
                    if agent_inputs::endpoint_node(binding.source_end(conn)) != Some(source) {
                        return None;
                    }
                    let slot = binding
                        .slot
                        .as_deref()
                        .and_then(atlas_agent::InputSlot::from_id)?;
                    let id = agent_inputs::endpoint_node(binding.target_end(conn))?;
                    self.doc()
                        .scene
                        .node(id)
                        .filter(|node| self.wire_input_ports(node).iter().any(|p| p.slot == slot))
                        .map(|_| (id, slot))
                });
            }
        }
        let Some((target, slot)) = target else {
            return false;
        };
        self.board_menu = None;
        self.board_empty_menu = None;
        self.agents.flow.graph_menu = Some(GraphMenu {
            tab: self.tab().id,
            target,
            slot: Some(slot),
            incoming: false,
            at: point,
        });
        true
    }

    pub(super) fn graph_menu_input(&mut self, ui: &egui::Ui) -> bool {
        let ctx = ui.ctx();
        if self.agents.flow.graph_menu.is_none()
            && self.agents.flow.menu.is_none()
            && self.board_menu.is_none()
            && self.board_empty_menu.is_none()
            && !ctx.memory(|memory| memory.top_modal_layer().is_some())
            && !self.turbo_pan.should_suppress_context_menu()
            && ctx.input(|i| {
                !i.modifiers.any() && i.pointer.button_clicked(egui::PointerButton::Secondary)
            })
        {
            if let Some(point) = ctx.pointer_latest_pos().filter(|point| {
                self.canvas_rect.contains(*point)
                    && ui.clip_rect().contains(*point)
                    && !ctx.layer_id_at(*point).is_some_and(|layer| {
                        layer.order >= atlas_shell::selection_tools::POPUP_ORDER
                    })
                    && self.graph_input_at(ctx, *point, &self.board_xf()).is_some()
            }) {
                self.open_graph_context(ctx, point, &self.board_xf());
            }
        }
        if self.history_handle_input(ui) {
            return true;
        }
        let Some(menu) = self.agents.flow.graph_menu.take() else {
            return false;
        };
        if menu.tab != self.tab().id {
            return false;
        }
        if menu.slot.is_none() {
            return self.image_history_popup(ui, menu);
        }
        let slot = menu.slot.unwrap();
        let Some(node) = self.doc().scene.node(menu.target) else {
            return false;
        };
        let checked = slate_doc::agent_chat::agent(node)
            .is_some_and(|a| a.generator.grafted_slots.contains(slot.id()));
        let enabled = !self.tab().read_only
            && !node.locked
            && self.wire_input_ports(node).iter().any(|p| p.slot == slot);
        use atlas_shell::selection_tools::menu_selector::{StudioActionRow, StudioActionStyle};
        let rows = [StudioActionRow {
            icon: None,
            hint: None,
            label: "Graft inputs",
            checked: Some(checked),
            interactive: enabled,
            danger: false,
            divider_before: false,
        }];
        let style = if self.generator_source_kind(menu.target) == Some(PendingHost::Image) {
            StudioActionStyle::Image
        } else {
            StudioActionStyle::Element
        };
        let mut open = true;
        let mut popup_ui = egui::Ui::new(
            ui.ctx().clone(),
            Id::new(("graph-menu-owner", menu.tab)),
            egui::UiBuilder::new()
                .layer_id(ui.layer_id())
                .max_rect(ui.max_rect()),
        );
        let response = atlas_shell::selection_tools::menu_selector::studio_action_popup(
            &mut popup_ui,
            Id::new(("graph-input-context", menu.tab, menu.target.0)),
            menu.at,
            &rows,
            &mut open,
            style,
            self.palette(),
        );
        let captured = response.picked.is_some()
            || response
                .popup
                .is_some_and(|r| ui.ctx().pointer_latest_pos().is_some_and(|p| r.contains(p)));
        if response.picked.is_some() {
            self.dispatch(
                ui.ctx(),
                atlas_commands::CommandId("board.input.graft"),
                Some(
                    serde_json::json!({"node":menu.target.0,"slot":slot.id(),"grafted":!checked})
                        .to_string(),
                ),
            );
        } else if open {
            self.agents.flow.graph_menu = Some(menu);
        }
        captured
    }
}
