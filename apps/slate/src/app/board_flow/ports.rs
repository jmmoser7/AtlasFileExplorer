//! Flow-port presentation on the shared typed-port geometry.
use super::*;

/// Designed port radius; hit testing and paint both use this board-space value.
pub(crate) const PORT_RADIUS: f32 = 6.0;

#[cfg(test)]
mod tests;

impl SlateApp {
    /// Shared reveal/fit decision for both the real host cutouts and its dots.
    pub(crate) fn flow_ports_visible(
        &self,
        node: &slate_doc::scene::Node,
        ctx: &egui::Context,
        xf: &BoardXf,
    ) -> bool {
        if self.portal_capture_owns_viewport(node.id) {
            return false;
        }
        let ports = self.wire_input_ports(node);
        if node.hidden
            || ports.is_empty()
            || !super::super::board_wire::wire_ports_fit(node.rect, xf.z)
            || !super::super::board_wire::port_disc_visible(PORT_RADIUS, xf.z)
        {
            return false;
        }
        let host = self.wire_input_host(node);
        if !host.offers_ports() {
            return false;
        }
        let screen = xf.rect_w2s(node.rect);
        let near = ctx
            .pointer_latest_pos()
            .is_some_and(|p| screen.expand(PORT_REVEAL * xf.z).contains(p));
        let wiring = matches!(self.board_drag, Some(BoardDrag::Wire(_)));
        let ordinary = agent_inputs::image_generation_target(node, &|id| {
            self.doc().item(id).map(|item| item.path.as_path())
        });
        if ordinary && !(wiring && near) {
            return false;
        }
        (near || wiring || self.board_sel.contains(&node.id))
            && (!self.agents.flow.hud_hidden(node.id) || wiring)
    }

    /// Typed ports of flow nodes the pointer is near, that are selected, or
    /// while a wire is being drawn.
    pub(crate) fn paint_flow_ports(&self, painter: &egui::Painter, xf: &BoardXf) {
        let z = xf.z;
        if !super::super::board_wire::port_disc_visible(PORT_RADIUS, z) {
            return;
        }
        let radius = canvas_scale::px(PORT_RADIUS, z);
        let palette = self.palette();
        let roles = palette.agent_roles();
        let pointer = painter.ctx().pointer_latest_pos();
        let font = canvas_scale::font(PORT_LABEL_PX, z);
        let labels = canvas_text::legible(font.size);
        let scene = &self.doc().scene;
        for node in &scene.nodes {
            let ports = self.wire_input_ports(node);
            if !self.flow_ports_visible(node, painter.ctx(), xf) {
                continue;
            }
            let host = self.wire_input_host(node);
            let sites = host.ports();
            for (port, site) in ports.iter().zip(&sites) {
                let center = xf.w2s(Pos2::new(site.point[0], site.point[1]));
                let (_, color) = InputRole::of_slot(port.slot).look(&roles);
                painter.circle_filled(center, radius, color);
                if labels {
                    canvas_text::text(
                        painter,
                        center
                            - egui::vec2(
                                radius + canvas_scale::px(6.0, z),
                                canvas_scale::px(6.0, z),
                            ),
                        Align2::RIGHT_BOTTOM,
                        port.label,
                        font.clone(),
                        palette.ink,
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
                painter.circle_filled(center, radius, ring);
                if labels {
                    canvas_text::text(
                        painter,
                        center + egui::vec2(radius + 6.0 * z, -6.0 * z),
                        Align2::LEFT_BOTTOM,
                        "Output",
                        font.clone(),
                        palette.ink,
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
