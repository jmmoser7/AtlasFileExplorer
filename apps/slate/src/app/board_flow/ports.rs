//! Flow-port presentation on the shared typed-port geometry.
use super::*;

#[cfg(test)]
mod tests;

impl SlateApp {
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
        let wiring = matches!(self.board_drag, Some(BoardDrag::Wire(_)));
        let font = canvas_scale::font(PORT_LABEL_PX, z);
        let labels = canvas_text::legible(font.size);
        let scene = &self.doc().scene;
        for node in &scene.nodes {
            let ports = self.wire_input_ports(node);
            if node.hidden
                || ports.is_empty()
                || !super::super::board_wire::wire_ports_fit(node.rect, z)
            {
                continue;
            }
            let screen = xf.rect_w2s(node.rect);
            let near = pointer
                .is_some_and(|p| screen.expand(canvas_scale::px(PORT_REVEAL, z)).contains(p));
            let ordinary = agent_inputs::image_generation_target(node, &|id| {
                self.doc().item(id).map(|item| item.path.as_path())
            });
            if ordinary && !(wiring && near) {
                continue;
            }
            if !(near || wiring || self.board_sel.contains(&node.id))
                || (self.agents.flow.hud_hidden(node.id) && !wiring)
            {
                continue;
            }
            let bound = agent_inputs::bound_slots(scene, node.id);
            let host = self.wire_input_host(node);
            let sites = host.ports();
            for (port, site) in ports.iter().zip(&sites) {
                let center = xf.w2s(Pos2::new(site.point[0], site.point[1]));
                let (_, color) = InputRole::of_slot(port.slot).look(&roles);
                if bound.contains(&port.slot) {
                    painter.circle_filled(center, radius, color);
                } else {
                    painter.circle_filled(center, radius, palette.card);
                    painter.circle_stroke(
                        center,
                        radius,
                        egui::Stroke::new(canvas_scale::px(1.5, z), color),
                    );
                }
                if labels {
                    canvas_text::text(
                        painter,
                        center - egui::vec2(radius + canvas_scale::px(6.0, z), 0.0),
                        Align2::RIGHT_CENTER,
                        port.label,
                        font.clone(),
                        color,
                    );
                }
            }
            if let Some(out) = sites.last() {
                let center = xf.w2s(Pos2::new(out.point[0], out.point[1]));
                let hot = pointer.is_some_and(|p| p.distance(center) <= radius * 3.0);
                let ring = if hot { palette.accent } else { palette.sub };
                painter.circle_filled(center, radius * 1.2, palette.card);
                painter.circle_stroke(
                    center,
                    radius * 1.2,
                    egui::Stroke::new(canvas_scale::px(1.5, z), ring),
                );
                let arm = radius * 0.6;
                let stroke = egui::Stroke::new(canvas_scale::px(1.5, z), ring);
                painter.line_segment(
                    [center - egui::vec2(arm, 0.0), center + egui::vec2(arm, 0.0)],
                    stroke,
                );
                painter.line_segment(
                    [center - egui::vec2(0.0, arm), center + egui::vec2(0.0, arm)],
                    stroke,
                );
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
