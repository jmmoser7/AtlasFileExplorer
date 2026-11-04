use super::*;
use crate::agent_inputs::{InputPort, GENERATOR_PORTS, MODEL_VIEW_PORT_T, OUTPUT_T};
const VIEW: InputPort = InputPort {
    slot: atlas_agent::InputSlot::View,
    t: MODEL_VIEW_PORT_T,
    label: "View",
};
const MIXED: [InputPort; 4] = [
    GENERATOR_PORTS[0],
    GENERATOR_PORTS[1],
    GENERATOR_PORTS[2],
    VIEW,
];

#[test]
fn mixed_model_roles_match_graph_datums_and_keep_native_view_and_arbitrary_anchors() {
    for height in [48.0, 100.0, 390.0] {
        for rotation in [0.0, 37.0, 90.0] {
            let mut scene = crate::Scene::default();
            let mut node = scene.build_node(
                WorldRect::new(20.0, 30.0, 420.0, height),
                crate::NodeKind::Image(crate::scene::ImageNode::new(crate::ItemId::NONE)),
            );
            node.rotation_deg = rotation;
            let graph = WireHost::from_node_flow(&node, &GENERATOR_PORTS);
            let mixed = WireHost::from_node_flow(&node, &MIXED);
            let legacy = WireHost::from_node_flow(&node, &[]);
            let view_only = WireHost::from_node_flow(&node, &[VIEW]);
            for (side, t) in GENERATOR_PORTS
                .iter()
                .map(|p| (Side::Left, p.t))
                .chain([(Side::Right, OUTPUT_T)])
            {
                let (a, b) = (graph.anchor(side, t), mixed.anchor(side, t));
                assert!(
                    (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4,
                    "{height} {rotation} {side:?} {t}"
                );
            }
            for t in [MODEL_VIEW_PORT_T, 0.69, 0.91] {
                assert_eq!(mixed.anchor(Side::Left, t), legacy.anchor(Side::Left, t));
                assert_eq!(
                    view_only.anchor(Side::Left, t),
                    legacy.anchor(Side::Left, t)
                );
            }
            assert_eq!(
                view_only.anchor(Side::Right, OUTPUT_T),
                legacy.anchor(Side::Right, OUTPUT_T)
            );
        }
    }
}

#[test]
fn explicit_typed_targets_and_saved_view_identity_are_distinct_from_legacy_edge_fallback() {
    use crate::agent_inputs::{infer_binding_with, InputKind};
    use crate::scene::{ConnectorEnd, ConnectorNode, Stroke};
    let mut doc = crate::SlateDoc::default();
    let png = doc.add_item("shot.png".into(), "shot.png", 1, 0, "shot");
    let model = doc.add_item("model.obj".into(), "model.obj", 1, 0, "model");
    let source = doc.scene.build_node(
        WorldRect::new(-400.0, 0.0, 220.0, 160.0),
        crate::NodeKind::Image(crate::scene::ImageNode::new(png)),
    );
    let target = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 420.0, 390.0),
        crate::NodeKind::Image(crate::scene::ImageNode::new(model)),
    );
    let (source_id, target_id) = (source.id, target.id);
    doc.scene.nodes.extend([source, target]);
    let a = ConnectorEnd::Anchored {
        node: source_id,
        side: Side::Right,
        t: OUTPUT_T,
    };
    let b = |t| ConnectorEnd::Anchored {
        node: target_id,
        side: Side::Left,
        t,
    };
    let paths = |id| doc.item(id).map(|item| item.path.as_path());
    for port in [GENERATOR_PORTS[0], GENERATOR_PORTS[2]] {
        let typed = infer_binding_with(&doc.scene, &a, &b(port.t), &paths).unwrap();
        assert_eq!(typed.slot.as_deref(), Some(port.slot.id()));
    }
    for t in [MODEL_VIEW_PORT_T, 0.26, 0.69] {
        let inferred = infer_binding_with(&doc.scene, &a, &b(t), &paths).unwrap();
        assert_eq!(inferred.slot.as_deref(), Some("view"));
    }
    let view = infer_binding_with(&doc.scene, &a, &b(MODEL_VIEW_PORT_T), &paths).unwrap();
    assert_eq!(view.kind, InputKind::Images);
    let conn = ConnectorNode {
        a,
        b: b(GENERATOR_PORTS[0].t),
        binding: Some(view),
        stroke: Stroke::none(),
        color: None,
        routing: None,
        arrow_a: false,
        arrow_b: false,
        label: None,
        display: Default::default(),
        crosstalk: None,
    };
    let saved = serde_json::to_vec(&conn).unwrap();
    let restored: ConnectorNode = serde_json::from_slice(&saved).unwrap();
    assert_eq!(
        restored.binding.unwrap().slot.as_deref(),
        Some("view"),
        "saved explicit binding is never re-inferred from its new port geometry"
    );
}

#[test]
fn real_model_roles_hide_only_mixed_view_grips_and_keep_generator_output_datums() {
    let mut doc = crate::SlateDoc::default();
    let model = doc.add_item("model.obj".into(), "model.obj", 1, 0, "model");
    let text = doc.add_item("note.txt".into(), "note.txt", 1, 0, "text");
    for height in [1.0, 36.0, 100.0, 390.0] {
        for rotation in [0.0, 37.0, 90.0] {
            let mut node = doc.scene.build_node(
                WorldRect::new(20.0, 40.0, 420.0, height),
                crate::NodeKind::Image(crate::scene::ImageNode::new(model)),
            );
            node.rotation_deg = rotation;
            let before = node.clone();
            let roles = crate::agent_inputs::input_ports_for(&node, &|id| {
                doc.item(id).map(|item| item.path.as_path())
            });
            assert_eq!(roles.len(), 4);
            let host = WireHost::from_node_flow(&node, roles);
            assert_eq!(host.ports().len(), 4);
            assert!(!host
                .ports()
                .iter()
                .any(|p| p.side == Side::Left && p.t == MODEL_VIEW_PORT_T));
            let scale = (height / (FLOW_PORT_TOP + 2.0 * FLOW_PORT_PITCH + 14.0)).min(1.0);
            let fixed_output = node.rect.rotate_point(
                [
                    node.rect.x + node.rect.w,
                    node.rect.y + FLOW_PORT_TOP * scale,
                ],
                rotation,
            );
            let native_output = node.rect.rotate_point(
                [node.rect.x + node.rect.w, node.rect.y + height * 0.5],
                rotation,
            );
            assert!(
                dist(host.anchor(Side::Right, OUTPUT_T), native_output) < 0.001,
                "an unconfigured native Model retains its ordinary midpoint output"
            );
            for (row, port) in GENERATOR_PORTS.iter().enumerate() {
                assert!(
                    dist(
                        host.anchor(Side::Left, port.t),
                        node.rect.rotate_point(
                            [
                                node.rect.x,
                                node.rect.y
                                    + (FLOW_PORT_TOP + row as f32 * FLOW_PORT_PITCH) * scale
                            ],
                            rotation
                        )
                    ) < 0.001
                );
            }
            let mut linked_text = node.clone();
            let crate::NodeKind::Image(image) = &mut linked_text.kind else {
                unreachable!()
            };
            image.item = text;
            let text_roles = crate::agent_inputs::input_ports_for(&linked_text, &|id| {
                doc.item(id).map(|item| item.path.as_path())
            });
            let text_host = WireHost::from_node_flow(&linked_text, text_roles);
            assert!(
                dist(text_host.anchor(Side::Right, OUTPUT_T), native_output) < 0.001,
                "unconfigured linked Text retains its ordinary midpoint output"
            );
            // Bound/generated sources retain the authored generator datum,
            // including after serialization; native source disclosure is separate.
            for (item, view, provider) in [
                (model, atlas_agent::PortalView::Chat, "codex"),
                (text, atlas_agent::PortalView::Text, "ollama"),
            ] {
                let mut bound = node.clone();
                let crate::NodeKind::Image(image) = &mut bound.kind else {
                    unreachable!()
                };
                image.item = item;
                let mut agent = crate::scene::AgentPortalRef::unbound(view);
                agent.provider = provider.into();
                agent.session = "saved-bound-generator-session".into();
                image.agent = Some(Box::new(agent));
                let saved = serde_json::to_vec(&bound).unwrap();
                let restored: crate::Node = serde_json::from_slice(&saved).unwrap();
                for configured in [&bound, &restored] {
                    let roles = crate::agent_inputs::input_ports_for(configured, &|id| {
                        doc.item(id).map(|item| item.path.as_path())
                    });
                    assert!(
                        dist(
                            WireHost::from_node_flow(configured, roles)
                                .anchor(Side::Right, OUTPUT_T),
                            fixed_output
                        ) < 0.001,
                        "bound {provider} output retains its fixed generator datum"
                    );
                }
            }
            let view_only = WireHost::from_node_flow(&node, &[VIEW]);
            assert_eq!(
                view_only.ports().len(),
                2,
                "View-only public callers retain their native port"
            );
            for t in [MODEL_VIEW_PORT_T, 0.69, 0.91] {
                assert_eq!(host.anchor(Side::Left, t), view_only.anchor(Side::Left, t));
            }
            for class in 0..3 {
                let mut configured = node.clone();
                let crate::NodeKind::Image(image) = &mut configured.kind else {
                    unreachable!()
                };
                if class == 0 {
                    image.item = crate::ItemId::NONE;
                } else {
                    let mut agent =
                        crate::scene::AgentPortalRef::unbound(atlas_agent::PortalView::Images);
                    if class == 1 {
                        agent.provider = "comfy".into();
                        agent.session = "actual-bound-session".into();
                    } else {
                        agent.generator.input_source_kind =
                            Some(crate::scene::GraphInputKind::Image);
                    }
                    image.agent = Some(Box::new(agent));
                }
                let configured_host = WireHost::from_node_flow(&configured, &GENERATOR_PORTS);
                let expected = if class == 0 {
                    native_output
                } else if class == 2 {
                    node.rect.rotate_point(
                        [
                            node.rect.x + node.rect.w,
                            node.rect.y + FLOW_PORT_TOP.min(height),
                        ],
                        rotation,
                    )
                } else {
                    fixed_output
                };
                assert!(
                    dist(configured_host.anchor(Side::Right, OUTPUT_T), expected) < 0.001,
                    "class {class}, height {height}, rotation {rotation}"
                );
            }
            assert_eq!(
                node, before,
                "pure disclosure cannot mutate the authored source"
            );
        }
    }
}
