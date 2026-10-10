//! The topmost painted node receives the press, whatever its kind.

use super::*;

fn board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Select);
    h
}

fn image_over(h: &mut Harness, host: slate_doc::scene::WorldRect) -> NodeId {
    let path = h.base.join(format!("pic-{}.png", now_nanos()));
    image::RgbaImage::from_pixel(8, 8, image::Rgba([10, 20, 30, 255]))
        .save(&path)
        .unwrap();
    let item = h.app.add_paths(std::slice::from_ref(&path))[0];
    let rect = slate_doc::scene::WorldRect::new(
        host.x + host.w * 0.25,
        host.y + host.h * 0.25,
        host.w * 0.5,
        host.h * 0.5,
    );
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    h.app.add_nodes(vec![node])[0]
}

fn center_of(h: &Harness, id: NodeId) -> (Pos2, Pos2) {
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let world = Pos2::new(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5);
    let screen = h.app.board_xf().w2s(world);
    (world, screen)
}

/// An image painted after a chat card is the press, and the card does not
/// claim the pointer through it. The reverse stack gives the card the press.
#[test]
fn image_over_chat_and_chat_over_image_follow_paint_order() {
    let mut h = board("hit_image_over_chat");
    h.app.place_agent_portal_at(Pos2::ZERO);
    h.frame();
    let chat = h.app.doc().scene.nodes[0].id;
    let host = h.app.doc().scene.node(chat).unwrap().rect;
    let image = image_over(&mut h, host);
    h.frame();
    let (world, screen) = center_of(&h, image);
    let xf = h.app.board_xf();
    assert_eq!(
        h.app.board_pick_node(world.x, world.y),
        Some(image),
        "the later image is the press"
    );
    assert!(
        !h.app.pointer_over_agent_card(&xf, Some(screen)),
        "a covered chat card does not own the pointer"
    );
    h.app.agent_focus(chat);
    assert!(
        !h.app.agent_shelf_captures(&xf, Some(screen)),
        "a focused chat still yields to the image on top"
    );

    let mut h = board("hit_chat_over_image");
    h.app.place_agent_portal_at(Pos2::new(2000.0, 2000.0));
    h.frame();
    let parked = h.app.doc().scene.nodes[0].id;
    let host = h.app.doc().scene.node(parked).unwrap().rect;
    let _image = image_over(&mut h, host);
    h.app
        .place_agent_portal_at(Pos2::new(host.x + host.w * 0.5, host.y + host.h * 0.5));
    h.frame();
    let chat = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .rev()
        .find(|n| matches!(n.kind, NodeKind::Portal(_)))
        .unwrap()
        .id;
    let (world, screen) = center_of(&h, chat);
    let xf = h.app.board_xf();
    assert_eq!(h.app.board_pick_node(world.x, world.y), Some(chat));
    assert!(h.app.pointer_over_agent_card(&xf, Some(screen)));
    h.app.agent_focus(chat);
    assert!(h.app.agent_shelf_captures(&xf, Some(screen)));
}
