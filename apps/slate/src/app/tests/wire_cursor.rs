//! The wire-grab hit zone changes the cursor on every node kind.

use super::*;

fn hover_grip(h: &mut Harness, kind: &str) -> egui::CursorIcon {
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    let node = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .rev()
        .find(|n| !matches!(n.kind, NodeKind::Connector(_)))
        .unwrap()
        .clone();
    assert!(
        board_wire::wire_ports_fit(node.rect, xf.z),
        "{kind} fixture must offer eligible wire ports: {:?}",
        node.rect
    );
    let point = h
        .app
        .wire_host(&node)
        .anchor(slate_doc::scene::Side::Right, 0.5);
    let grip = xf.w2s(Pos2::new(point[0], point[1]));
    // egui reports the canvas hovered from the second frame the pointer is over it.
    h.frame_with(pointer_to(grip, false));
    let icon = h
        .frame_output(pointer_to(grip, false))
        .platform_output
        .cursor_icon;
    assert_eq!(
        h.app.wire_grips.map(|g| g.hovered),
        Some(Some(slate_doc::scene::Side::Right)),
        "{kind} grip did not arm"
    );
    icon
}

#[test]
fn wire_grab_zone_uses_a_grab_cursor_on_every_node_kind() {
    let mut h = web_board("wire_cursor_rect");
    add_rect(&mut h.app, 0.0, 0.0);
    assert_eq!(hover_grip(&mut h, "rectangle"), egui::CursorIcon::Grab);

    let mut h = web_board("wire_cursor_text");
    // The authored 40-unit host is eligible at 1:1, not the default 0.8x.
    h.app.tab_mut().cam.z = 1.0;
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 160.0, 40.0),
        NodeKind::Text(slate_doc::scene::TextNode {
            text: "Hello".into(),
            style: Default::default(),
            family: Default::default(),
            // Keep its measured line above the short-host suppression floor.
            size: 64.0,
            color: slate_doc::scene::Rgba::BLACK,
            align: Default::default(),
            fill: None,
            stroke: slate_doc::scene::Stroke::none(),
            agent: None,
        }),
    );
    h.app.add_nodes(vec![node]);
    assert_eq!(hover_grip(&mut h, "text"), egui::CursorIcon::Grab);

    let mut h = web_board("wire_cursor_image");
    let path = h.base.join("cursor.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    let item = h.app.item_for_path(&path).unwrap();
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 120.0, 80.0),
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    h.app.add_nodes(vec![node]);
    assert_eq!(hover_grip(&mut h, "image"), egui::CursorIcon::Grab);

    let mut h = web_board("wire_cursor_portal");
    h.app.place_web_portal_at(Pos2::ZERO);
    assert_eq!(hover_grip(&mut h, "web portal"), egui::CursorIcon::Grab);

    let mut h = web_board("wire_cursor_frame");
    h.app.place_frame_at(Pos2::ZERO);
    assert_eq!(hover_grip(&mut h, "frame"), egui::CursorIcon::Grab);

    let mut h = web_board("wire_cursor_atlas");
    h.app.place_atlas_portal_at(Pos2::ZERO);
    assert_eq!(hover_grip(&mut h, "atlas portal"), egui::CursorIcon::Grab);

    let mut h = web_board("wire_cursor_agent");
    h.app.place_agent_portal_at(Pos2::ZERO);
    let id = *h.app.board_sel.iter().next().unwrap();
    // The unbound program chooser has no wire handles. A bound image agent
    // offers the same right output port as its generated image.
    h.app.set_agent_program(id, "comfy");
    assert_eq!(hover_grip(&mut h, "agent picture"), egui::CursorIcon::Grab);
}
