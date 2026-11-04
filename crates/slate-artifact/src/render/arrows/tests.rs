use super::*;
use slate_doc::agent_inputs::{GENERATOR_PORTS, OUTPUT_T};
use slate_doc::scene::{AgentPortalRef, ConnectorEnd, FrameNode, ImageNode, SceneCmd, Stroke};

fn style_number(html: &str, name: &str) -> f32 {
    html.split(format!("{name}:").as_str())
        .nth(1)
        .unwrap()
        .split("px")
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn actual_svg_model_source_and_target_keep_path_resolved_endpoints_after_serialization() {
    let dir = std::env::temp_dir().join(format!(
        "slate-artifact-model-output-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let file = dir.join("owned.obj");
    std::fs::write(&file, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let saved = dir.join("geometry.slate");
    for empty_ref in [false, true] {
        for height in [100.0, 390.0] {
            for rotation in [0.0, 37.0, 90.0] {
                let mut doc = SlateDoc::new("Model output geometry");
                let item = doc.add_item(file.clone(), "owned.obj", 32, 0, "owned-model");
                let frame = doc.scene.build_node(
                    WorldRect::new(0.0, 0.0, 4000.0, 2000.0),
                    NodeKind::Frame(FrameNode {
                        presentation_media: Default::default(),
                        title: "Frame".into(),
                        order: 0,
                        fill: Rgba::WHITE,
                        fill_authored: false,
                        assignments: Default::default(),
                        stroke: Stroke::none(),
                        corner: Corner::Square,
                    }),
                );
                doc.scene.nodes.push(frame);
                let mut ids = Vec::new();
                for x in [100.0, 1300.0] {
                    let mut image = ImageNode::new(item);
                    if empty_ref {
                        image.agent = Some(Box::new(AgentPortalRef::unbound(Default::default())));
                    }
                    let mut node = doc.scene.build_node(
                        WorldRect::new(x, 100.0, 420.0, height),
                        NodeKind::Image(image),
                    );
                    node.rotation_deg = rotation;
                    ids.push(node.id);
                    doc.scene.nodes.push(node);
                }
                let connector = ConnectorNode {
                    a: ConnectorEnd::Anchored {
                        node: ids[0],
                        side: slate_doc::scene::Side::Right,
                        t: OUTPUT_T,
                    },
                    b: ConnectorEnd::Anchored {
                        node: ids[1],
                        side: slate_doc::scene::Side::Left,
                        t: GENERATOR_PORTS[0].t,
                    },
                    stroke: Stroke {
                        width: 2.0,
                        ..Stroke::none()
                    },
                    color: None,
                    routing: Some(WireRouting::Bezier),
                    arrow_a: false,
                    arrow_b: false,
                    label: None,
                    display: Default::default(),
                    crosstalk: None,
                    binding: None,
                };
                let wire = doc.scene.build_node(
                    WorldRect::new(0.0, 0.0, 1.0, 1.0),
                    NodeKind::Connector(connector),
                );
                let wire_id = wire.id;
                let index = doc.scene.nodes.len();
                doc.scene.apply(&SceneCmd::Add { index, node: wire });
                let before = doc.scene.clone();
                doc.save_to(&saved).unwrap();
                let restored = SlateDoc::load_from(&saved).unwrap();
                let wire = restored.scene.node(wire_id).unwrap();
                let NodeKind::Connector(connector) = &wire.kind else {
                    unreachable!()
                };
                let mut html = String::new();
                let options = crate::ExportOptions::default();
                render_connector(
                    &mut html,
                    &restored,
                    wire,
                    connector,
                    0.0,
                    0.0,
                    options.wire_routing,
                    options.wire_theme,
                );
                let origin = [style_number(&html, "left"), style_number(&html, "top")];
                let d = html
                    .split(" d=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap();
                let numbers: Vec<f32> = d
                    .split_whitespace()
                    .filter_map(|value| value.parse().ok())
                    .collect();
                assert_eq!(numbers.len(), 8, "actual cubic SVG path: {d}");
                let drawn = connector_drawn_stroke(connector.stroke);
                let scale = (height
                    / (slate_doc::wire_host::FLOW_PORT_TOP
                        + 2.0 * slate_doc::wire_host::FLOW_PORT_PITCH
                        + 14.0))
                    .min(1.0);
                for (id, side, offset) in [(ids[0], 1.0, 0), (ids[1], -1.0, 6)] {
                    let node = restored.scene.node(id).unwrap();
                    let endpoint = node.rect.rotate_point(
                        [
                            if side > 0.0 {
                                node.rect.x + node.rect.w
                            } else {
                                node.rect.x
                            },
                            // Ordinary linked models keep their authored midpoint output;
                            // only the receiving typed input uses the fitted graph rail.
                            node.rect.y
                                + if side > 0.0 {
                                    node.rect.h * 0.5
                                } else {
                                    slate_doc::wire_host::FLOW_PORT_TOP * scale
                                },
                        ],
                        rotation,
                    );
                    let angle = rotation.to_radians();
                    let expected = [
                        endpoint[0] + side * angle.cos() * drawn.width * 0.5,
                        endpoint[1] + side * angle.sin() * drawn.width * 0.5,
                    ];
                    let actual = [origin[0] + numbers[offset], origin[1] + numbers[offset + 1]];
                    // CSS wrapper and SVG each serialize to one decimal place.
                    assert!(
                        (actual[0] - expected[0]).abs() <= 0.11
                            && (actual[1] - expected[1]).abs() <= 0.11,
                        "source/target {id:?}, h{height}, r{rotation}: {actual:?} vs {expected:?}"
                    );
                }
                assert!(
                    render_html(&restored, &AssetMap::default()).contains(&html),
                    "real document renderer passes current document paths to the same SVG producer"
                );
                assert_eq!(doc.scene, before, "render/export never mutates the source");
            }
        }
    }
    std::fs::remove_file(saved).unwrap();
    std::fs::remove_file(file).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
