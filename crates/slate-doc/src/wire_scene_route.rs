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
    connector_route(a, b, host_of, routing, &obstacles, lane)
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
            && binding.slot.as_deref() == Some("memory")
    })?;
    let source = crate::agent_inputs::endpoint_node(binding.source_end(conn))?;
    let target = crate::agent_inputs::endpoint_node(binding.target_end(conn))?;
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
        for routing in [WireRouting::Bezier, WireRouting::Orthogonal] {
            let path = connector_route_in_scene_with_hosts(&scene, None, &a, &b, routing, |id| {
                if id == source {
                    Some(WireHost::from_rect(preview))
                } else {
                    scene_wire_hosts(&scene)(id)
                }
            })
            .unwrap();
            assert_eq!(path.start(), [180.0, 180.0]);
            assert_eq!(path.end(), [300.0, 30.0]);
            let authored = connector_route_in_scene(&scene, None, &a, &b, routing).unwrap();
            assert_eq!(authored.start(), [80.0, 30.0]);
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
            a: a.clone(),
            b: b.clone(),
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
