use super::*;
use crate::app::board_wire::WireMode;
use crate::app::tests::Harness;
use slate_doc::agent_inputs::OUTPUT_T;

#[test]
fn ordinary_model_source_output_matches_real_grip_rubberband_committed_route_and_saved_host() {
    for empty_ref in [false, true] {
        let mut h = Harness::new("model-source-output-owner");
        h.app.leave_home();
        h.app.ensure_work_tab();
        h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
        let file = h.base.join("owned.obj");
        std::fs::write(&file, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let items = h.app.add_paths(&[file]);
        h.app.place_items_on_board(&items, Pos2::ZERO);
        let source = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.place_items_on_board(&items, Pos2::new(1300.0, 100.0));
        let target = h.app.doc().scene.nodes.last().unwrap().id;
        h.app.board_wire_routing = slate_doc::wire::WireRouting::Bezier;
        assert!(slate_doc::agent_chat::agent(h.app.doc().scene.node(source).unwrap()).is_none());
        if empty_ref {
            let command = h.app.prepare_input_graft(source, "style", true).unwrap();
            assert!(h.app.commit_scene(vec![command]));
        }
        for height in [100.0, 390.0] {
            for rotation in [0.0, 37.0, 90.0] {
                h.app.patch_nodes(&[source], |node| {
                    node.rect = WorldRect::new(100.0, 100.0, 420.0, height);
                    node.rotation_deg = rotation;
                });
                h.app.patch_nodes(&[target], |node| {
                    node.rect = WorldRect::new(1300.0, 100.0, 420.0, height);
                    node.rotation_deg = rotation;
                });
                crate::app::tests::frame_on(&mut h, source, 1.0);
                h.app.board_sel = [source].into_iter().collect();
                let xf = h.app.board_xf();
                let before = h.app.doc().scene.nodes.clone();
                let simple_output = h
                    .app
                    .wire_input_host(h.app.doc().scene.node(source).unwrap())
                    .anchor(Side::Right, OUTPUT_T);
                let native_source = h.app.doc().scene.node(source).unwrap();
                assert_eq!(
                    simple_output,
                    native_source.rect.rotate_point(
                        [
                            native_source.rect.x + native_source.rect.w,
                            native_source.rect.y + native_source.rect.h * 0.5
                        ],
                        native_source.rotation_deg,
                    )
                );
                assert!(h
                    .app
                    .open_object_panel(crate::app::board_properties::Panel::Agent));
                let node = h.app.doc().scene.node(source).unwrap();
                let anchor = h.app.wire_input_host(node).anchor(Side::Right, OUTPUT_T);
                assert_eq!(
                    h.app
                        .visible_wire_host(source)
                        .unwrap()
                        .anchor(Side::Right, OUTPUT_T),
                    anchor
                );
                assert_eq!(
                    h.app.wire_port_point(node, Side::Right, OUTPUT_T),
                    Pos2::from(anchor)
                );
                let screen = xf.w2s(Pos2::from(anchor));
                assert_eq!(
                    h.app.wire_grip_at(screen, &xf),
                    Some((source, Side::Right, OUTPUT_T))
                );
                let mut drag = h
                    .app
                    .try_begin_wire_drag(screen, Pos2::from(anchor), Default::default())
                    .expect("actual Model output grip begins a wire");
                assert!(
                    matches!(drag.mode, WireMode::Add { from } if from == (source, Side::Right, OUTPUT_T))
                );
                h.app.board_sel = [target].into_iter().collect();
                assert!(h
                    .app
                    .open_object_panel(crate::app::board_properties::Panel::Agent));
                let target_node = h.app.doc().scene.node(target).unwrap();
                let target_t = h.app.wire_input_ports(target_node)[0].t;
                let target_anchor = h
                    .app
                    .wire_input_host(target_node)
                    .anchor(Side::Left, target_t);
                h.app
                    .wire_drag_update(&mut drag, Pos2::from(target_anchor), false);
                assert_eq!(
                    drag.snap,
                    Some((target, Side::Left, target_t)),
                    "real target snap uses its typed Model datum"
                );
                h.app.board_sel = [source].into_iter().collect();
                assert!(h
                    .app
                    .open_object_panel(crate::app::board_properties::Panel::Agent));
                assert_eq!(
                    h.app
                        .wire_input_host(h.app.doc().scene.node(source).unwrap())
                        .anchor(Side::Right, OUTPUT_T),
                    simple_output,
                    "Agent expansion keeps the native source output fixed"
                );
                let output = h.ctx.run(Default::default(), |ctx| {
                    let painter = ctx.layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::new("actual-model-output-preview"),
                    ));
                    h.app.paint_flow_ports(&painter, &xf);
                    h.app
                        .paint_wire_drag(&painter, &xf, &drag, Default::default());
                });
                assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Circle(circle) if circle.center.distance(screen) < 0.001)), "actual painted output dot uses the same site");
                let start = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::CubicBezier(curve) => Some(curve.points[0]),
                        _ => None,
                    })
                    .expect("actual rubberband bezier paints");
                let drawn = slate_doc::scene::connector_drawn_stroke(h.app.default_wire_stroke());
                let outward = h
                    .app
                    .visible_wire_host(source)
                    .unwrap()
                    .outward(Side::Right, OUTPUT_T);
                let expected_drawn = xf.w2s(
                    Pos2::from(anchor) + egui::vec2(outward[0], outward[1]) * (drawn.width * 0.5),
                );
                assert!(
                    start.distance(expected_drawn) < 0.001,
                    "rubberband starts outside the displayed Model output, not at native midpoint"
                );
                h.app.finish_wire_drag(drag);
                let wire = h.app.doc().scene.nodes.last().unwrap();
                let NodeKind::Connector(connector) = &wire.kind else {
                    panic!("real release commits a wire")
                };
                let path = h.app.connector_path_visible(wire.id, connector).unwrap();
                assert_eq!(path.start(), anchor);
                assert_eq!(path.end(), target_anchor);
                assert_eq!(h.app.wire_end_point(&connector.a), Some(Pos2::from(anchor)));
                let stored = serde_json::to_vec(h.app.doc()).unwrap();
                let restored: slate_doc::SlateDoc = serde_json::from_slice(&stored).unwrap();
                let restored_node = restored.scene.node(source).unwrap();
                let restored_host =
                    slate_doc::WireHost::from_node_with_paths(restored_node, &|item| {
                        restored.item(item).map(|file| file.path.as_path())
                    });
                assert_eq!(restored_host.anchor(Side::Right, OUTPUT_T), anchor);
                assert!(h.app.agents.dispatched.is_empty());
                h.app.board_undo();
                assert_eq!(
                    h.app.doc().scene.nodes,
                    before,
                    "one Undo restores source metadata/geometry and removes the actual wire"
                );
            }
        }
    }
}
