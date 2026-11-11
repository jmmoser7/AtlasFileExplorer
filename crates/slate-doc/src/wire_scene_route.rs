//! Shared scene-aware route and bounds, including authored memory traces.
use super::*;
use std::cell::OnceCell;
use std::sync::Arc;

/// Visible-node host lookup for both interpreters.
pub fn scene_wire_hosts(scene: &Scene) -> impl Fn(NodeId) -> Option<WireHost> + '_ {
    |id| {
        scene
            .node(id)
            .filter(|n| !n.hidden)
            .map(WireHost::from_node)
    }
}

/// Scene-aware route: looks up hosts, obstacles, and the bundle lane for
/// `connector_id` so sibling orthogonal wires stay spaced.
pub fn connector_route_in_scene(
    scene: &Scene,
    connector_id: Option<NodeId>,
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    routing: WireRouting,
) -> Option<ConnectorPath> {
    connector_route_in_scene_with_hosts(scene, connector_id, a, b, routing, scene_wire_hosts(scene))
}

/// Route through the same scene knowledge with an ephemeral host pose lookup.
/// Authored memory rails, obstacle silhouettes and bundle lanes keep one owner;
/// previews replace geometry without changing the scene or its journal.
pub fn connector_route_in_scene_with_hosts(
    scene: &Scene,
    connector_id: Option<NodeId>,
    a: &ConnectorEnd,
    b: &ConnectorEnd,
    routing: WireRouting,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
) -> Option<ConnectorPath> {
    SceneWireRouting::default().route(scene, connector_id, a, b, routing, host_of)
}

/// Shared authored routing inputs for a single scene revision. Only square
/// wires use the obstacle and lane tables.
#[derive(Default)]
pub struct SceneWireRouting {
    obstacles: OnceCell<Vec<WireObstacle>>,
    lanes: OnceCell<HashMap<NodeId, OrthoLane>>,
}

impl SceneWireRouting {
    pub fn route(
        &self,
        scene: &Scene,
        connector_id: Option<NodeId>,
        a: &ConnectorEnd,
        b: &ConnectorEnd,
        routing: WireRouting,
        host_of: impl Fn(NodeId) -> Option<WireHost>,
    ) -> Option<ConnectorPath> {
        let connector = connector_id.and_then(|id| match &scene.node(id)?.kind {
            NodeKind::Connector(conn) => Some((id, conn)),
            _ => None,
        });
        if let Some((_, conn)) = connector {
            if let Some(path) = memory_route(conn, &host_of) {
                return Some(path);
            }
        }
        let obstacles = match routing {
            WireRouting::Bezier => &[][..],
            WireRouting::Orthogonal => self
                .obstacles
                .get_or_init(|| scene_wire_obstacles(scene))
                .as_slice(),
        };
        let graft = connector.and_then(|(id, conn)| {
            conn.binding.as_ref()?.slot.as_ref()?;
            scene.with_graft_terminals(|index| index.offsets.get(&id).copied())
        });
        let lane = match (routing, connector_id, graft) {
            (WireRouting::Orthogonal, Some(id), None) => self
                .lanes
                .get_or_init(|| scene_ortho_lanes(scene))
                .get(&id)
                .copied()
                .unwrap_or_default(),
            _ => OrthoLane::default(),
        };
        connector_route(
            a,
            b,
            |id| {
                let mut host = host_of(id)?;
                if let Some((_, offset)) = graft.filter(|(target, _)| *target == id) {
                    let angle = host.rotation_deg.to_radians();
                    host.rect.x -= offset * angle.sin();
                    host.rect.y += offset * angle.cos();
                }
                Some(host)
            },
            routing,
            obstacles,
            lane,
        )
    }
}

/// Authored graft groups do not depend on camera, host previews or renderer.
#[derive(Debug, Clone, Default)]
pub(crate) struct SceneGraftCache {
    revision: Option<(u64, usize)>,
    index: Arc<GraftTerminals>,
}

#[derive(Debug, Default)]
struct GraftTerminals {
    counts: HashMap<NodeId, HashMap<String, usize>>,
    offsets: HashMap<NodeId, (NodeId, f32)>,
}

impl Scene {
    fn with_graft_terminals<T>(&self, f: impl FnOnce(&GraftTerminals) -> T) -> T {
        let revision = (self.scene_gen(), self.nodes.len());
        let mut cache = self.graft_cache.borrow_mut();
        if cache.revision != Some(revision) {
            *cache = SceneGraftCache {
                revision: Some(revision),
                index: Arc::new(GraftTerminals::build(self)),
            };
        }
        f(&cache.index)
    }

    /// A grafted input's authored fan size, including hidden wires.
    pub fn graft_input_count(&self, target: NodeId, slot: &str) -> Option<usize> {
        self.with_graft_terminals(|index| index.counts.get(&target)?.get(slot).copied())
    }
}

impl GraftTerminals {
    fn build(scene: &Scene) -> Self {
        let mut groups: HashMap<_, Vec<(&[u64], NodeId)>> = scene
            .nodes
            .iter()
            .filter_map(|node| {
                Some((
                    node.id,
                    &crate::agent_chat::agent(node)?.generator.grafted_slots,
                ))
            })
            .flat_map(|(id, slots)| {
                slots
                    .iter()
                    .map(move |slot| ((id, slot.as_str()), Vec::new()))
            })
            .collect();
        if groups.is_empty() {
            return Self::default();
        }
        for node in &scene.nodes {
            let NodeKind::Connector(conn) = &node.kind else {
                continue;
            };
            let Some(binding) = &conn.binding else {
                continue;
            };
            let (Some(target), Some(slot)) = (
                crate::agent_inputs::endpoint_node(binding.target_end(conn)),
                binding.slot.as_deref(),
            ) else {
                continue;
            };
            let Some(wires) = groups.get_mut(&(target, slot)) else {
                continue;
            };
            let order = if binding.order.is_empty() {
                std::slice::from_ref(&node.id.0)
            } else {
                binding.order.as_slice()
            };
            wires.push((order, node.id));
        }
        let mut index = Self::default();
        for ((target, slot), mut wires) in groups {
            wires.sort_unstable();
            let count = wires.len();
            index
                .counts
                .entry(target)
                .or_default()
                .insert(slot.to_owned(), count);
            for (ordinal, (_, id)) in wires.into_iter().enumerate() {
                let offset = crate::wire_host::graft_terminal_offset(ordinal, count);
                index.offsets.insert(id, (target, offset));
            }
        }
        index
    }
}

/// AABB of the derived route (connector `Node.rect` stays equal to this).
pub fn connector_aabb_routed(
    conn: &ConnectorNode,
    host_of: impl Fn(NodeId) -> Option<WireHost>,
    routing: WireRouting,
    obstacles: &[WireObstacle],
    lane: OrthoLane,
) -> Option<WorldRect> {
    if let Some(path) = memory_route(conn, &host_of) {
        return Some(path.aabb());
    }
    connector_route(
        &conn.a,
        &conn.b,
        host_of,
        conn.effective_routing(routing),
        obstacles,
        lane,
    )
    .map(|p| p.aabb())
}

/// AABB of the derived curve — the connector node's `rect` is kept equal to
/// this so marquee/hit systems keep working. `None` when unresolvable.
pub fn connector_aabb(
    conn: &ConnectorNode,
    rect_of: impl Fn(NodeId) -> Option<WorldRect>,
) -> Option<WorldRect> {
    if let Some(path) = memory_route(conn, &|id| rect_of(id).map(WireHost::from_rect)) {
        return Some(path.aabb());
    }
    crate::scene::connector_bezier(&conn.a, &conn.b, rect_of).map(|bez| bez.aabb())
}

fn memory_route(
    conn: &ConnectorNode,
    host_of: &impl Fn(NodeId) -> Option<WireHost>,
) -> Option<ConnectorPath> {
    let binding = conn.binding.as_ref().filter(|binding| {
        binding.kind == crate::agent_inputs::InputKind::Memory
            && matches!(binding.slot.as_deref(), Some("memory" | "generator_memory"))
    })?;
    let source = crate::agent_inputs::endpoint_node(binding.source_end(conn))?;
    let target = crate::agent_inputs::endpoint_node(binding.target_end(conn))?;
    if binding.slot.as_deref() == Some("generator_memory") {
        let source = host_of(source)?;
        let target = host_of(target)?;
        let a = source.rect.rotate_point(
            [source.rect.x + source.rect.w - 6.0, source.rect.y + 10.0],
            source.rotation_deg,
        );
        let b = target.rect.rotate_point(
            [target.rect.x + 6.0, target.rect.y + 10.0],
            target.rotation_deg,
        );
        return Some(ConnectorPath::Bezier(
            crate::scene::connector_bezier_from_dirs(a, [1.0, 0.0], b, [-1.0, 0.0]),
        ));
    }
    Some(ConnectorPath::Bezier(crate::agent_chat::rail(
        host_of(source)?.rect,
        host_of(target)?.rect,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{ImageNode, SceneCmd, Stroke};
    use crate::ItemId;

    fn add(scene: &mut Scene, rect: WorldRect, kind: NodeKind) -> NodeId {
        let node = scene.build_node(rect, kind);
        let id = node.id;
        scene.apply(&SceneCmd::Add {
            index: scene.nodes.len(),
            node,
        });
        id
    }

    fn hosts() -> (Scene, NodeId, NodeId, ConnectorEnd, ConnectorEnd) {
        let mut scene = Scene::default();
        let a = add(
            &mut scene,
            WorldRect::new(0.0, 0.0, 80.0, 60.0),
            NodeKind::Image(ImageNode::new(ItemId(1))),
        );
        let b = add(
            &mut scene,
            WorldRect::new(300.0, 0.0, 80.0, 60.0),
            NodeKind::Image(ImageNode::new(ItemId(2))),
        );
        (
            scene,
            a,
            b,
            ConnectorEnd::Anchored {
                node: a,
                side: Side::Right,
                t: 0.5,
            },
            ConnectorEnd::Anchored {
                node: b,
                side: Side::Left,
                t: 0.5,
            },
        )
    }

    #[test]
    fn transient_host_pose_moves_each_route_endpoint_without_mutating_scene() {
        let (scene, source, _, a, b) = hosts();
        let before = scene.clone();
        let preview = WorldRect::new(100.0, 150.0, 80.0, 60.0);
        let mut preview_host = WireHost::from_node(scene.node(source).unwrap());
        preview_host.rect = preview;
        let authored_host = WireHost::from_node(scene.node(source).unwrap());
        for routing in [WireRouting::Bezier, WireRouting::Orthogonal] {
            let path = connector_route_in_scene_with_hosts(&scene, None, &a, &b, routing, |id| {
                if id == source {
                    Some(preview_host.clone())
                } else {
                    scene_wire_hosts(&scene)(id)
                }
            })
            .unwrap();
            assert_eq!(path.start(), preview_host.anchor(Side::Right, 0.5));
            assert_eq!(path.end(), [300.0, 30.0]);
            let authored = connector_route_in_scene(&scene, None, &a, &b, routing).unwrap();
            assert_eq!(authored.start(), authored_host.anchor(Side::Right, 0.5));
            assert_eq!(
                [
                    path.start()[0] - authored.start()[0],
                    path.start()[1] - authored.start()[1],
                ],
                [
                    preview.x - authored_host.rect.x,
                    preview.y - authored_host.rect.y
                ]
            );
        }
        assert_eq!(scene, before);
    }

    #[test]
    fn missing_or_hidden_preview_host_never_draws_a_stale_attached_wire() {
        let (scene, source, _, a, b) = hosts();
        let path =
            connector_route_in_scene_with_hosts(&scene, None, &a, &b, WireRouting::Bezier, |id| {
                (id != source)
                    .then(|| scene_wire_hosts(&scene)(id))
                    .flatten()
            });
        assert!(path.is_none());
    }

    #[test]
    fn graft_terminals_share_order_and_follow_scene_edits() {
        use crate::agent_inputs::{InputKind, WireBinding};
        let (mut scene, _, target, a, b) = hosts();
        let mut agent = crate::scene::AgentPortalRef::unbound(atlas_agent::PortalView::Images);
        agent.generator.grafted_slots.insert("media".into());
        let target_node = scene.node_mut(target).unwrap();
        let NodeKind::Image(image) = &mut target_node.kind else {
            panic!("image host");
        };
        image.agent = Some(Box::new(agent));
        let add_wire = |scene: &mut Scene, order, input_b| {
            let (from, to) = if input_b { (a, b) } else { (b, a) };
            add(
                scene,
                WorldRect::default(),
                NodeKind::Connector(ConnectorNode {
                    a: from,
                    b: to,
                    stroke: Stroke::none(),
                    color: None,
                    routing: None,
                    arrow_a: false,
                    arrow_b: false,
                    label: None,
                    display: Default::default(),
                    crosstalk: None,
                    binding: Some(WireBinding {
                        input_b,
                        kind: InputKind::Images,
                        output: None,
                        order,
                        all_images: false,
                        consumed: false,
                        slot: Some("media".into()),
                    }),
                }),
            )
        };
        let late = add_wire(&mut scene, Vec::new(), true);
        let early = add_wire(&mut scene, vec![1], false);
        scene.node_mut(late).unwrap().hidden = true;
        let anchor = scene_wire_hosts(&scene)(target)
            .unwrap()
            .anchor(Side::Left, 0.5);
        let target_y = |scene: &Scene, id| {
            let NodeKind::Connector(conn) = &scene.node(id).unwrap().kind else {
                panic!("connector");
            };
            let path =
                connector_route_in_scene(scene, Some(id), &conn.a, &conn.b, WireRouting::Bezier)
                    .unwrap();
            if conn.binding.as_ref().unwrap().input_b {
                path.end()[1]
            } else {
                path.start()[1]
            }
        };
        assert_eq!(scene.graft_input_count(target, "media"), Some(2));
        assert_eq!(target_y(&scene, early), anchor[1] - 2.0);
        assert_eq!(target_y(&scene, late), anchor[1] + 2.0);

        let before = scene.node(target).unwrap().clone();
        let mut after = before.clone();
        crate::agent_chat::agent_mut(&mut after)
            .unwrap()
            .generator
            .grafted_slots
            .clear();
        let edit = SceneCmd::Patch {
            before: Box::new(before),
            after: Box::new(after),
        };
        assert!(scene.apply(&edit));
        assert_eq!(scene.graft_input_count(target, "media"), None);
        assert_eq!(target_y(&scene, early), anchor[1]);
        assert!(scene.apply(&edit.inverted()));
        assert_eq!(target_y(&scene, early), anchor[1] - 2.0);
    }

    #[test]
    fn memory_rail_uses_the_same_transient_hosts_before_generic_routing() {
        let (mut scene, source, target, a, b) = hosts();
        let conn = ConnectorNode {
            a,
            b,
            stroke: Stroke::none(),
            color: None,
            routing: None,
            arrow_a: false,
            arrow_b: false,
            label: None,
            display: Default::default(),
            crosstalk: None,
            binding: Some(crate::agent_inputs::WireBinding {
                input_b: true,
                kind: crate::agent_inputs::InputKind::Memory,
                output: None,
                order: Vec::new(),
                all_images: false,
                consumed: false,
                slot: Some("memory".into()),
            }),
        };
        let wire = add(&mut scene, WorldRect::default(), NodeKind::Connector(conn));
        let preview = WorldRect::new(100.0, 150.0, 80.0, 60.0);
        let path = connector_route_in_scene_with_hosts(
            &scene,
            Some(wire),
            &a,
            &b,
            WireRouting::Orthogonal,
            |id| {
                if id == source {
                    Some(WireHost::from_rect(preview))
                } else {
                    scene_wire_hosts(&scene)(id)
                }
            },
        )
        .unwrap();
        assert_eq!(
            path,
            ConnectorPath::Bezier(crate::agent_chat::rail(
                preview,
                scene.node(target).unwrap().rect
            ))
        );
    }
}
