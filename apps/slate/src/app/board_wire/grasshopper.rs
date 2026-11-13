//! Named Grasshopper wire grips, data styles and reversible wire presentation.
use super::*;
use atlas_shell::{canvas_scale, canvas_text};
use slate_doc::scene::{
    grasshopper_wire, GrasshopperPort, GrasshopperPortAccess, GrasshopperPortDirection,
    GrasshopperPortId, GrasshopperPortType, GrasshopperValue,
};
use slate_doc::wire_host::{grasshopper_port_row, grasshopper_port_t};
use std::collections::BTreeMap;

const LITERAL_POPUP: &str = "grasshopper-input-value";
type Pair = (NodeId, NodeId);

pub(super) struct WireBundle {
    pub wires: Vec<NodeId>,
    trunk: ConnectorBezier,
    leads: Vec<[ConnectorBezier; 2]>,
}

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
    fn wire_pair(node: &Node) -> Option<(Pair, bool)> {
        let NodeKind::Connector(conn) = &node.kind else {
            return None;
        };
        let wire = grasshopper_wire(conn)?;
        Some(((wire.source, wire.target), wire.bundled))
    }

    pub(crate) fn grasshopper_wire_bundle_choice(&self, id: NodeId) -> Option<bool> {
        let scene = &self.doc().scene;
        let (pair, bundled) = Self::wire_pair(scene.node(id)?)?;
        if self.tab().read_only {
            return None;
        }
        let mut count = 0;
        for node in &scene.nodes {
            if Self::wire_pair(node).is_some_and(|(other, _)| other == pair) {
                if node.locked {
                    return None;
                }
                count += 1;
            }
        }
        (bundled || count > 1).then_some(!bundled)
    }

    pub(crate) fn set_grasshopper_wire_bundle(&mut self, id: NodeId, bundled: bool) -> bool {
        if self.grasshopper_wire_bundle_choice(id) != Some(bundled) {
            return false;
        }
        let scene = &self.doc().scene;
        let Some((pair, _)) = scene.node(id).and_then(Self::wire_pair) else {
            return false;
        };
        self.set_grasshopper_bundle_pairs(&[pair], bundled)
    }

    pub(crate) fn set_grasshopper_grip_bundle(
        &mut self,
        node: NodeId,
        port: &GrasshopperPortId,
        side: Side,
        bundled: bool,
    ) -> bool {
        if self.tab().read_only || !matches!(side, Side::Left | Side::Right) {
            return false;
        }
        let pairs = self.grasshopper_grip_pairs(node, port, side, bundled);
        self.set_grasshopper_bundle_pairs(&pairs, bundled)
    }

    fn grasshopper_grip_pairs(
        &self,
        host: NodeId,
        port: &GrasshopperPortId,
        side: Side,
        bundled: bool,
    ) -> Vec<Pair> {
        let mut groups: BTreeMap<Pair, (usize, bool, bool)> = BTreeMap::new();
        for node in &self.doc().scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(wire) = grasshopper_wire(conn) else {
                continue;
            };
            let pair = (wire.source, wire.target);
            if host
                != if side == Side::Left {
                    wire.target
                } else {
                    wire.source
                }
            {
                continue;
            }
            let group = groups.entry(pair).or_default();
            group.0 += 1;
            group.1 |= node.locked;
            let endpoint = if side == Side::Left {
                &wire.target_port
            } else {
                &wire.source_port
            };
            group.2 |= endpoint == port && wire.bundled != bundled;
        }
        groups
            .into_iter()
            .filter_map(|(pair, (count, locked, applies))| {
                (applies && !locked && (count > 1 || !bundled)).then_some(pair)
            })
            .collect()
    }

    fn set_grasshopper_bundle_pairs(&mut self, pairs: &[Pair], bundled: bool) -> bool {
        let commands = self
            .doc()
            .scene
            .nodes
            .iter()
            .filter_map(|before| {
                let (pair, current) = Self::wire_pair(before)?;
                if current == bundled || !pairs.contains(&pair) || before.locked {
                    return None;
                }
                let mut after = before.clone();
                let NodeKind::Connector(conn) = &mut after.kind else {
                    return None;
                };
                conn.grasshopper.as_deref_mut()?.bundled = bundled;
                Some(SceneCmd::Patch {
                    before: Box::new(before.clone()),
                    after: Box::new(after),
                })
            })
            .collect::<Vec<_>>();
        !commands.is_empty() && self.commit_scene(commands)
    }

    pub(super) fn grasshopper_bundles(&self, nodes: &[Node]) -> Vec<WireBundle> {
        let mut groups: BTreeMap<Pair, Vec<&Node>> = BTreeMap::new();
        for node in nodes {
            if let Some((pair, true)) = Self::wire_pair(node).filter(|_| !node.hidden) {
                groups.entry(pair).or_default().push(node);
            }
        }
        groups
            .into_values()
            .filter_map(|nodes| {
                if nodes.len() < 2 {
                    return None;
                }
                let members = nodes
                    .into_iter()
                    .filter_map(|node| {
                        let NodeKind::Connector(conn) = &node.kind else {
                            return None;
                        };
                        let [a, b] = self.current_grasshopper_ends(conn);
                        let ends = [self.wire_end_point(&a)?, self.wire_end_point(&b)?];
                        let [source, target] = [a, b].map(|end| match end {
                            ConnectorEnd::Anchored { node, .. } => {
                                self.doc().scene.node(node).is_some_and(|n| !n.hidden)
                            }
                            _ => false,
                        });
                        (source && target).then_some((node.id, ends))
                    })
                    .collect::<Vec<_>>();
                WireBundle::new(&members)
            })
            .collect()
    }

    pub(crate) fn grasshopper_bundle_at(&self, pointer: Pos2, xf: &BoardXf) -> Option<NodeId> {
        self.grasshopper_bundles(&self.doc().scene.nodes)
            .into_iter()
            .find_map(|bundle| {
                let reach = CONNECTOR_PICK_PX / xf.z;
                bundle.hit(xf.s2w(pointer), reach)
            })
    }

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

impl WireBundle {
    fn new(members: &[(NodeId, [Pos2; 2])]) -> Option<Self> {
        if members.len() < 2 {
            return None;
        }
        let average = |index: usize| {
            Pos2::ZERO
                + members
                    .iter()
                    .map(|(_, points)| points[index].to_vec2())
                    .fold(Vec2::ZERO, |a, b| a + b)
                    / members.len() as f32
        };
        let a = average(0);
        let b = average(1);
        let span = b.x - a.x;
        let inset = Vec2::new((span * 0.22).clamp(-72.0, 72.0), 0.0);
        let join = a + inset;
        let split = b - inset;
        let curve = |from: Pos2, to: Pos2| {
            let span = to.x - from.x;
            let side = if span == 0.0 { 0.0 } else { span.signum() };
            let middle = from.x + span * 0.5;
            let mut bezier = slate_doc::scene::connector_bezier_from_dirs(
                from.into(),
                [side, 0.0],
                to.into(),
                [-side, 0.0],
            );
            // A steep fan can be much taller than its short horizontal lead.
            // Keep the two horizontal handles ordered within the endpoint span.
            bezier.c1[0] = bezier.c1[0].clamp(from.x.min(middle), from.x.max(middle));
            bezier.c2[0] = bezier.c2[0].clamp(to.x.min(middle), to.x.max(middle));
            bezier
        };
        let trunk = curve(join, split);
        let leads = members
            .iter()
            .map(|(_, [a, b])| [curve(*a, join), curve(split, *b)])
            .collect();
        let wires = members.iter().map(|(id, _)| *id).collect();
        Some(Self {
            wires,
            trunk,
            leads,
        })
    }

    pub(super) fn paint(
        &self,
        painter: &egui::Painter,
        xf: &BoardXf,
        color: Color32,
        selected: bool,
    ) {
        let lead_ink = color.gamma_multiply(0.83);
        for pair in &self.leads {
            for &path in pair {
                paint_route_preview(
                    painter,
                    xf,
                    &ConnectorPath::Bezier(path),
                    EStroke::new(1.7 * xf.z, lead_ink),
                );
            }
        }
        let trunk = EStroke::new(if selected { 4.4 } else { 3.8 } * xf.z, color);
        paint_route_preview(painter, xf, &ConnectorPath::Bezier(self.trunk), trunk);
        for point in [self.trunk.p0, self.trunk.p3] {
            painter.circle_filled(xf.w2s(Pos2::from(point)), trunk.width * 0.5, trunk.color);
        }
    }

    fn hit(&self, point: Pos2, reach: f32) -> Option<NodeId> {
        let hits = |path: &ConnectorBezier| {
            let nearest = (1..=20).map(|index| Pos2::from(path.point_at(index as f32 / 20.0)));
            let mut previous = Pos2::from(path.p0);
            for next in nearest {
                let delta = next - previous;
                let t = ((point - previous).dot(delta) / delta.length_sq().max(f32::EPSILON))
                    .clamp(0.0, 1.0);
                if point.distance(previous + delta * t) <= reach {
                    return true;
                }
                previous = next;
            }
            false
        };
        self.leads
            .iter()
            .zip(&self.wires)
            .find_map(|(pair, id)| pair.iter().any(hits).then_some(*id))
            .or_else(|| hits(&self.trunk).then_some(self.wires[0]))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_leads_reach_their_ports_and_short_steep_spans_stay_monotone() {
        for target_x in [18.0, 315.0, 318.0, 395.0] {
            let members = [
                (
                    NodeId(1),
                    [Pos2::new(315.0, 417.0), Pos2::new(target_x, 560.0)],
                ),
                (
                    NodeId(2),
                    [Pos2::new(315.0, 452.0), Pos2::new(target_x, 584.0)],
                ),
            ];
            let bundle = WireBundle::new(&members).unwrap();
            for ((id, [source, target]), [left, right]) in members.iter().zip(&bundle.leads) {
                assert_eq!(left.p0, <[f32; 2]>::from(*source));
                assert_eq!(left.p3, bundle.trunk.p0);
                assert_eq!(right.p0, bundle.trunk.p3);
                assert_eq!(right.p3, <[f32; 2]>::from(*target));
                assert_eq!(bundle.hit(*target, 0.1), Some(*id));
            }
            for curve in std::iter::once(&bundle.trunk).chain(bundle.leads.iter().flatten()) {
                let span = curve.p3[0] - curve.p0[0];
                let direction = span.signum();
                let xs = [curve.p0[0], curve.c1[0], curve.c2[0], curve.p3[0]];
                if span == 0.0 {
                    assert!(xs.iter().all(|x| *x == xs[0]));
                }
                assert!(xs.windows(2).all(|x| (x[1] - x[0]) * direction >= 0.0));
            }
        }
    }
}
