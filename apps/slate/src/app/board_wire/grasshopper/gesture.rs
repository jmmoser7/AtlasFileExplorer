//! Snap and commit typed Grasshopper wires at their actual canvas endpoints.
use super::*;

impl SlateApp {
    /// An image shares its native right output with Grasshopper. Interpret it
    /// as a file only when the other endpoint is an exposed definition input.
    pub(in crate::app::board_wire) fn grasshopper_image_at_output(
        &self,
        (id, side, t): (NodeId, Side, f32),
    ) -> Option<&'static GrasshopperPort> {
        if side == Side::Right && t == slate_doc::agent_inputs::OUTPUT_T {
            self.grasshopper_image_file_output(id)
        } else {
            None
        }
    }

    pub(in crate::app::board_wire) fn grasshopper_wire_snap(
        &self,
        world: Pos2,
        origin: (NodeId, Side, f32),
        reach: f32,
    ) -> Option<(NodeId, Side, f32)> {
        let (source, side, t) = origin;
        let image = self.grasshopper_image_at_output(origin);
        let from = image.or_else(|| self.grasshopper_grip(source, side, t))?;
        let target_side = if side == Side::Right {
            Side::Left
        } else {
            Side::Right
        };
        let xf = self.board_xf();
        let scene = &self.doc().scene;
        for id in self.wire_grip_candidates(world, &xf).into_iter().rev() {
            let Some(node) = scene.node(id) else { continue };
            if node.hidden || id == source || matches!(&node.kind, NodeKind::Connector(_)) {
                continue;
            }
            let inside = !node.is_frame()
                && node
                    .rect
                    .contains_rotated(world.x, world.y, node.rotation_deg);
            if node.locked || self.portal_capture_owns_viewport(id) {
                if inside {
                    return None;
                }
                continue;
            }

            let host = self.wire_input_host(node);
            let close = |t| Pos2::from(host.anchor(target_side, t)).distance(world);
            // These receivers share the body and existing connector of the
            // ordinary canvas card. Neither needs an additional resting grip.
            if target_side == Side::Left
                && self.can_connect_grasshopper_output_to_panel(source, from, id)
            {
                let panel_t = grasshopper_port_t(0);
                if inside || close(panel_t) <= reach {
                    return Some((id, target_side, panel_t));
                }
            } else if target_side == Side::Right
                && self
                    .grasshopper_image_output_for_input(id, source, from)
                    .is_some()
            {
                let image_t = slate_doc::agent_inputs::OUTPUT_T;
                if inside || close(image_t) <= reach {
                    return Some((id, target_side, image_t));
                }
            }
            let snap = ports_on(self.grasshopper_component_ports(id), target_side)
                .enumerate()
                .filter(|(_, port)| match (target_side, image) {
                    (Side::Left, Some(_)) => self
                        .grasshopper_image_output_for_input(source, id, port)
                        .is_some(),
                    (Side::Left, None) => port.kind.accepts(from.kind),
                    _ => from.kind.accepts(port.kind),
                })
                .map(|(row, _)| {
                    let t = grasshopper_port_t(row);
                    (t, close(t))
                })
                .filter(|(_, distance)| *distance <= reach)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((t, _)) = snap {
                return Some((id, target_side, t));
            }
            if inside {
                return None;
            }
        }
        None
    }

    pub(in crate::app::board_wire) fn finish_grasshopper_wire(
        &mut self,
        from: (NodeId, Side, f32),
        to: (NodeId, Side, f32),
    ) {
        let (source, target) = if from.1 == Side::Right {
            (from, to)
        } else {
            (to, from)
        };
        let Some(output) = self
            .grasshopper_image_at_output(source)
            .or_else(|| self.grasshopper_grip(source.0, source.1, source.2))
            .map(|p| p.id.clone())
        else {
            return;
        };
        let Some(input) = self
            .grasshopper_grip(target.0, target.1, target.2)
            .map(|p| p.id.clone())
        else {
            return;
        };
        if let Err(error) =
            self.connect_grasshopper_ports(source.0, &output, target.0, &input, source.2, target.2)
        {
            self.toast(error);
        }
    }
}
