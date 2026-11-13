//! Reversible Grasshopper wire bundles and their fan-out geometry.
use super::*;
use std::collections::BTreeMap;

type Pair = (NodeId, NodeId);

pub(in crate::app::board_wire) struct WireBundle {
    pub wires: Vec<NodeId>,
    trunk: ConnectorBezier,
    leads: Vec<[ConnectorBezier; 2]>,
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

    pub(super) fn grasshopper_grip_pairs(
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

    pub(in crate::app::board_wire) fn grasshopper_bundles(
        &self,
        nodes: &[Node],
    ) -> Vec<WireBundle> {
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

    pub(in crate::app::board_wire) fn paint(
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
