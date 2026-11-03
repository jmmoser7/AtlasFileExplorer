//! Flow-port presentation on the shared typed-port geometry.
use super::*;
use atlas_shell::selection_tools::hit_regions::PortPresentation;

#[cfg(test)]
mod tests;

impl SlateApp {
    pub(crate) fn set_input_grafted(&mut self, detail: Option<&str>) -> bool {
        #[derive(serde::Deserialize)]
        struct Change {
            node: u64,
            slot: String,
            grafted: bool,
        }
        let Some(change) = detail.and_then(|text| serde_json::from_str::<Change>(text).ok()) else {
            return false;
        };
        let id = slate_doc::NodeId(change.node);
        let Ok(command) = self.prepare_input_graft(id, &change.slot, change.grafted) else {
            return false;
        };
        self.commit_scene(vec![command])
    }

    /// Shared human/targeted translation; preparing never changes the document.
    pub(crate) fn prepare_input_graft(
        &self,
        id: slate_doc::NodeId,
        slot: &str,
        grafted: bool,
    ) -> Result<slate_doc::scene::SceneCmd, String> {
        let node = self.doc().scene.node(id).ok_or("Generator is missing")?;
        if self.tab().read_only
            || node.locked
            || !self
                .wire_input_ports(node)
                .iter()
                .any(|port| port.slot.id() == slot && port.slot != atlas_agent::InputSlot::View)
        {
            return Err(
                "Choose an unlocked generator and one of its graftable typed input slots".into(),
            );
        }
        if slate_doc::agent_chat::agent(node)
            .is_some_and(|a| a.generator.grafted_slots.contains(slot))
            == grafted
        {
            return Err("The input graft setting is unchanged".into());
        }
        let mut after = self.graph_settings_node(id)?;
        let agent = slate_doc::agent_chat::agent_mut(&mut after).unwrap();
        if grafted {
            agent.generator.grafted_slots.insert(slot.to_owned());
        } else {
            agent.generator.grafted_slots.remove(slot);
        }
        if node == &after {
            return Err("The input graft setting is unchanged".into());
        }
        Ok(slate_doc::scene::SceneCmd::Patch {
            before: Box::new(node.clone()),
            after: Box::new(after),
        })
    }

    pub(crate) fn graft_input_count(
        &self,
        id: slate_doc::NodeId,
        slot: atlas_agent::InputSlot,
    ) -> Option<usize> {
        let agent = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)?;
        if !agent.generator.grafted_slots.contains(slot.id()) {
            return None;
        }
        Some(
            self.doc()
                .scene
                .nodes
                .iter()
                .filter(|node| {
                    let slate_doc::scene::NodeKind::Connector(conn) = &node.kind else {
                        return false;
                    };
                    conn.binding.as_ref().is_some_and(|binding| {
                        binding.slot.as_deref() == Some(slot.id())
                            && agent_inputs::endpoint_node(binding.target_end(conn)) == Some(id)
                    })
                })
                .count(),
        )
    }
    pub(crate) fn flow_port_presentation(
        node: &slate_doc::scene::Node,
        xf: &BoardXf,
    ) -> Option<PortPresentation> {
        PortPresentation::typed(node.rect.w.abs().min(node.rect.h.abs()) * xf.z, xf.z)
    }

    /// Shared reveal/fit decision for both the real host cutouts and its dots.
    pub(crate) fn flow_ports_visible(
        &self,
        node: &slate_doc::scene::Node,
        ctx: &egui::Context,
        xf: &BoardXf,
    ) -> bool {
        if self.portal_capture_owns_viewport(node.id) || !self.source_agent_inputs_visible(node.id)
        {
            return false;
        }
        let ports = self.wire_input_ports(node);
        if node.hidden || ports.is_empty() || Self::flow_port_presentation(node, xf).is_none() {
            return false;
        }
        let host = self.wire_input_host(node);
        if !host.offers_ports() {
            return false;
        }
        let screen = xf.rect_w2s(node.rect);
        let near = ctx
            .pointer_latest_pos()
            .is_some_and(|p| screen.expand(PORT_REVEAL).contains(p));
        let wiring = matches!(self.board_drag, Some(BoardDrag::Wire(_)));
        (near || wiring || self.board_sel.contains(&node.id))
            && (!self.agents.flow.hud_hidden(node.id) || wiring)
    }

    /// Typed ports of flow nodes the pointer is near, that are selected, or
    /// while a wire is being drawn.
    pub(crate) fn paint_flow_ports(&self, painter: &egui::Painter, xf: &BoardXf) {
        let z = xf.z;
        let palette = self.palette();
        let roles = palette.agent_roles();
        let pointer = painter.ctx().pointer_latest_pos();
        let scene = &self.doc().scene;
        for node in &scene.nodes {
            if let Some(kind) =
                slate_doc::agent_chat::agent(node).and_then(|a| a.generator.input_source_kind)
            {
                let body = xf.rect_w2s(node.rect);
                if !node.hidden
                    && (self.board_sel.contains(&node.id)
                        || pointer.is_some_and(|p| body.contains(p))
                        || matches!(self.board_drag, Some(BoardDrag::Wire(_))))
                {
                    if let (Some(site), Some(presentation)) = (
                        self.wire_host(node).ports().first().copied(),
                        Self::flow_port_presentation(node, xf),
                    ) {
                        let slot = match kind {
                            slate_doc::scene::GraphInputKind::Image => {
                                atlas_agent::InputSlot::Media
                            }
                            slate_doc::scene::GraphInputKind::Text => {
                                atlas_agent::InputSlot::Prompt
                            }
                            slate_doc::scene::GraphInputKind::Style => {
                                atlas_agent::InputSlot::Style
                            }
                        };
                        painter.circle_filled(
                            xf.w2s(Pos2::new(site.point[0], site.point[1])),
                            presentation.radius,
                            InputRole::of_slot(slot).look(&roles).1,
                        );
                    }
                }
                continue;
            }
            let ports = self.wire_input_ports(node);
            if !self.flow_ports_visible(node, painter.ctx(), xf) {
                continue;
            }
            let Some(presentation) = Self::flow_port_presentation(node, xf) else {
                continue;
            };
            let radius = presentation.radius;
            for incoming in [true, false] {
                if let Some(rect) =
                    self.image_history_handle(node.id, incoming, xf.rect_w2s(node.rect), z)
                {
                    painter.circle_filled(
                        rect.center(),
                        rect.width() * 0.5,
                        Color32::from_rgb(135, 142, 150),
                    );
                }
            }
            let font = canvas_scale::font(PORT_LABEL_PX, z);
            let labels = canvas_text::legible(font.size);
            let label_gap = canvas_scale::px(6.0, z);
            let host = self.wire_input_host(node);
            let sites = host.ports();
            for (port, site) in ports.iter().zip(&sites) {
                if !self.source_agent_input_visible(node.id, port.slot) {
                    continue;
                }
                let outward = host.outward(site.side, site.t);
                let center = atlas_shell::selection_tools::hit_regions::wire_port_center(
                    xf.w2s(Pos2::new(site.point[0], site.point[1])),
                    egui::vec2(outward[0], outward[1]),
                    radius,
                );
                let (_, color) = InputRole::of_slot(port.slot).look(&roles);
                let color = color.gamma_multiply(presentation.prominence);
                if let Some(incoming) = self.graft_input_count(node.id, port.slot) {
                    let height = (slate_doc::wire_host::graft_capsule_height(incoming) * z)
                        .max(radius * 2.0);
                    painter.rect_filled(
                        egui::Rect::from_center_size(center, egui::vec2(radius * 2.0, height)),
                        radius,
                        color,
                    );
                } else {
                    painter.circle_filled(center, radius, color);
                }
                if labels {
                    canvas_text::text(
                        painter,
                        center - egui::vec2(radius + label_gap, label_gap),
                        Align2::RIGHT_BOTTOM,
                        port.label,
                        font.clone(),
                        palette.ink.gamma_multiply(presentation.prominence),
                    );
                }
            }
            if let Some(out) = sites.last() {
                let center = xf.w2s(Pos2::new(out.point[0], out.point[1]));
                let hot = pointer.is_some_and(|p| p.distance(center) <= radius * 3.0);
                let ring = if hot {
                    palette.link_hover
                } else {
                    palette.link
                };
                painter.circle_filled(center, radius, ring.gamma_multiply(presentation.prominence));
                if labels {
                    canvas_text::text(
                        painter,
                        center + egui::vec2(radius + label_gap, -label_gap),
                        Align2::LEFT_BOTTOM,
                        "Output",
                        font.clone(),
                        palette.ink.gamma_multiply(presentation.prominence),
                    );
                }
            }
        }
        // A dropped wire waits at its menu until a choice is made.
        if let Some(menu) = self.agents.flow.menu.as_ref().filter(|m| m.dropped) {
            if let Some(source) = scene.node(menu.source) {
                let (side, t) = menu.grip.unwrap_or((Side::Right, OUTPUT_T));
                let from = ConnectorEnd::Anchored {
                    node: source.id,
                    side,
                    t,
                };
                let to = ConnectorEnd::Free {
                    point: [menu.at.x, menu.at.y],
                };
                if let Some(path) =
                    connector_route_in_scene(scene, None, &from, &to, self.board_wire_routing)
                {
                    super::super::board_wire::paint_route_preview(
                        painter,
                        xf,
                        &path,
                        egui::Stroke::new(
                            canvas_scale::px(1.5, z),
                            palette.sub.gamma_multiply(0.6),
                        ),
                    );
                }
            }
        }
    }
}
