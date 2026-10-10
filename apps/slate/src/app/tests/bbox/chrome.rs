//! Live bounding-box chrome, fillets, and portal outlines.

use super::*;

/// GP12 — bake adds the poster plus its provenance and leaves the portal alone
/// (D25).
#[test]
fn gp12_bake_copies_the_poster_and_leaves_the_portal_in_place() {
    let mut h = web_board("web_gp12");
    with_fake_host(&mut h);
    let page = h.base.join("dash.html");
    std::fs::write(&page, "<h1>hi</h1>").unwrap();
    h.app.divert_web_drops(&[page], Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.board_sel = std::iter::once(id).collect();

    assert!(h.app.web_bake_selected());
    let kinds: Vec<&str> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .map(|n| match &n.kind {
            NodeKind::Portal(_) => "portal",
            NodeKind::Image(_) => "image",
            NodeKind::Text(_) => "text",
            _ => "other",
        })
        .collect();
    assert!(
        kinds.contains(&"portal"),
        "bake copies, it does not convert"
    );
    assert!(kinds.contains(&"image"));
    assert!(kinds.contains(&"text"));
    let note = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap();
    assert!(note.contains("dash.html"), "provenance names the source");
    assert!(note.contains("captured"), "and when it was captured");
    h.frame();
}

/// With no WebView2 runtime, portals still place, bind, and export — they just
/// say what is missing instead of stalling (D29, D30).
#[test]
fn without_a_runtime_a_portal_degrades_instead_of_stalling() {
    let mut h = web_board("web_noruntime");
    h.app.paste_web_url("https://example.com/a", Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.web_allow_origin(id);
    web_settle(&mut h, &[(id, 600.0)], 2);
    assert_eq!(h.app.web.state(id), board_web::WebState::NoRuntime);
    assert_eq!(h.app.web.live_count(), 0);
}

/// Alt on a scale handle copies, then scales the copy. The original stays.
#[test]
fn alt_edge_scale_copies_and_keeps_the_original() {
    let mut h = web_board("alt_scale_copy");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let xf = h.app.board_xf();
    // Off the edge midpoint so a wire grip does not steal the press.
    let edge = xf.w2s(Pos2::new(rect.x + rect.w, rect.y + 12.0));
    let edge_world = xf.s2w(edge);
    h.app.alt_down = true;
    let drag = h.app.begin_gesture_for_test(
        edge,
        edge_world,
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    assert!(
        matches!(drag, Some(board::BoardDrag::Resize { dup: true, .. })),
        "Alt on an edge must scale a copy"
    );
    h.app.board_drag = drag;
    let grown = Pos2::new(rect.x + rect.w + 50.0, rect.y + 12.0);
    h.app.update_gesture_for_test(
        grown,
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    h.app.end_gesture_for_test(
        grown,
        Some(xf.w2s(grown)),
        egui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );

    assert_eq!(h.app.doc().scene.nodes.len(), 2);
    let original = h.app.doc().scene.node(id).unwrap().rect;
    assert!(
        (original.w - 80.0).abs() < 0.01 && (original.h - 60.0).abs() < 0.01,
        "original must stay, got {}×{}",
        original.w,
        original.h
    );
    let copy = h.app.doc().scene.nodes.iter().find(|n| n.id != id).unwrap();
    assert!(
        copy.rect.w > original.w + 20.0,
        "copy must grow, got {}",
        copy.rect.w
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    assert!(h.app.doc().scene.node(id).is_some());
}

/// Ctrl on an edge scales about the center.
#[test]
fn ctrl_edge_scale_keeps_the_center() {
    let mut h = web_board("ctrl_scale_center");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.insert(id);
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let before = h.app.doc().scene.node(id).unwrap().rect;
    let (cx, cy) = before.center();
    let xf = h.app.board_xf();
    let edge = xf.w2s(Pos2::new(before.x + before.w, before.y + 12.0));
    let mods = egui::Modifiers {
        ctrl: true,
        ..Default::default()
    };
    h.app.ctrl_down = true;
    h.app.board_drag = h.app.begin_gesture_for_test(edge, xf.s2w(edge), mods);
    let grown = Pos2::new(before.x + before.w + 40.0, before.y + 12.0);
    h.app.update_gesture_for_test(grown, mods);
    h.app.end_gesture_for_test(grown, Some(xf.w2s(grown)), mods);
    let after = h.app.doc().scene.node(id).unwrap().rect;
    let (nx, ny) = after.center();
    assert!(
        (nx - cx).abs() < 0.5 && (ny - cy).abs() < 0.5,
        "center walked to {nx},{ny}"
    );
    assert!(after.w > before.w + 20.0, "width {}", after.w);
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
}

/// Bounding-box chrome is live on hover — no prior selection (P1.node.transform).
#[test]
fn bbox_chrome_is_live_without_selection() {
    let mut h = web_board("bbox_chrome");
    add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.nodes[0].clone();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);

    let hit = h.app.transform_hit_at(geom.edges[1]);
    assert!(
        matches!(
            hit,
            Some((
                Some(_),
                board_handles::BoardHitTarget::Resize(board_handles::ResizeHandle::E)
            ))
        ),
        "right-edge hover must resize without a selection, got {hit:?}"
    );

    let hit = h.app.transform_hit_at(geom.rotate_points[0]);
    assert!(
        matches!(hit, Some((_, board_handles::BoardHitTarget::Rotate(_)))),
        "outside-corner hover must rotate a shape, got {hit:?}"
    );

    // Press the edge away from the wire-grip midpoint so resize, not a
    // wire, is the gesture (P1.node.transform).
    let edge = xf.w2s(Pos2::new(n.rect.x + n.rect.w, n.rect.y + 12.0));
    let world = xf.s2w(edge);
    let drag = h.app.begin_transform_drag(edge, world);
    assert!(
        matches!(drag, Some(board::BoardDrag::Resize { .. })),
        "pressing an edge on an unselected node starts resize"
    );
    assert!(
        h.app.board_sel.contains(&n.id),
        "starting a resize selects the node"
    );
}

#[test]
fn fillet_grip_press_beats_edge_band_but_nw_corner_still_resizes() {
    let mut h = web_board("fillet_press_order");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.patch_nodes(&[id], |node| {
        slate_doc::scene::set_corner(node, slate_doc::scene::Corner::Rounded { radius: 12.0 });
    });
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;

    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let geom = board_handles::selection_geom(&xf, node.rect, node.rotation_deg);
    let grip = h.app.fillet_grip_at(&node, &xf).expect("visible grip");
    let overlap = Pos2::new(
        grip.x,
        geom.corners[0].y
            + atlas_shell::canvas_scale::hit_px(board_handles::EDGE_BAND_PX, geom.zoom),
    );
    assert!(board_handles::hit_test_fillet_grip(overlap, &geom, grip));
    assert!(board_handles::hit_test_resize_bands(overlap, &geom).is_some());
    assert!(matches!(
        h.app
            .begin_gesture(overlap, xf.s2w(overlap), egui::Modifiers::NONE),
        Some(board::BoardDrag::FilletRadius { id: hit, .. }) if hit == id
    ));

    let nw = geom.corners[0];
    assert!(matches!(
        h.app
            .begin_gesture(nw, xf.s2w(nw), egui::Modifiers::NONE),
        Some(board::BoardDrag::Resize { id: hit, handle: 0, .. }) if hit == id
    ));
}

/// Edge hover changes the cursor target only — no body highlight (and
/// therefore no selection-look tab / handle chrome).
#[test]
fn edge_hover_does_not_arm_a_body_highlight() {
    let mut h = web_board("edge_hover_cursor");
    add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.nodes[0].clone();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);
    let ctx = h.ctx.clone();
    h.app
        .hover_transform_chrome(Some(geom.edges[1]), &xf, &ctx, false);
    assert!(
        matches!(
            h.app.board_hover_hit,
            Some(board_handles::BoardHitTarget::Resize(_))
        ),
        "edge hover must still be a resize hit"
    );
    let world = xf.s2w(geom.edges[1]);
    assert!(
        h.app.hover_preview_target(Some(world)).is_none(),
        "edge hover must not highlight the node like a selection"
    );
}

/// Interior hover is the eased preview target; the node is still unselected.
#[test]
fn body_hover_targets_the_unselected_node() {
    let mut h = web_board("body_hover_preview");
    let id = add_rect(&mut h.app, 0.0, 0.0);
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap();
    let center = xf.w2s(Pos2::new(
        n.rect.x + n.rect.w * 0.5,
        n.rect.y + n.rect.h * 0.5,
    ));
    let ctx = h.ctx.clone();
    h.app.hover_transform_chrome(Some(center), &xf, &ctx, false);
    assert!(
        h.app.board_hover_hit.is_none(),
        "the interior is not resize chrome"
    );
    let world = xf.s2w(center);
    assert_eq!(h.app.hover_preview_target(Some(world)), Some(id));
    assert!(h.app.board_sel.is_empty(), "hover never selects");
}

/// Selection outline follows painted geometry — fillets and ellipses, not the AABB.
#[test]
fn selection_outline_follows_silhouette() {
    let mut h = web_board("sel_silhouette");
    let square = add_rect(&mut h.app, 0.0, 0.0);
    let rounded = {
        use slate_doc::scene::{ShapeKind, ShapeNode};
        let rect = slate_doc::scene::WorldRect::new(100.0, 0.0, 80.0, 60.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Rect,
                fill: Some(slate_doc::scene::Rgba::WHITE),
                stroke: slate_doc::scene::Stroke::none(),
                corner: slate_doc::scene::Corner::Rounded { radius: 12.0 },
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,

                text: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    let ellipse = {
        use slate_doc::scene::{ShapeKind, ShapeNode};
        let rect = slate_doc::scene::WorldRect::new(200.0, 0.0, 80.0, 60.0);
        let node = h.app.doc_mut().scene.build_node(
            rect,
            slate_doc::scene::NodeKind::Shape(ShapeNode {
                shape: ShapeKind::Ellipse,
                fill: Some(slate_doc::scene::Rgba::WHITE),
                stroke: slate_doc::scene::Stroke::none(),
                corner: slate_doc::scene::Corner::Square,
                sides: slate_doc::scene::default_regular_sides(),
                phase_deg: 0.0,
                flip: false,
                path: None,

                text: None,
            }),
        );
        h.app.add_nodes(vec![node])[0]
    };
    h.app.place_web_portal_at(Pos2::new(400.0, 40.0));
    h.frame();
    let xf = h.app.board_xf();
    let sq = h.app.doc().scene.node(square).unwrap();
    assert_eq!(h.app.node_screen_outline(&h.ctx, &xf, sq).len(), 4);

    let rd = h.app.doc().scene.node(rounded).unwrap();
    let rd_pts = h.app.node_screen_outline(&h.ctx, &xf, rd);
    assert!(
        rd_pts.len() > 4,
        "a filleted rect must not highlight as a sharp box"
    );
    let srect = xf.rect_w2s(rd.rect);
    let sharp = srect.left_top();
    assert!(
        rd_pts.iter().all(|p| p.distance(sharp) > 2.0),
        "the highlight must leave the sharp corner empty"
    );

    let el = h.app.doc().scene.node(ellipse).unwrap();
    assert!(
        h.app.node_screen_outline(&h.ctx, &xf, el).len() > 4,
        "an ellipse highlight must be circular, not a box"
    );

    let portal = h.app.doc().scene.nodes.last().unwrap();
    let p_pts = h.app.node_screen_outline(&h.ctx, &xf, portal);
    assert!(
        p_pts.len() > 4,
        "a portal highlight must follow the shared fillet"
    );

    let strip = add_dock_strip(&mut h.app, "tool.shapes", &["shape.rect"]);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let sn = h.app.doc().scene.node(strip).unwrap().clone();
    let s_pts = h.app.node_screen_outline(&h.ctx, &xf, &sn);
    assert!(
        s_pts.len() > 4,
        "a dock-strip highlight must follow the shared fillet"
    );
    let slate_doc::scene::NodeKind::DockStrip(strip_data) = &sn.kind else {
        panic!("expected a dock strip");
    };
    let (card, r) = h.app.dock_strip_screen_card(&h.ctx, &xf, &sn, strip_data);
    let sharp = card.left_top();
    assert!(
        s_pts.iter().all(|p| p.distance(sharp) > r * 0.3),
        "the dock-strip highlight must leave the sharp corner empty"
    );
}

/// P1.node.corner-grip: a selected portal's painted outline is its authored
/// corner — a chamfer as well as a fillet — not the frame box.
#[test]
fn selected_portal_outline_follows_its_authored_corner() {
    let mut h = web_board("sel_portal_corner");
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 320.0, 200.0),
        NodeKind::Portal(slate_doc::PortalNode::unbound_web("Web")),
    );
    let id = node.id;
    h.app.add_nodes(vec![node]);
    h.app
        .zoom_to_rect(WorldRect::new(-40.0, -40.0, 400.0, 280.0));
    for corner in [
        slate_doc::scene::Corner::Chamfer { cut: 30.0 },
        slate_doc::scene::Corner::Rounded { radius: 30.0 },
    ] {
        h.app.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                p.corner = corner;
            }
        });
        h.app.board_sel = std::iter::once(id).collect();
        h.frame();
        let out = h.frame_output(|_| {});
        let xf = h.app.board_xf();
        let node = h.app.doc().scene.node(id).unwrap();
        let expected = board::corner_outline(xf.rect_w2s(node.rect), corner, xf.z);
        assert!(
            expected.len() > 4,
            "{corner:?}: a cornered outline, not a box"
        );
        let same = |a: &[Pos2]| {
            a.len() == expected.len() && a.iter().zip(&expected).all(|(p, q)| p.distance(*q) < 0.01)
        };
        assert!(
            same(&h.app.node_screen_outline(&h.ctx, &xf, node)),
            "{corner:?}: the selection outline is the corner outline"
        );
        assert!(
            painted_closed_paths(&out).iter().any(|pts| same(pts)),
            "{corner:?}: the painted selection ring is the corner outline"
        );
    }
}

/// Portals stay axis-aligned: no rotate chrome, but edges still resize.
#[test]
fn portals_do_not_rotate() {
    let mut h = web_board("portal_norot");
    h.app.place_web_portal_at(Pos2::new(0.0, 0.0));
    h.app.board_sel.clear();
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();

    let n = h.app.doc().scene.nodes[0].clone();
    assert!(
        !SlateApp::node_allows_rotation(&n),
        "portals must not offer rotation"
    );

    let xf = h.app.board_xf();
    let geom = board_handles::selection_geom(&xf, n.rect, n.rotation_deg);

    let hit = h.app.transform_hit_at(geom.rotate_points[0]);
    assert!(
        !matches!(hit, Some((_, board_handles::BoardHitTarget::Rotate(_)))),
        "outside-corner hover must not rotate a portal, got {hit:?}"
    );

    let hit = h.app.transform_hit_at(geom.edges[1]);
    assert!(
        matches!(
            hit,
            Some((
                Some(_),
                board_handles::BoardHitTarget::Resize(board_handles::ResizeHandle::E)
            ))
        ),
        "portal edges still resize, got {hit:?}"
    );
}
