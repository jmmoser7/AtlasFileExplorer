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
    let grip = xf.w2s(board_wire::grip_point(
        node.rect,
        slate_doc::scene::Side::Right,
    ));
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
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 160.0, 40.0),
        NodeKind::Text(slate_doc::scene::TextNode {
            text: "Hello".into(),
            family: Default::default(),
            size: 18.0,
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
}
