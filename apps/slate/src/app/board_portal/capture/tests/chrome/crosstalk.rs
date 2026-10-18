//! Capture ownership must not change the meaning of a held crosstalk drag.
use super::*;
use slate_doc::scene::Side;

fn coding_pair() -> (Harness, NodeId, NodeId) {
    let (mut h, placeholder) = source("capture-crosstalk-ports");
    h.app.delete_board_nodes(&[placeholder]);
    let mut nodes = Vec::new();
    for (index, provider) in ["cursor", "codex"].into_iter().enumerate() {
        let mut portal = PortalNode::unbound_agent(provider, provider);
        let agent = portal.agent.as_mut().unwrap();
        agent.channel = Some(format!("capture-conversation-{index}"));
        agent.chat.linear = true;
        agent.chat.train = true;
        nodes.push(h.app.doc_mut().scene.build_node(
            WorldRect::new(0.0, index as f32 * 400.0, 320.0, 200.0),
            NodeKind::Portal(portal),
        ));
    }
    let ids = h.app.add_nodes(nodes);
    h.app.board_sel.clear();
    h.app.tab_mut().cam.z = 1.0;
    (h, ids[0], ids[1])
}

fn port(h: &Harness, node: NodeId, side: Side) -> egui::Pos2 {
    let point = slate_doc::connector_anchor_on(h.app.doc().scene.node(node).unwrap(), side, 0.5);
    egui::Pos2::new(point[0], point[1])
}

fn port_paint(h: &mut Harness, pointer: egui::Pos2) -> egui::FullOutput {
    let xf = h.app.board_xf();
    let time = h.ctx.input(|i| i.time) + 0.03;
    h.ctx.clone().run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            events: vec![egui::Event::PointerMoved(pointer)],
            time: Some(time),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                h.app.paint_crosstalk(ui, &ui.painter().clone(), &xf);
            });
        },
    )
}

fn dots_at(output: &egui::FullOutput, at: egui::Pos2) -> usize {
    output
        .shapes
        .iter()
        .filter(|shape| {
            matches!(&shape.shape, egui::Shape::Circle(circle) if circle.center.distance(at) < 0.1)
        })
        .count()
}

#[test]
fn capture_suppresses_hover_drag_and_wired_crosstalk_port_paint_then_restores_it() {
    for mode in ["hover", "drag", "wired"] {
        let (mut h, source, target) = coding_pair();
        let at = h.app.board_xf().w2s(port(&h, source, Side::Bottom));
        if mode == "drag" {
            h.app.board_drag = Some(crate::app::board::BoardDrag::Wire(
                crate::app::board_wire::WireDrag {
                    mode: crate::app::board_wire::WireMode::Add {
                        from: (source, Side::Bottom, 0.5),
                    },
                    cursor: port(&h, target, Side::Top),
                    snap: None,
                },
            ));
        } else if mode == "wired" {
            let binding = slate_doc::crosstalk::Crosstalk::owner(
                "capture-paint".into(),
                "source",
                "partner",
                slate_doc::crosstalk::Role::Builds,
            );
            let connector =
                slate_doc::crosstalk::connector(&h.app.doc().scene, source, target, binding)
                    .unwrap();
            let node = h.app.doc_mut().scene.build_node(
                WorldRect::new(0.0, 0.0, 1.0, 1.0),
                NodeKind::Connector(connector),
            );
            h.app.add_nodes(vec![node]);
        }
        let pointer = if mode == "hover" {
            at
        } else {
            egui::Pos2::new(-1000.0, -1000.0)
        };
        assert_eq!(dots_at(&port_paint(&mut h, pointer), at), 1, "{mode}");
        let path = h.base.join(format!("{mode}-port.png"));
        begin_session(&mut h, source, path, true, false);
        assert_eq!(dots_at(&port_paint(&mut h, pointer), at), 0, "{mode}");
        h.app.portal_capture_cancel();
        assert_eq!(dots_at(&port_paint(&mut h, pointer), at), 1, "{mode}");
    }
}

#[test]
fn captured_coding_source_and_target_suppress_ports_and_snap_without_reclassifying_a_held_drag() {
    let (mut h, source, target) = coding_pair();
    let xf = h.app.board_xf();
    let source_port = port(&h, source, Side::Bottom);
    let target_port = port(&h, target, Side::Top);
    let target_body = egui::Pos2::new(160.0, 500.0);
    let from = (source, Side::Bottom, 0.5);
    let expected = Some((target, Side::Top, 0.5));
    assert!(h.app.is_crosstalk_port(from));
    assert_eq!(h.app.wire_snap_target(target_port, Some(from)), expected);
    assert_eq!(h.app.wire_snap_target(target_body, Some(from)), expected);
    for (node, side, point) in [
        (source, Side::Bottom, source_port),
        (target, Side::Top, target_port),
    ] {
        let screen = xf.w2s(point);
        assert_eq!(
            h.app
                .crosstalk_port_at(h.app.doc().scene.node(node).unwrap(), screen, &xf),
            Some(side)
        );
        assert_eq!(h.app.crosstalk_port_under(screen, &xf), Some((node, side)));
        let path = h.base.join(format!("coding-{}.png", node.0));
        begin_session(&mut h, node, path, true, false);
        assert!(
            h.app.is_crosstalk_port(from),
            "the held drag keeps its semantic kind"
        );
        assert!(h
            .app
            .crosstalk_port_at(h.app.doc().scene.node(node).unwrap(), screen, &xf)
            .is_none());
        assert!(h.app.crosstalk_port_under(screen, &xf).is_none());
        for point in [target_port, target_body] {
            assert!(h.app.crosstalk_snap(point, source, 14.0).is_none());
            assert!(h.app.wire_snap_target(point, Some(from)).is_none());
        }
        h.app.portal_capture_cancel();
        assert!(h.app.is_crosstalk_port(from));
        assert_eq!(h.app.crosstalk_port_under(screen, &xf), Some((node, side)));
        assert_eq!(h.app.wire_snap_target(target_port, Some(from)), expected);
        assert_eq!(h.app.wire_snap_target(target_body, Some(from)), expected);
    }
}

#[test]
fn a_foreground_noncoding_capture_body_occludes_underlying_crosstalk_port_and_body_snap() {
    let (mut h, source, target) = coding_pair();
    let xf = h.app.board_xf();
    let target_port = port(&h, target, Side::Top);
    let target_body = egui::Pos2::new(160.0, 500.0);
    let from = (source, Side::Bottom, 0.5);
    let expected = Some((target, Side::Top, 0.5));
    let overlay = h.app.doc_mut().scene.build_node(
        WorldRect::new(140.0, target_port.y - 20.0, 100.0, 160.0),
        NodeKind::Portal(PortalNode::bound_web(
            "Capture",
            "https://example.test/capture",
        )),
    );
    let overlay = h.app.add_nodes(vec![overlay])[0];
    let screen = xf.w2s(target_port);
    assert_eq!(
        h.app.crosstalk_port_under(screen, &xf),
        Some((target, Side::Top))
    );
    assert_eq!(h.app.wire_snap_target(target_body, Some(from)), expected);
    let path = h.base.join("noncoding-occluder.png");
    begin_session(&mut h, overlay, path, true, false);
    assert!(h.app.crosstalk_port_under(screen, &xf).is_none());
    for point in [target_port, target_body] {
        assert!(h.app.crosstalk_snap(point, source, 14.0).is_none());
        assert!(h.app.wire_snap_target(point, Some(from)).is_none());
    }
    h.app.patch_nodes(&[overlay], |node| node.hidden = true);
    assert_eq!(
        h.app.crosstalk_port_under(screen, &xf),
        Some((target, Side::Top))
    );
    assert_eq!(h.app.wire_snap_target(target_port, Some(from)), expected);
    assert_eq!(h.app.wire_snap_target(target_body, Some(from)), expected);
    h.app.patch_nodes(&[overlay], |node| node.hidden = false);
    assert!(h.app.crosstalk_port_under(screen, &xf).is_none());
    h.app.portal_capture_cancel();
    assert_eq!(
        h.app.crosstalk_port_under(screen, &xf),
        Some((target, Side::Top))
    );
    assert_eq!(h.app.wire_snap_target(target_body, Some(from)), expected);
}
