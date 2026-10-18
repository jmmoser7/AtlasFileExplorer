//! Captured pixels retain their source silhouette while shared controls move out.
use super::*;

mod crosstalk;

fn selected_host(tag: &str, model: bool) -> (Harness, NodeId) {
    let (mut h, node) = if model {
        let mut h = Harness::new(tag);
        let (_, node) = crate::app::tests::place_linked(
            &mut h,
            "triangle.obj",
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        );
        (h, node)
    } else {
        source(tag)
    };
    h.app.patch_nodes(&[node], |node| {
        node.rect = WorldRect::new(-450.0, -250.0, 900.0, 500.0);
    });
    h.app.board_sel = [node].into_iter().collect();
    h.app.tab_mut().cam.offset = egui::Vec2::ZERO;
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    for _ in 0..12 {
        palette_frame(&mut h);
    }
    (h, node)
}

fn palette_frame(h: &mut Harness) {
    let xf = h.app.board_xf();
    let time = h.ctx.input(|i| i.time) + 1.0 / 60.0;
    let _ = h.ctx.clone().run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            time: Some(time),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                h.app.shape_properties_ui(ui, &xf);
            });
        },
    );
}

fn assert_external_capture_controls(h: &mut Harness, node: NodeId) {
    palette_frame(h);
    let host = h.app.doc().scene.node(node).unwrap();
    let bounds = h.app.board_xf().rect_w2s(host.rect);
    let capture = h
        .app
        .model_screenshot_button()
        .expect("capture stays usable");
    assert!(
        capture.top() >= bounds.bottom(),
        "capture controls must use the existing external strip: {capture:?} / {bounds:?}"
    );
    assert!(h
        .app
        .media_host_surface(&h.ctx, &h.app.board_xf(), host)
        .is_none());
    assert!(!h.app.flow_ports_visible(host, &h.ctx, &h.app.board_xf()));
}

#[test]
fn selected_model_and_portal_capture_suspend_notches_until_every_session_phase_ends() {
    for model in [false, true] {
        let (mut h, node) = selected_host("capture-chrome-lifecycle", model);
        let original = h.app.doc().scene.node(node).unwrap().clone();
        assert!(h
            .app
            .media_host_surface(&h.ctx, &h.app.board_xf(), &original)
            .is_some());

        let path = h.base.join("pending.png");
        begin_session(&mut h, node, path, true, false);
        // No further chrome pass: LastChrome still describes the old notch.
        assert!(h.app.portal_capture_owns_viewport(node));
        assert!(h
            .app
            .media_host_surface(&h.ctx, &h.app.board_xf(), &original)
            .is_none());
        assert_external_capture_controls(&mut h, node);
        for phase in [
            CapturePhase::Starting,
            CapturePhase::Recording,
            CapturePhase::Paused,
            CapturePhase::Saving,
        ] {
            h.app.portal_capture_test_status(node, phase, 2.0);
            assert!(h.app.portal_capture_owns_viewport(node), "{phase:?}");
            assert_external_capture_controls(&mut h, node);
        }
        assert_eq!(h.app.doc().scene.node(node), Some(&original));
        h.app.portal_capture_cancel();
        palette_frame(&mut h);
        assert!(!h.app.portal_capture_owns_viewport(node));
        assert!(h
            .app
            .media_host_surface(&h.ctx, &h.app.board_xf(), &original)
            .is_some());
    }
}

#[test]
fn capture_start_rejects_an_image_frame_cached_before_the_same_frame_start() {
    let (mut h, node) = selected_host("capture-stale-image-frame", true);
    let mut agent = PortalNode::unbound_agent("Image model", "comfy")
        .agent
        .unwrap();
    agent.view = atlas_ai::agent::PortalView::Images;
    h.app.patch_nodes(&[node], |node| {
        if let NodeKind::Image(image) = &mut node.kind {
            image.agent = Some(agent.clone());
        }
    });
    for _ in 0..12 {
        palette_frame(&mut h);
    }
    assert!(h.app.image_palette_frame(&h.ctx, node).is_some());
    let host = h.app.doc().scene.node(node).unwrap().clone();
    assert!(h.app.image_blister_eligible(&host));
    let path = h.base.join("pending.png");
    begin_session(&mut h, node, path, true, false);
    assert!(h.app.image_palette_frame(&h.ctx, node).is_none());
    assert!(!h.app.image_blister_eligible(&host));
    assert!(h
        .app
        .media_host_surface(&h.ctx, &h.app.board_xf(), &host)
        .is_none());
    // The cache itself was retained; ownership must reject it immediately.
    h.app.portal_capture_cancel();
    assert!(h.app.image_palette_frame(&h.ctx, node).is_some());
}

#[test]
fn capture_viewport_ownership_excludes_other_hosts_tabs_nested_boards_and_writers() {
    let (mut h, node) = source("capture-chrome-identity");
    let path = h.base.join("pending.png");
    begin_session(&mut h, node, path, true, false);
    assert!(h.app.portal_capture_owns_viewport(node));
    let other = h.app.doc_mut().scene.build_node(
        WorldRect::new(500.0, 30.0, 320.0, 160.0),
        NodeKind::Portal(PortalNode::bound_web("Other", "https://example.test/other")),
    );
    let other = h.app.add_nodes(vec![other])[0];
    assert!(!h.app.portal_capture_owns_viewport(other));
    h.app.set_slate_nesting_for_test(true);
    assert!(!h.app.portal_capture_owns_viewport(node));
    h.app.set_slate_nesting_for_test(false);
    h.app.at_home = true;
    assert!(!h.app.portal_capture_owns_viewport(node));
    h.app.at_home = false;
    let tab = h.app.active_tab;
    h.app.new_tab();
    assert!(!h.app.portal_capture_owns_viewport(node));
    h.app.active_tab = tab;
    assert!(h.app.portal_capture_owns_viewport(node));
    let source = h.app.portal_chrome.capture.session.take().unwrap().source;
    let (_sender, result) = crossbeam_channel::bounded(1);
    h.app.portal_chrome.capture.writes.push(StillWrite {
        source,
        canvas: false,
        cancelled: false,
        size: [8, 4],
        result,
        resolved: false,
        video_elapsed: Some(1.0),
    });
    assert!(!h.app.portal_capture_owns_viewport(node));
}

#[test]
fn capture_suppresses_generic_wire_hits_without_exposing_an_occluded_port() {
    use slate_doc::scene::Side;

    let (mut h, node) = selected_host("capture-generic-wire-hit", false);
    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(node).unwrap().rect;
    let world_grip = egui::Pos2::new(rect.x + rect.w, rect.y + rect.h * 0.5);
    let grip = xf.w2s(world_grip);
    assert_eq!(
        h.app.wire_grip_at(grip, &xf),
        Some((node, Side::Right, 0.5))
    );
    assert_eq!(
        h.app.wire_snap_target(world_grip, None),
        Some((node, Side::Right, 0.5))
    );
    let path = h.base.join("pending.png");
    begin_session(&mut h, node, path, true, false);
    assert!(h.app.wire_grip_at(grip, &xf).is_none());
    assert!(h.app.wire_snap_target(world_grip, None).is_none());
    h.app.portal_capture_cancel();
    assert_eq!(
        h.app.wire_grip_at(grip, &xf),
        Some((node, Side::Right, 0.5))
    );
    assert_eq!(
        h.app.wire_snap_target(world_grip, None),
        Some((node, Side::Right, 0.5))
    );

    h.app.delete_board_nodes(&[node]);
    let under = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Portal(PortalNode::bound_web("Under", "https://example.test/under")),
    );
    let over = h.app.doc_mut().scene.build_node(
        WorldRect::new(80.0, -50.0, 320.0, 200.0),
        NodeKind::Portal(PortalNode::bound_web(
            "Capture",
            "https://example.test/capture",
        )),
    );
    let ids = h.app.add_nodes(vec![under, over]);
    let (under, over) = (ids[0], ids[1]);
    h.app.board_sel.clear();
    let covered_world = egui::Pos2::new(100.0, 50.0);
    let covered_grip = xf.w2s(covered_world);
    assert!(h.app.wire_grip_at(covered_grip, &xf).is_none());
    let path = h.base.join("occluding.png");
    begin_session(&mut h, over, path, true, false);
    assert!(
        h.app.wire_grip_at(covered_grip, &xf).is_none(),
        "a capture-owned body must still occlude the underlying port"
    );
    assert!(
        h.app.wire_snap_target(covered_world, None).is_none(),
        "capture must not expose an underlying drag snap through its source body"
    );
    h.app.patch_nodes(&[over], |node| node.hidden = true);
    assert_eq!(
        h.app.wire_grip_at(covered_grip, &xf),
        Some((under, Side::Right, 0.5)),
        "control: this exact point reaches the underlying port without its occluder"
    );
    assert_eq!(
        h.app.wire_snap_target(covered_world, None),
        Some((under, Side::Right, 0.5))
    );
    h.app.patch_nodes(&[over], |node| node.hidden = false);
    h.app.portal_capture_cancel();
    assert!(h.app.wire_grip_at(covered_grip, &xf).is_none());
}

#[test]
fn capture_suppresses_generator_body_default_snap_and_restores_it_afterward() {
    use slate_doc::scene::Side;

    let (mut h, node) = selected_host("capture-flow-body-snap", false);
    let source = crate::app::board_flow::tests::place(&mut h, "brief.txt", -1100.0);
    let mut agent = PortalNode::unbound_agent("Legacy image portal", "comfy")
        .agent
        .unwrap();
    agent.view = atlas_ai::agent::PortalView::Images;
    h.app.patch_nodes(&[node], |node| {
        if let NodeKind::Portal(portal) = &mut node.kind {
            portal.agent = Some(agent.clone());
        }
    });
    let host = h.app.doc().scene.node(node).unwrap();
    assert!(h.app.wire_input_host(host).is_flow());
    let body = egui::Pos2::new(
        host.rect.x + host.rect.w * 0.5,
        host.rect.y + host.rect.h * 0.5,
    );
    let from = Some((source, Side::Right, 0.5));
    let expected = Some((node, Side::Left, 0.5));
    assert_eq!(h.app.wire_snap_target(body, from), expected);
    let path = h.base.join("body.png");
    begin_session(&mut h, node, path, true, false);
    assert!(h.app.wire_snap_target(body, from).is_none());
    h.app.portal_capture_cancel();
    assert_eq!(h.app.wire_snap_target(body, from), expected);
}

fn wire_grip_frame(h: &mut Harness) -> egui::FullOutput {
    let xf = h.app.board_xf();
    let time = h.ctx.input(|i| i.time) + 0.03;
    h.ctx.clone().run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            time: Some(time),
            ..Default::default()
        },
        |ctx| {
            h.app
                .paint_wire_grips(&ctx.layer_painter(egui::LayerId::background()), &xf);
        },
    )
}

fn painted_grip_count(output: &egui::FullOutput) -> usize {
    output
        .shapes
        .iter()
        .filter(|shape| matches!(shape.shape, egui::Shape::Circle(_)))
        .count()
}

#[test]
fn capture_suppresses_live_and_cached_fading_wire_dots_and_grab_cursor() {
    let (mut h, node) = selected_host("capture-cached-wire-dot", false);
    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(node).unwrap().rect;
    let grip = xf.w2s(egui::Pos2::new(rect.x + rect.w, rect.y + rect.h * 0.5));
    h.app.update_wire_grips(Some(grip), &xf);
    for _ in 0..8 {
        wire_grip_frame(&mut h);
    }
    let visible = wire_grip_frame(&mut h);
    assert_eq!(painted_grip_count(&visible), 1);
    assert_eq!(visible.platform_output.cursor_icon, egui::CursorIcon::Grab);

    let path = h.base.join("pending.png");
    begin_session(&mut h, node, path, true, false);
    // Start occurred after hover calculation: the current hover is stale too.
    let stale_current = wire_grip_frame(&mut h);
    assert_eq!(painted_grip_count(&stale_current), 0);
    assert_ne!(
        stale_current.platform_output.cursor_icon,
        egui::CursorIcon::Grab
    );
    h.app.update_wire_grips(None, &xf);
    let cached_fade = wire_grip_frame(&mut h);
    assert_eq!(painted_grip_count(&cached_fade), 0);
    assert_ne!(
        cached_fade.platform_output.cursor_icon,
        egui::CursorIcon::Grab
    );

    h.app.portal_capture_cancel();
    h.app.update_wire_grips(Some(grip), &xf);
    let restored = wire_grip_frame(&mut h);
    assert_eq!(painted_grip_count(&restored), 1);
    assert_eq!(restored.platform_output.cursor_icon, egui::CursorIcon::Grab);
}
