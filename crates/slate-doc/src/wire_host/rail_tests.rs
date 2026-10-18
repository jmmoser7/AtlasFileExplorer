//! Fixed typed rails are geometry, not a painter-only adjustment.
use super::*;
use crate::scene::{ArtifactGeneratorRef, ConnectorEnd, PortalNode, Scene};

fn media(scene: &mut Scene, class: usize, height: f32) -> Node {
    let mut portal = if class == 2 {
        let mut p = PortalNode::unbound_web("Artifact");
        p.web.as_mut().unwrap().artifact = Some(Box::new(ArtifactGeneratorRef::default()));
        p
    } else {
        let mut p = PortalNode::unbound_agent("Media", "fixture");
        p.agent.as_mut().unwrap().view = if class == 0 {
            atlas_agent::PortalView::Images
        } else {
            atlas_agent::PortalView::Text
        };
        p
    };
    // All three classes intentionally share this geometry owner.
    portal.title = "Rail fixture".into();
    scene.build_node(
        WorldRect::new(20.0, 40.0, 680.0, height),
        NodeKind::Portal(portal),
    )
}

#[test]
fn media_text_and_artifact_rails_stay_top_justified_across_resize_and_rotation() {
    let mut scene = Scene::default();
    for class in 0..3 {
        for height in [36.0, 120.0, 400.0, 900.0] {
            for rotation in [0.0, 30.0, 90.0, 180.0] {
                let mut node = media(&mut scene, class, height);
                node.rotation_deg = rotation;
                let host = WireHost::from_node(&node);
                let inputs = crate::agent_inputs::input_ports_of(&node, false);
                for (row, port) in inputs.iter().enumerate() {
                    let expected = node.rect.rotate_point(
                        [
                            node.rect.x,
                            node.rect.y + FLOW_PORT_TOP + row as f32 * FLOW_PORT_PITCH,
                        ],
                        rotation,
                    );
                    assert!(
                        dist(host.anchor(Side::Left, port.t), expected) < 0.001,
                        "class{class}, h{height}, r{rotation}"
                    );
                }
                let expected = node.rect.rotate_point(
                    [node.rect.x + node.rect.w, node.rect.y + FLOW_PORT_TOP],
                    rotation,
                );
                assert!(
                    dist(
                        host.anchor(Side::Right, crate::agent_inputs::OUTPUT_T),
                        expected
                    ) < 0.001
                );
                if height < FLOW_PORT_TOP + (inputs.len() - 1) as f32 * FLOW_PORT_PITCH + 14.0 {
                    assert!(
                        host.ports().is_empty(),
                        "an invisible rail must not leave live hit targets"
                    );
                }
            }
        }
    }
}

#[test]
fn a_mixed_size_image_train_routes_straight_from_the_shared_saved_identities() {
    let mut scene = Scene::default();
    let left = media(&mut scene, 0, 180.0);
    let mut right = media(&mut scene, 0, 600.0);
    right.rect.x = 1000.0;
    let source = left.id;
    let target = right.id;
    let input = crate::agent_inputs::input_ports_of(&right, false)[0].t;
    scene.nodes.extend([left, right]);
    let a = ConnectorEnd::Anchored {
        node: source,
        side: Side::Right,
        t: crate::agent_inputs::OUTPUT_T,
    };
    let b = ConnectorEnd::Anchored {
        node: target,
        side: Side::Left,
        t: input,
    };
    for routing in [
        crate::wire::WireRouting::Bezier,
        crate::wire::WireRouting::Orthogonal,
    ] {
        let path = crate::wire::connector_route_in_scene(&scene, None, &a, &b, routing).unwrap();
        assert!((path.start()[1] - 70.0).abs() < 0.001);
        assert!((path.end()[1] - 70.0).abs() < 0.001);
    }
}

#[test]
fn view_and_chat_header_semantics_keep_their_distinct_datums() {
    let mut scene = Scene::default();
    let node = media(&mut scene, 0, 400.0);
    const VIEW: [crate::agent_inputs::InputPort; 1] = [crate::agent_inputs::InputPort {
        slot: atlas_agent::InputSlot::View,
        t: crate::agent_inputs::MODEL_VIEW_PORT_T,
        label: "View",
    }];
    let view = WireHost::from_node_flow(&node, &VIEW);
    assert!((view.ports()[0].point[1] - (40.0 + 400.0 * VIEW[0].t)).abs() < 0.001);
    let chat = scene.build_node(
        WorldRect::new(20.0, 40.0, 680.0, 400.0),
        NodeKind::Portal(PortalNode::unbound_agent("Chat", "fixture")),
    );
    let header = WireHost::from_node(&chat).anchor(Side::Right, 0.5);
    assert!((header[1] - (chat.rect.y + crate::link_inset::rail_inset())).abs() < 0.001);
}
