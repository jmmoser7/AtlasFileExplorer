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

/// A picture lying on a focused web page is grabbed: the page hears no press,
/// gives up focus, and the portal stays put.
#[test]
fn a_picture_on_a_focused_page_is_dragged_and_the_page_hears_nothing() {
    let (mut h, page, host) = focused_page("hit_image_over_page");
    let frame = h.app.doc().scene.node(page).unwrap().rect;
    let picture = image_over(&mut h, frame);
    h.frame();
    let (world, _) = center_of(&h, picture);
    let before = h.app.doc().scene.node(picture).unwrap().rect;
    let step = before.w * 0.1;
    let path: Vec<Pos2> = (0..=5)
        .map(|i| world + EVec2::new(step * i as f32, 0.0))
        .collect();
    press_drag_release_frames(&mut h, &path, egui::Modifiers::NONE, |_| {});
    let after = h.app.doc().scene.node(picture).unwrap().rect;
    assert!(
        (after.x - before.x - step * 5.0).abs() < 1.0,
        "the picture moves with the drag: {} -> {}",
        before.x,
        after.x
    );
    let downs = host.sent(|i| matches!(i, board_web::WebInput::Down { .. }).then_some(()));
    assert!(downs.is_empty(), "the covered page hears no press");
    assert_eq!(
        h.app.web.focused, None,
        "a press on the picture peels page focus"
    );
    assert_eq!(h.app.doc().scene.node(page).unwrap().rect, frame);
}

/// The same rule over a focused File Atlas portal: the folder map does not
/// claim a drag that starts on a picture lying on it.
#[test]
fn a_picture_on_a_focused_folder_map_is_dragged() {
    let (mut h, atlas, _) = bound_atlas_lens("hit_image_over_atlas");
    let frame = h.app.doc().scene.node(atlas).unwrap().rect;
    h.app.zoom_to_rect(frame);
    h.app.atlas_focus(atlas);
    let picture = image_over(&mut h, frame);
    h.frame();
    let (world, _) = center_of(&h, picture);
    let before = h.app.doc().scene.node(picture).unwrap().rect;
    let step = before.w * 0.1;
    let path: Vec<Pos2> = (0..=5)
        .map(|i| world + EVec2::new(step * i as f32, 0.0))
        .collect();
    press_drag_release_frames(&mut h, &path, egui::Modifiers::NONE, |_| {});
    let after = h.app.doc().scene.node(picture).unwrap().rect;
    assert!(
        (after.x - before.x - step * 5.0).abs() < 1.0,
        "the picture moves with the drag: {} -> {}",
        before.x,
        after.x
    );
    assert_eq!(
        h.app.atlas_lenses.focused, None,
        "a press on the picture peels focus"
    );
    assert_eq!(h.app.doc().scene.node(atlas).unwrap().rect, frame);
}

/// The press arrives together with the first hover event: no selection or
/// warm-up hover is needed before the picture follows the pointer.
#[test]
fn unselected_image_moves_on_first_press_drag_and_one_undo_restores_it() {
    let mut h = board("first_press_image_drag");
    let image = image_over(&mut h, WorldRect::new(-200.0, -150.0, 400.0, 300.0));
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.frame();
    h.frame();
    assert!(h.app.board_sel.is_empty());
    let before = h.app.doc().scene.node(image).unwrap().rect;
    let (_, start) = center_of(&h, image);
    crop_pointer(&mut h, start, Some(true));
    crop_pointer(&mut h, start + EVec2::new(30.0, 20.0), None);
    crop_pointer(&mut h, start + EVec2::new(60.0, 40.0), None);
    crop_pointer(&mut h, start + EVec2::new(60.0, 40.0), Some(false));
    let after = h.app.doc().scene.node(image).unwrap().rect;
    let z = h.app.board_xf().z;
    assert!(
        (after.x - before.x - 60.0 / z).abs() < 0.1,
        "{before:?} -> {after:?}"
    );
    assert!((after.y - before.y - 40.0 / z).abs() < 0.1);
    assert_eq!((after.w, after.h), (before.w, before.h));
    assert!(h.app.board_sel.contains(&image));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(image).unwrap().rect, before);
}

#[test]
fn front_body_occludes_rear_resize_but_front_corner_keeps_its_priority() {
    let mut h = board("transform_stack_order");
    let rear = image_over(&mut h, WorldRect::new(-200.0, -150.0, 400.0, 300.0));
    let front = image_over(&mut h, WorldRect::new(-300.0, -250.0, 400.0, 300.0));
    h.frame();
    h.frame();
    let xf = h.app.board_xf();
    let rear_rect = h.app.doc().scene.node(rear).unwrap().rect;
    let at_rear_corner = xf.w2s(Pos2::new(rear_rect.x, rear_rect.y));
    assert!(matches!(h.app.transform_hit_at(at_rear_corner),
        Some((Some(id), board_handles::BoardHitTarget::Body)) if id == front));

    // The front corner is inside the rear picture. Its visible resize
    // affordance still wins, even though the rear body is also under it.
    h.app.patch_nodes(&[front], |n| {
        n.rect = WorldRect::new(0.0, 0.0, 160.0, 120.0)
    });
    let at_front_corner = xf.w2s(Pos2::ZERO);
    assert!(matches!(h.app.transform_hit_at(at_front_corner),
        Some((Some(id), board_handles::BoardHitTarget::Resize(board_handles::ResizeHandle::Nw))) if id == front));
}
