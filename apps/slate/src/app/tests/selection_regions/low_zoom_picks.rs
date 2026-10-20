//! Pixel-based stroke, connector and draft feedback below the former zoom floor.
use super::*;

#[test]
fn an_open_stroke_keeps_its_four_pixel_pick_margin_below_the_old_zoom_minimum() {
    let mut h = grip_board("regions_low_zoom_stroke_pick");
    let id = add_stroke(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.frame();
    let focus = h.app.canvas_rect.center();
    for zoom in [0.05, 0.02, 1.0 / 128.0] {
        h.app.zoom_at(focus, zoom / h.app.tab().cam.z);
        assert!((h.app.tab().cam.z - zoom).abs() < 0.000001);
        let xf = h.app.board_xf();
        let center = xf.w2s(Pos2::new(50.0, 0.5));
        for sign in [-1.0, 1.0] {
            let inside = xf.s2w(center + EVec2::new(0.0, sign * 3.5));
            let outside = xf.s2w(center + EVec2::new(0.0, sign * 5.0));
            assert_eq!(
                h.app.board_pick_node(inside.x, inside.y),
                Some(id),
                "stroke's screen pick margin shrank at zoom {zoom}"
            );
            assert_eq!(
                h.app.board_pick_node(outside.x, outside.y),
                None,
                "stroke pick expanded beyond its screen margin at zoom {zoom}"
            );
        }
    }
}

#[test]
fn connector_screen_pick_and_body_exclusion_margins_survive_the_new_overview_zoom() {
    let mut h = grip_board("regions_low_zoom_connector_pick");
    h.app.board_wire_routing = slate_doc::WireRouting::Bezier;
    let wire = h.app.build_connector(
        slate_doc::scene::ConnectorEnd::Free {
            point: [-12_800.0, 0.0],
        },
        slate_doc::scene::ConnectorEnd::Free {
            point: [12_800.0, 0.0],
        },
    );
    let id = h.app.add_nodes(vec![wire])[0];
    h.app.board_sel.clear();
    h.frame();
    let focus = h.app.canvas_rect.center();
    for zoom in [0.05, 0.02, 1.0 / 128.0] {
        h.app.zoom_at(focus, zoom / h.app.tab().cam.z);
        let xf = h.app.board_xf();
        let center = xf.w2s(Pos2::ZERO);
        for sign in [-1.0, 1.0] {
            let inside = xf.s2w(center + EVec2::new(0.0, sign * 7.0));
            let outside = xf.s2w(center + EVec2::new(0.0, sign * 10.0));
            assert_eq!(
                h.app.board_pick_node(inside.x, inside.y),
                Some(id),
                "connector's screen pick margin shrank at zoom {zoom}"
            );
            assert_eq!(h.app.board_pick_node(outside.x, outside.y), None);
        }
    }
    let z = h.app.tab().cam.z;
    assert_eq!(z, 1.0 / 128.0);
    let blocker = card(
        &mut h,
        WorldRect::new(-5.0 / z, 6.0 / z, 10.0 / z, 10.0 / z),
    );
    assert_eq!(
        h.app.board_pick_node(0.0, 0.0),
        None,
        "a visible body six screen pixels away must exclude the wire underneath its pick margin"
    );
    h.app.patch_nodes(&[blocker], |node| node.rect.y = 10.0 / z);
    assert_eq!(
        h.app.board_pick_node(0.0, 0.0),
        Some(id),
        "moving the body beyond the wire's screen exclusion margin restores that wire pick"
    );
}

#[test]
fn live_stroke_feedback_keeps_screen_width_and_antialias_feather_at_overview_zoom() {
    let mut h = grip_board("regions_low_zoom_draft_feedback");
    let mut bez = vector_ink::kurbo::BezPath::new();
    bez.move_to((0.0, 0.0));
    bez.line_to((12_800.0, 0.0));
    let tips = [slate_doc::vertex_style::PlacedTip {
        width: 0.01,
        color: Rgba::BLACK,
        opacity: 1.0,
    }; 2];
    let focus = h.app.canvas_rect.center();
    let mut reference = None::<f32>;
    for zoom in [0.05, 0.02, 1.0 / 128.0] {
        h.app.zoom_at(focus, zoom / h.app.tab().cam.z);
        let z = h.app.tab().cam.z;
        let (ink, _) = board_path::draft_stroke_ink(&bez, false, &tips, z);
        let low = ink
            .vertices
            .iter()
            .map(|v| v.pos[1])
            .fold(f32::INFINITY, f32::min);
        let high = ink
            .vertices
            .iter()
            .map(|v| v.pos[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let screen_span = (high - low) * z;
        assert!(screen_span > 1.0 && screen_span < 6.0);
        if let Some(previous) = reference {
            assert!(
                (screen_span - previous).abs() < 0.01,
                "live stroke feedback/AA narrowed at zoom {z}: {screen_span} vs {previous}"
            );
        } else {
            reference = Some(screen_span);
        }
    }
    assert!(
        tips.iter().all(|tip| tip.width == 0.01),
        "screen feedback must not mutate authored tip widths"
    );
}

#[test]
fn smart_guide_reach_and_lane_remain_screen_invariant_below_the_old_zoom_floor() {
    let mut h = grip_board("regions_low_zoom_snap_scope");
    h.app.board_snap_reach = crate::app::settings::SnapReach::Nearby;
    let (reach_px, lane_px) = h.app.board_snap_reach.screen_px();
    assert!(reach_px.is_finite() && lane_px.is_finite());
    let focus = h.app.canvas_rect.center();
    for zoom in [0.05, 0.025, 1.0 / 128.0] {
        h.app.zoom_at(focus, zoom / h.app.tab().cam.z);
        let z = h.app.tab().cam.z;
        assert!((z - zoom).abs() < 0.000001);
        let scope = h.app.snap_scope();
        assert!((scope.reach * z - reach_px).abs() < 0.001);
        assert!((scope.lane * z - lane_px).abs() < 0.001);
        assert!((scope.threshold * z - crate::app::board_snap::SNAP_SCREEN_PX).abs() < 0.001);
        assert!((h.app.osnap_radius_world() * z - 12.0).abs() < 0.001);
        assert!(
            (h.app.board_snap_threshold_pub() * z - crate::app::board_snap::SNAP_SCREEN_PX).abs()
                < 0.001
        );
        assert!((scope.view.w * z - h.app.canvas_rect.width()).abs() < 0.001);
        assert!((scope.view.h * z - h.app.canvas_rect.height()).abs() < 0.001);
    }
}

#[test]
fn a_live_wire_keeps_its_fourteen_pixel_snap_band_at_overview_zoom() {
    let mut h = grip_board("regions_low_zoom_wire_snap_band");
    let id = card(&mut h, WorldRect::new(0.0, 0.0, 16_384.0, 16_384.0));
    h.app.board_sel.clear();
    let port = h
        .app
        .wire_input_host(h.app.doc().scene.node(id).unwrap())
        .ports()
        .into_iter()
        .find(|p| p.side == Side::Right)
        .unwrap();
    h.app.tab_mut().cam.offset = EVec2::from(port.point);
    h.frame();
    let focus = h.app.canvas_rect.center();
    for zoom in [0.05, 0.025, 1.0 / 128.0] {
        h.app.zoom_at(focus, zoom / h.app.tab().cam.z);
        let xf = h.app.board_xf();
        let host = h.app.wire_input_host(h.app.doc().scene.node(id).unwrap());
        let port = host
            .ports()
            .into_iter()
            .find(|p| p.side == Side::Right)
            .unwrap();
        let anchor = xf.w2s(Pos2::from(port.point));
        let outward = EVec2::from(host.outward(port.side, port.t));
        let inside = xf.s2w(anchor + outward * 13.0);
        let outside = xf.s2w(anchor + outward * 15.5);
        assert!(h.app.canvas_rect.contains(anchor + outward * 13.0));
        assert_eq!(
            h.app.wire_snap_target(inside, None).map(|p| (p.0, p.1)),
            Some((id, Side::Right)),
            "wire snap band shrank at zoom {zoom}"
        );
        assert!(
            h.app.wire_snap_target(outside, None).is_none(),
            "wire snap extended beyond the fourteen-pixel band at zoom {zoom}"
        );
    }
}
