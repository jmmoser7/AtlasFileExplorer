//! Named Grasshopper wire grips, data styles and reversible wire presentation.
use super::*;
use atlas_shell::{canvas_scale, canvas_text};
use slate_doc::scene::{
    grasshopper_wire, GrasshopperPort, GrasshopperPortAccess, GrasshopperPortDirection,
    GrasshopperPortId, GrasshopperPortType, GrasshopperValue,
};
use slate_doc::wire_host::{grasshopper_port_row, grasshopper_port_t};
mod bundle;

const LITERAL_POPUP: &str = "grasshopper-input-value";
fn side_for(direction: GrasshopperPortDirection) -> Side {
    match direction {
        GrasshopperPortDirection::Input => Side::Left,
        GrasshopperPortDirection::Output => Side::Right,
    }
}

fn ports_on(ports: &[GrasshopperPort], side: Side) -> impl Iterator<Item = &GrasshopperPort> {
    ports.iter().filter(move |p| side_for(p.direction) == side)
}

#[derive(Clone)]
struct LiteralEdit {
    tab: u64,
    node: NodeId,
    port: GrasshopperPortId,
    kind: GrasshopperPortType,
    side: Side,
    at: Pos2,
    text: String,
    edit: bool,
    error: Option<String>,
}

impl SlateApp {
    pub(super) fn grasshopper_rail_sizes(&self, node: NodeId) -> (usize, usize) {
        let ports = self.grasshopper_ports(node);
        let inputs = ports_on(ports, Side::Left).count();
        (inputs, ports.len() - inputs)
    }

    pub(super) fn grasshopper_grip(
        &self,
        node: NodeId,
        side: Side,
        t: f32,
    ) -> Option<&GrasshopperPort> {
        ports_on(self.grasshopper_ports(node), side).nth(grasshopper_port_row(t)?)
    }

    pub(super) fn grasshopper_wire_ports(&self, node: &Node, ports: &mut Vec<slate_doc::WirePort>) {
        let (inputs, outputs) = self.grasshopper_rail_sizes(node.id);
        if inputs + outputs == 0 {
            return;
        }
        let host = self.wire_input_host(node);
        for (side, rows) in [(Side::Left, inputs), (Side::Right, outputs)] {
            ports.extend((0..rows).map(|row| {
                let t = grasshopper_port_t(row);
                slate_doc::WirePort {
                    side,
                    t,
                    point: host.anchor(side, t),
                }
            }));
        }
    }

    pub(super) fn grasshopper_wire_snap(
        &self,
        world: Pos2,
        (source, side, t): (NodeId, Side, f32),
        reach: f32,
    ) -> Option<(NodeId, Side, f32)> {
        let from = self.grasshopper_grip(source, side, t)?;
        let target_side = if side == Side::Right {
            Side::Left
        } else {
            Side::Right
        };
        let xf = self.board_xf();
        let scene = &self.doc().scene;
        for id in self.wire_grip_candidates(world, &xf).into_iter().rev() {
            let Some(node) = scene.node(id) else { continue };
            if node.hidden || node.locked || id == source || self.portal_capture_owns_viewport(id) {
                continue;
            }
            let host = self.wire_input_host(node);
            let snap = ports_on(self.grasshopper_ports(id), target_side)
                .enumerate()
                .filter(|(_, port)| {
                    if target_side == Side::Left {
                        port.kind.accepts(from.kind)
                    } else {
                        from.kind.accepts(port.kind)
                    }
                })
                .map(|(row, _)| {
                    let t = grasshopper_port_t(row);
                    let p = Pos2::from(host.anchor(target_side, t));
                    (t, p.distance(world))
                })
                .filter(|(_, dist)| *dist <= reach)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((t, _)) = snap {
                return Some((id, target_side, t));
            }
            if !node.is_frame()
                && node
                    .rect
                    .contains_rotated(world.x, world.y, node.rotation_deg)
            {
                return None;
            }
        }
        None
    }

    pub(super) fn finish_grasshopper_wire(
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
            .grasshopper_grip(source.0, source.1, source.2)
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

    pub(crate) fn paint_grasshopper_grips(
        &self,
        node: &Node,
        painter: &egui::Painter,
        xf: &BoardXf,
    ) {
        let ports = self.grasshopper_ports(node.id);
        if ports.is_empty() || node.hidden || self.portal_capture_owns_viewport(node.id) {
            return;
        }
        let body = xf.rect_w2s(node.rect);
        let near = painter
            .ctx()
            .pointer_latest_pos()
            .is_some_and(|p| body.expand(24.0).contains(p));
        if !near
            && !self.board_sel.contains(&node.id)
            && !matches!(
                self.board_drag,
                Some(super::super::board::BoardDrag::Wire(_))
            )
        {
            return;
        }
        let Some(presentation) = Self::flow_port_presentation(node, xf) else {
            return;
        };
        let host = self.wire_input_host(node);
        let palette = self.palette();
        let radius = presentation.radius;
        let font = canvas_scale::font(11.0, xf.z);
        let mut rows = [0; 2];
        for port in ports {
            let side = side_for(port.direction);
            let index = usize::from(side == Side::Right);
            let t = grasshopper_port_t(rows[index]);
            rows[index] += 1;
            let at = xf.w2s(Pos2::from(host.anchor(side, t)));
            let hovered = self
                .wire_grips
                .is_some_and(|g| g.node == node.id && g.hovered == Some(side) && g.t == t);
            let ink = palette
                .wire
                .gamma_multiply(presentation.prominence * if hovered { 1.0 } else { 0.86 });
            painter.circle_filled(at, radius, ink);
            if canvas_text::legible(font.size) {
                let label_x = if side == Side::Left { -1.0 } else { 1.0 };
                canvas_text::text(
                    painter,
                    at + Vec2::new(label_x * (radius + 6.0 * xf.z), 0.0),
                    if side == Side::Left {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    },
                    &port.name,
                    font.clone(),
                    palette.ink.gamma_multiply(presentation.prominence),
                );
            }
        }
    }

    pub(crate) fn open_grasshopper_grip_context(
        &self,
        ctx: &egui::Context,
        at: Pos2,
        xf: &BoardXf,
    ) -> bool {
        let Some((node, side, t)) = self.wire_grip_at(at, xf) else {
            return false;
        };
        let Some(port) = self.grasshopper_grip(node, side, t) else {
            return false;
        };
        let literal = side == Side::Left
            && matches!(
                port.kind,
                GrasshopperPortType::Integer
                    | GrasshopperPortType::Number
                    | GrasshopperPortType::Text
                    | GrasshopperPortType::Data
            );
        if !literal
            && [true, false].into_iter().all(|bundled| {
                self.grasshopper_grip_pairs(node, &port.id, side, bundled)
                    .is_empty()
            })
        {
            return false;
        }
        let text = match self.grasshopper_literal(node, &port.id) {
            Some(GrasshopperValue::Text(s)) => s.clone(),
            Some(GrasshopperValue::Number(n)) => n.to_string(),
            _ => String::new(),
        };
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new(LITERAL_POPUP),
                LiteralEdit {
                    tab: self.tab().id,
                    node,
                    port: port.id.clone(),
                    kind: port.kind,
                    side,
                    at,
                    text,
                    edit: false,
                    error: None,
                },
            )
        });
        true
    }

    pub(crate) fn grasshopper_grip_context_input(&mut self, ui: &egui::Ui) -> bool {
        let ctx = ui.ctx();
        let id = egui::Id::new(LITERAL_POPUP);
        let Some(mut menu) = ctx.data(|data| data.get_temp::<LiteralEdit>(id)) else {
            return false;
        };
        if menu.tab != self.tab().id
            || !self.grasshopper_viewer(menu.node)
            || ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            ctx.data_mut(|data| data.remove::<LiteralEdit>(id));
            return false;
        }
        let dark = self.dark_mode;
        let enabled = !self.tab().read_only
            && self
                .doc()
                .scene
                .node(menu.node)
                .is_some_and(|node| !node.locked);
        let has_value = self.grasshopper_literal(menu.node, &menu.port).is_some();
        let literal = menu.side == Side::Left
            && matches!(
                menu.kind,
                GrasshopperPortType::Integer
                    | GrasshopperPortType::Number
                    | GrasshopperPortType::Text
                    | GrasshopperPortType::Data
            );
        let mut close = false;
        let area = egui::Area::new(id).fixed_pos(menu.at).order(egui::Order::Foreground).show(ctx, |ui| {
            atlas_shell::menu::frame(dark).show(ui, |ui| {
                atlas_shell::menu::prepare(ui, dark);
                ui.set_min_width(168.0);
                if menu.edit {
                    let field = ui.add(egui::TextEdit::singleline(&mut menu.text).desired_width(168.0));
                    if field.gained_focus() || !field.has_focus() && !field.lost_focus() { field.request_focus(); }
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let save = atlas_shell::menu::row(ui, atlas_shell::menu::Row::new(atlas_shell::menu::MenuIcon::Check, "Set value"), dark).clicked();
                    if (enter || save) && enabled {
                        match literal_value(menu.kind, &menu.text).and_then(|value| self.set_grasshopper_literal(menu.node, &menu.port, Some(value))) {
                            Ok(()) => close = true,
                            Err(error) => menu.error = Some(error),
                        }
                    }
                    if let Some(error) = &menu.error { ui.small(error); }
                } else if enabled {
                    use atlas_shell::menu::{self, MenuIcon};
                    if literal {
                        if menu::item(ui, MenuIcon::Rename, if has_value { "Edit value…" } else { "Set value…" }, dark).clicked() { menu.edit = true; }
                        if has_value && menu::item(ui, MenuIcon::TrashX, "Clear value", dark).clicked() {
                            close = self.set_grasshopper_literal(menu.node, &menu.port, None).is_ok();
                        }
                    }
                    for (bundled, icon, label) in [(true, MenuIcon::Group, "Bundle Grasshopper wires"), (false, MenuIcon::Ungroup, "Unbundle Grasshopper wires")] {
                        if !self.grasshopper_grip_pairs(menu.node, &menu.port, menu.side, bundled).is_empty()
                            && menu::item(ui, icon, label, dark).clicked()
                        {
                            close = self.dispatch(ctx, atlas_commands::CommandId("board.wire.grasshopper_bundle"), Some(serde_json::json!({"node":menu.node,"port":menu.port,"side":menu.side,"bundled":bundled}).to_string()));
                        }
                    }
                }
            });
        }).response.rect;
        let pointer = ctx.pointer_latest_pos();
        let captured = pointer.is_some_and(|p| area.contains(p));
        let dismissed = !captured && ctx.input(|i| i.pointer.any_pressed());
        ctx.data_mut(|data| {
            if close || dismissed || !enabled {
                data.remove::<LiteralEdit>(id);
            } else {
                data.insert_temp(id, menu);
            }
        });
        captured
    }

    pub(super) fn grasshopper_access(&self, conn: &ConnectorNode) -> Option<GrasshopperPortAccess> {
        let wire = grasshopper_wire(conn)?;
        self.grasshopper_output_access(wire.source, &wire.source_port)
            .or_else(|| {
                self.grasshopper_ports(wire.source)
                    .iter()
                    .find(|p| p.id == wire.source_port)
                    .map(|p| p.access)
            })
            .or(Some(GrasshopperPortAccess::Item))
    }

    pub(super) fn current_grasshopper_ends(&self, conn: &ConnectorNode) -> [ConnectorEnd; 2] {
        let mut ends = [conn.a, conn.b];
        let Some(wire) = grasshopper_wire(conn) else {
            return ends;
        };
        for (end, port) in ends.iter_mut().zip([&wire.source_port, &wire.target_port]) {
            if let ConnectorEnd::Anchored { node, side, t } = end {
                if let Some(row) =
                    ports_on(self.grasshopper_ports(*node), *side).position(|p| &p.id == port)
                {
                    *t = grasshopper_port_t(row);
                }
            }
        }
        ends
    }
}

pub(super) fn paint_grasshopper_fancy_wire(
    painter: &egui::Painter,
    xf: &BoardXf,
    path: &ConnectorPath,
    access: GrasshopperPortAccess,
    color: Color32,
    width: f32,
) {
    let center: Vec<_> = match path {
        ConnectorPath::Bezier(bezier) => {
            (0..=32).map(|i| bezier.point_at(i as f32 / 32.0)).collect()
        }
        ConnectorPath::Orthogonal(points) => points.clone(),
    }
    .into_iter()
    .map(|p| xf.w2s(Pos2::from(p)))
    .collect();
    for side in [-1.0, 1.0] {
        let offset = 1.45 * width.max(1.0) * xf.z * side;
        let rail: Vec<_> = center
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let tangent = center[(i + 1).min(center.len() - 1)] - center[i.saturating_sub(1)];
                p + Vec2::new(-tangent.y, tangent.x).normalized() * offset
            })
            .collect();
        let stroke = EStroke::new((0.7 * width).max(0.7) * xf.z, color);
        if access == GrasshopperPortAccess::Tree {
            painter.extend(egui::Shape::dashed_line(
                &rail,
                stroke,
                6.0 * xf.z,
                4.0 * xf.z,
            ));
        } else {
            painter.add(egui::Shape::line(rail, stroke));
        }
    }
}

fn literal_value(kind: GrasshopperPortType, text: &str) -> Result<GrasshopperValue, String> {
    if kind == GrasshopperPortType::Integer {
        return text
            .trim()
            .parse::<i32>()
            .map(|n| GrasshopperValue::Number(f64::from(n)))
            .map_err(|_| "Enter a whole number.".into());
    }
    if kind == GrasshopperPortType::Text {
        return Ok(GrasshopperValue::Text(text.into()));
    }
    match text.trim().parse::<f64>() {
        Ok(n) if n.is_finite() => Ok(GrasshopperValue::Number(n)),
        _ if kind == GrasshopperPortType::Data => Ok(GrasshopperValue::Text(text.into())),
        _ => Err("Enter a finite number.".into()),
    }
}
