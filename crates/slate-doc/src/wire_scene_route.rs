//! Shared scene-aware route and bounds, including authored memory traces.
use super::*;

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
    if let Some(Node {
        kind: NodeKind::Connector(conn),
        ..
    }) = connector_id.and_then(|id| scene.node(id))
    {
        if let Some(path) = memory_route(conn, &host_of) {
            return Some(path);
        }
    }
    let obstacles = scene_wire_obstacles(scene);
    let lane = match (routing, connector_id) {
        (WireRouting::Orthogonal, Some(id)) => scene_ortho_lanes(scene)
            .get(&id)
            .copied()
            .unwrap_or_default(),
        _ => OrthoLane::default(),
    };
    let graft = connector_id.and_then(|id| graft_offset(scene, id));
    connector_route(
        a,
        b,
        |id| {
            let mut host = host_of(id)?;
            if let Some((target, offset)) = graft.filter(|(target, _)| *target == id) {
                let _ = target;
                let angle = host.rotation_deg.to_radians();
                host.rect.x -= offset * angle.sin();
                host.rect.y += offset * angle.cos();
            }
            Some(host)
        },
        routing,
        &obstacles,
        if graft.is_some() {
            OrthoLane::default()
        } else {
            lane
        },
    )
}

fn graft_offset(scene: &Scene, connector: NodeId) -> Option<(NodeId, f32)> {
    use crate::agent_inputs::endpoint_node;
    let NodeKind::Connector(conn) = &scene.node(connector)?.kind else {
        return None;
    };
    let binding = conn.binding.as_ref()?;
    let target = endpoint_node(binding.target_end(conn))?;
    let slot = binding.slot.as_deref()?;
    let agent = crate::agent_chat::agent(scene.node(target)?)?;
    if !agent.generator.grafted_slots.contains(slot) {
        return None;
    }
    let mut wires: Vec<_> = scene
        .nodes
        .iter()
        .filter_map(|node| {
            let NodeKind::Connector(conn) = &node.kind else {
                return None;
            };
            let binding = conn.binding.as_ref()?;
            (endpoint_node(binding.target_end(conn)) == Some(target)
                && binding.slot.as_deref() == Some(slot))
            .then(|| {
                (
                    if binding.order.is_empty() {
                        vec![node.id.0]
                    } else {
                        binding.order.clone()
                    },
                    node.id,
                )
            })
        })
        .collect();
    wires.sort();
    let index = wires.iter().position(|(_, id)| *id == connector)?;
    Some((
        target,
        crate::wire_host::graft_terminal_offset(index, wires.len()),
    ))
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
