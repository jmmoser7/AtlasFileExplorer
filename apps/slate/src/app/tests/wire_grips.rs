//! Wire grip, port hit, and edge-scale tests.

use super::*;

/// Wire grips preview only at the handle under the pointer — not the whole edge.
#[test]
fn wire_grips_preview_only_the_handle_under_the_pointer() {
    let mut h = web_board("wire_grip_prox");
    add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.nodes[0].clone();
    // Midway along the top edge, well away from the side-midpoint grip.
    let between = xf.w2s(Pos2::new(n.rect.x + 12.0, n.rect.y));
    h.app.update_wire_grips(Some(between), &xf);
    assert!(
        h.app.wire_grips.is_none(),
        "an edge between grips must not preview a wire handle"
    );

    let grip = xf.w2s(board_wire::grip_point(n.rect, slate_doc::scene::Side::Top));
    h.app.update_wire_grips(Some(grip), &xf);
    assert_eq!(
        h.app.wire_grips.map(|g| (g.node, g.hovered)),
        Some((n.id, Some(slate_doc::scene::Side::Top))),
        "only the grip under the pointer previews"
    );
}

/// The enlarged grip hit reaches outside the node and stops at the edge.
#[test]
fn wire_grip_hit_reaches_outside_the_node_only() {
    let mut h = web_board("wire_grip_outward");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, slate_doc::scene::Side::Top));
    let outside = grip + EVec2::new(0.0, -30.0);
    let inside = grip + EVec2::new(0.0, 30.0);

    assert_eq!(
        h.app.wire_grip_at(outside, &xf).map(|(_, side, _)| side),
        Some(slate_doc::scene::Side::Top),
        "30 px outside the edge is a grip"
    );
    assert!(
        h.app.wire_grip_at(inside, &xf).is_none(),
        "the same distance inside the node is not a grip"
    );

    let mods = egui::Modifiers::default();
    let drag = h.app.begin_gesture_for_test(outside, xf.s2w(outside), mods);
    assert!(
        matches!(drag, Some(board::BoardDrag::Wire(_))),
        "a press in the outward hit starts a wire"
    );
}

/// A press on the displayed wire handle starts a wire, even when that
/// point is also on the resize band and the live hover cache has cleared
/// (egui's drag threshold often leaves the 8 px dot before drag_started).
#[test]
fn wire_grip_press_beats_edge_resize() {
    let mut h = web_board("wire_grip_beats_resize");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, slate_doc::scene::Side::Top));
    let world = xf.s2w(grip);

    // Simulate the live pointer having already left the dot.
    h.app.wire_grips = None;
    let mods = egui::Modifiers::default();
    let drag = h.app.begin_gesture_for_test(grip, world, mods);
    assert!(
        matches!(drag, Some(board::BoardDrag::Wire(_))),
        "press on the wire handle must start a wire"
    );
    assert!(
        h.app.begin_transform_drag(grip, world).is_none(),
        "resize must stay suppressed on the wire handle"
    );

    let edge = xf.w2s(Pos2::new(rect.x + 12.0, rect.y));
    let edge_world = xf.s2w(edge);
    let edge_drag = h.app.begin_gesture_for_test(edge, edge_world, mods);
    assert!(
        matches!(edge_drag, Some(board::BoardDrag::Resize { .. })),
        "the rest of the edge must still resize"
    );
}

/// The inner hit grows past 8 px once the camera pulls back, and stays 8 px
/// when zoomed in. Painted discs still use canvas scale.
#[test]
fn wire_grip_hit_grows_when_zoomed_out() {
    let mut h = web_board("wire_hit_zoom");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();

    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip_w = board_wire::grip_point(rect, slate_doc::scene::Side::Top);
    let xf = h.app.board_xf();
    let inside = xf.w2s(grip_w) + EVec2::new(0.0, 9.0);
    assert!(
        h.app.wire_grip_at(inside, &xf).is_none(),
        "9 px inside the edge is past the zoom-1 disk"
    );

    h.app.tab_mut().cam.z = 0.55;
    let xf = h.app.board_xf();
    let inside = xf.w2s(grip_w) + EVec2::new(0.0, 9.0);
    assert_eq!(
        h.app.wire_grip_at(inside, &xf).map(|(_, side, _)| side),
        Some(slate_doc::scene::Side::Top),
        "the same 9 px reaches the grown hit"
    );
}

/// A port the painter has dropped is not a press target.
#[test]
fn wire_ports_below_the_lod_are_not_hittable() {
    let mut h = web_board("wire_port_lod");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 0.2;
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, slate_doc::scene::Side::Right));
    assert!(
        h.app.wire_grip_at(grip, &xf).is_none(),
        "a disc below 1.5 px is not a grip"
    );
}

/// A neighbor's enlarged port must not steal a press that landed in this body.
#[test]
fn a_press_inside_a_node_does_not_start_a_neighbors_wire() {
    let mut h = web_board("wire_neighbor_body");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let _b = add_rect(&mut h.app, 84.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(a).unwrap().rect;
    let world = Pos2::new(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    let screen = xf.w2s(world);
    assert!(
        h.app.wire_grip_at(screen, &xf).is_none(),
        "the neighbor's outward hit stops at this body"
    );
    let drag = h
        .app
        .begin_gesture_for_test(screen, world, egui::Modifiers::NONE);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "the press moves the node"
    );
}

/// A slide frame around a card holds both the press and the port, so it
/// does not count as another body: the card's port still starts a wire.
#[test]
fn a_port_inside_a_slide_frame_still_starts_a_wire() {
    let mut h = web_board("wire_port_in_frame");
    h.seed_frame(None);
    let id = add_rect(&mut h.app, 200.0, 200.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = board_wire::grip_point(rect, slate_doc::scene::Side::Right);
    let press = xf.w2s(grip) + EVec2::new(12.0, 0.0);
    assert_eq!(
        h.app.wire_grip_at(press, &xf),
        Some((id, slate_doc::scene::Side::Right, 0.5)),
        "the outward hit over the frame still reaches the card's port"
    );
}

/// Zoomed out, a packed multi-selection drags as a group. Overlapping port
/// hits must not start a wire. Images share the area-port hit with these cards.
#[test]
fn zoomed_out_packed_selection_moves_instead_of_starting_a_wire() {
    let mut h = web_board("wire_packed_sel");
    let mut ids = Vec::new();
    for row in 0..3 {
        for col in 0..3 {
            ids.push(add_image_card(
                &mut h.app,
                col as f32 * 84.0,
                row as f32 * 64.0,
            ));
        }
    }
    let middle = ids[4];
    h.app.board_sel = ids.into_iter().collect();
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 0.55;
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(middle).unwrap().rect;
    let world = Pos2::new(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    let screen = xf.w2s(world);
    assert!(
        h.app.wire_grip_at(screen, &xf).is_none(),
        "a press inside the selection is not a port"
    );
    let drag = h
        .app
        .begin_gesture_for_test(screen, world, egui::Modifiers::NONE);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "the group moves"
    );
}

/// New wires store no color, so they paint the active theme's wire gray.
/// A picked color is kept, even when it is the old default ink.
#[test]
fn default_wire_color_follows_the_theme() {
    let mut h = web_board("wire_theme_color");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 240.0, 0.0);
    h.frame();
    let wire = h.app.build_connector(
        slate_doc::scene::ConnectorEnd::Anchored {
            node: a,
            side: slate_doc::scene::Side::Right,
            t: 0.5,
        },
        slate_doc::scene::ConnectorEnd::Anchored {
            node: b,
            side: slate_doc::scene::Side::Left,
            t: 0.5,
        },
    );
    let id = h.app.add_nodes(vec![wire])[0];
    let conn = |h: &Harness| match &h.app.doc().scene.node(id).unwrap().kind {
        slate_doc::scene::NodeKind::Connector(c) => c.clone(),
        _ => panic!("connector"),
    };
    assert_eq!(conn(&h).color, None);
    h.app.dark_mode = false;
    let light = board::to_rgba(h.app.palette().wire);
    h.app.dark_mode = true;
    let dark = board::to_rgba(h.app.palette().wire);
    assert_ne!(light, dark);
    assert_eq!(conn(&h).paint_color(light), light);
    assert_eq!(conn(&h).paint_color(dark), dark);

    let ink = slate_doc::scene::WIRE_LEGACY_INK_LIGHT;
    let rgb = [ink.0[0], ink.0[1], ink.0[2]];
    h.app.board_sel = [id].into_iter().collect();
    h.app.patch_nodes(&[id], |n| {
        board_properties::Property::StrokeRgb(rgb).apply(n, None)
    });
    assert_eq!(conn(&h).color, Some(ink), "picking the old ink keeps it");
    assert_eq!(conn(&h).paint_color(dark), ink);
    h.app.patch_nodes(&[id], |n| {
        board_properties::Property::StrokeWidth(5.0).apply(n, None)
    });
    assert_eq!(conn(&h).color, Some(ink), "a width edit keeps the color");
}

/// Dropping a new wire on empty canvas commits a free end there.
/// The tool-search palette stays closed. Undo removes that wire.
#[test]
fn wire_drop_on_empty_canvas_keeps_a_free_end() {
    let mut h = web_board("wire_drop_empty");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let grip = xf.w2s(board_wire::grip_point(rect, slate_doc::scene::Side::Right));
    let mods = egui::Modifiers::default();
    let Some(board::BoardDrag::Wire(mut wd)) =
        h.app.begin_gesture_for_test(grip, xf.s2w(grip), mods)
    else {
        panic!("press on the grip starts a wire");
    };

    let drop = Pos2::new(rect.x + rect.w + 240.0, rect.y + rect.h * 0.5);
    h.app.wire_drag_update(&mut wd, drop, false);
    assert!(wd.snap.is_none(), "blank canvas is not a snap target");
    let end = [wd.cursor.x, wd.cursor.y];
    h.app.finish_wire_drag(wd);

    assert!(
        !h.app.palette_state.open,
        "empty release must not open the tool search"
    );
    let (a, b) = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            slate_doc::scene::NodeKind::Connector(c) => Some((c.a, c.b)),
            _ => None,
        })
        .expect("a wire is committed");
    assert!(matches!(
        a,
        slate_doc::scene::ConnectorEnd::Anchored { node, .. } if node == id
    ));
    match b {
        slate_doc::scene::ConnectorEnd::Free { point } => assert_eq!(point, end),
        other => panic!("free end at the drop, got {other:?}"),
    }
    assert!(
        slate_doc::wire::connector_route_in_scene(
            &h.app.doc().scene,
            None,
            &a,
            &b,
            h.app.board_wire_routing,
        )
        .is_some(),
        "the free end draws"
    );

    h.app.board_undo();
    assert!(
        h.app
            .doc()
            .scene
            .nodes
            .iter()
            .all(|n| !matches!(n.kind, slate_doc::scene::NodeKind::Connector(_))),
        "undo removes the wire"
    );
}
