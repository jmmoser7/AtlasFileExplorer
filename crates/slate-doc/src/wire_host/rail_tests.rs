//! Fixed typed rails are geometry, not a painter-only adjustment.
use super::*;
use crate::scene::{ArtifactGeneratorRef, ConnectorEnd, ImageNode, PortalNode, Scene};

fn source_image(scene: &mut Scene, height: f32) -> Node {
    scene.build_node(
        WorldRect::new(20.0, 40.0, 680.0, height),
        NodeKind::Image(ImageNode::new(crate::ItemId(1))),
    )
}

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
    let image = source_image(&mut scene, 400.0);
    let view = WireHost::from_node_flow(&image, &VIEW);
    for port in view.ports() {
        assert!(
            dist(
                port.point,
                connector_anchor_point(image.rect, port.side, port.t)
            ) < 0.001
        );
    }
    let chat = scene.build_node(
        WorldRect::new(20.0, 40.0, 680.0, 400.0),
        NodeKind::Portal(PortalNode::unbound_agent("Chat", "fixture")),
    );
    let header = WireHost::from_node(&chat).anchor(Side::Right, 0.5);
    assert!((header[1] - (chat.rect.y + crate::link_inset::rail_inset())).abs() < 0.001);
}

#[test]
fn ordinary_source_output_uses_the_top_datum_and_short_sources_offer_no_ports() {
    let mut scene = Scene::default();
    for height in [12.0, 36.0, 44.0, 120.0, 600.0] {
        for rotation in [0.0, 30.0, 90.0, 180.0] {
            let mut node = source_image(&mut scene, height);
            node.rotation_deg = rotation;
            let host = WireHost::from_node(&node);
            assert!(
                !host.is_flow(),
                "an ordinary source has no generator input rail"
            );
            let expected = node.rect.rotate_point(
                [node.rect.x + node.rect.w, node.rect.y + FLOW_PORT_TOP],
                rotation,
            );
            assert!(
                dist(
                    host.anchor(Side::Right, crate::agent_inputs::OUTPUT_T),
                    expected
                ) < 0.001,
                "h{height}, r{rotation}"
            );
            assert_eq!(host.offers_ports(), height >= FLOW_PORT_TOP + 14.0);
            if host.offers_ports() {
                let output = host
                    .ports()
                    .into_iter()
                    .find(|port| port.side == Side::Right)
                    .unwrap();
                assert_eq!(
                    output.t,
                    crate::agent_inputs::OUTPUT_T,
                    "saved output identity stays stable"
                );
                assert!(dist(output.point, expected) < 0.001);
            } else {
                assert!(
                    host.ports().is_empty(),
                    "short sources leave no invisible port targets"
                );
            }
        }
    }
}

#[test]
fn ordinary_source_arbitrary_saved_anchors_keep_their_existing_edge_geometry() {
    let mut scene = Scene::default();
    for height in [12.0, 400.0] {
        for rotation in [0.0, 37.0, 90.0] {
            let mut node = source_image(&mut scene, height);
            node.rotation_deg = rotation;
            let host = WireHost::from_node(&node);
            for side in Side::BOX {
                for t in [-0.2, 0.0, 0.2, 0.5, 0.71, 1.0, 1.4] {
                    if side == Side::Right && t == crate::agent_inputs::OUTPUT_T {
                        continue;
                    }
                    let expected = node
                        .rect
                        .rotate_point(connector_anchor_point(node.rect, side, t), rotation);
                    assert!(
                        dist(host.anchor(side, t), expected) < 0.001,
                        "saved {side:?}/{t}, h{height}, r{rotation}"
                    );
                }
            }
        }
    }
}

#[test]
fn ordinary_source_to_generator_routes_share_the_top_datum_without_moving_cards() {
    for source_height in [44.0, 180.0, 700.0] {
        for target_height in [180.0, 600.0] {
            let mut scene = Scene::default();
            let source = source_image(&mut scene, source_height);
            let mut target = media(&mut scene, 0, target_height);
            target.rect.x = 1000.0;
            let a = ConnectorEnd::Anchored {
                node: source.id,
                side: Side::Right,
                t: crate::agent_inputs::OUTPUT_T,
            };
            let b = ConnectorEnd::Anchored {
                node: target.id,
                side: Side::Left,
                t: crate::agent_inputs::input_ports_of(&target, false)[0].t,
            };
            scene.nodes.extend([source, target]);
            let before = scene.clone();
            for routing in [
                crate::wire::WireRouting::Bezier,
                crate::wire::WireRouting::Orthogonal,
            ] {
                let path =
                    crate::wire::connector_route_in_scene(&scene, None, &a, &b, routing).unwrap();
                assert!((path.start()[1] - (40.0 + FLOW_PORT_TOP)).abs() < 0.001);
                assert!(
                    (path.end()[1] - path.start()[1]).abs() < 0.001,
                    "aligned cards {source_height}/{target_height}, {routing:?}"
                );
            }
            assert_eq!(scene, before, "datum resolution never moves either card");
        }
    }
}
