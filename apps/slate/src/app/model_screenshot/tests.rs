use super::super::tests::Harness;
use super::*;

fn add_model(app: &mut SlateApp, path: PathBuf) -> NodeId {
    let item = app
        .doc_mut()
        .add_item(path, "model.obj", 1, 0, "model-cache");
    let node = app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 320.0, 200.0),
        NodeKind::Image(ImageNode::new(item)),
    );
    app.add_nodes(vec![node])[0]
}

/// A saved workbook with one selected triangle model on its board.
pub(super) fn selected_model(tag: &str) -> (Harness, NodeId) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.tab_mut().path = Some(h.base.join("book.slate"));
    let source = h.base.join("tri.obj");
    std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let items = h.app.add_paths(&[source]);
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.place_items_on_board(&items, Pos2::ZERO);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    h.app.zoom_to_rect(WorldRect::new(
        rect.x - rect.w,
        rect.y - rect.h,
        rect.w * 3.0,
        rect.h * 3.0,
    ));
    h.app.board_sel = std::iter::once(id).collect();
    for _ in 0..3 {
        h.frame();
    }
    (h, id)
}

fn click(h: &mut Harness, p: Pos2) {
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(p)));
    for pressed in [true, false] {
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            });
        });
    }
    h.frame();
}

fn click_screenshot_button(h: &mut Harness) {
    let button = h
        .app
        .model_screenshot_button()
        .expect("a selected model offers the screenshot button");
    click(h, button.center());
}

#[test]
fn a_model_draws_and_hits_no_view_port_at_rest() {
    use slate_doc::agent_inputs::{input_ports_of, MODEL_VIEW_PORT_T};
    use slate_doc::scene::Side;
    let (mut h, model) = selected_model("view_port_rest");
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(model).unwrap().clone();
    let old_port = slate_doc::WireHost::from_node_flow(&node, input_ports_of(&node, true))
        .anchor(Side::Left, MODEL_VIEW_PORT_T);
    let at = xf.w2s(Pos2::new(old_port[0], old_port[1]));
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(at)));
    let out = h.ctx.run(Default::default(), |ctx| {
        let layer = egui::LayerId::new(egui::Order::Foreground, Id::new("ports"));
        h.app.paint_flow_ports(&ctx.layer_painter(layer), &xf);
    });
    assert!(
        out.shapes.is_empty(),
        "selected, pointer on the old site: nothing painted"
    );
    assert_eq!(h.app.wire_grip_at(at, &xf), None, "and nothing to grab");
}

#[test]
fn a_view_wire_snaps_to_the_model_edge_during_a_drag_and_binds() {
    use slate_doc::scene::Side;
    let (mut h, model) = selected_model("view_port_drag");
    let png = h.base.join("shot.png");
    image::RgbaImage::new(4, 4).save(&png).unwrap();
    let item = h
        .app
        .doc_mut()
        .add_item(png, "shot.png", 1, 0, "shot-cache");
    let rect = h.app.doc().scene.node(model).unwrap().rect;
    let node = h.app.doc_mut().scene.build_node(
        WorldRect::new(rect.x - rect.w * 0.8, rect.y, rect.w * 0.4, rect.h * 0.4),
        NodeKind::Image(ImageNode::new(item)),
    );
    let shot = h.app.add_nodes(vec![node])[0];
    h.app.board_sel.clear();
    h.frame();

    let xf = h.app.board_xf();
    let shot_rect = h.app.doc().scene.node(shot).unwrap().rect;
    let grip = xf.w2s(super::super::board_wire::grip_point(shot_rect, Side::Right));
    let Some(super::super::board::BoardDrag::Wire(mut wd)) =
        h.app
            .begin_gesture_for_test(grip, xf.s2w(grip), Default::default())
    else {
        panic!("a wire starts at the picture's grip");
    };
    let near_edge = Pos2::new(rect.x - 2.0 / xf.z, rect.y + rect.h * 0.7);
    h.app.wire_drag_update(&mut wd, near_edge, false);
    let (node, side, t) = wd.snap.expect("near the edge, the model takes the wire");
    assert_eq!((node, side), (model, Side::Left));
    assert!(
        (t - 0.7).abs() < 0.05,
        "at the pointer, not a fixed port: {t}"
    );
    h.app.finish_wire_drag(wd);
    let binding = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::Connector(c) => c.binding.clone(),
            _ => None,
        })
        .expect("the wire is bound");
    assert_eq!(
        binding.slot.as_deref(),
        Some(atlas_agent::InputSlot::View.id())
    );
}

#[test]
fn the_screenshot_button_opens_a_menu_that_stays_open() {
    let (mut h, id) = selected_model("shot_button_opens");
    click_screenshot_button(&mut h);
    let popup = h.app.model_shot_popup.as_ref().expect("the menu is open");
    assert_eq!(popup.node, id);
    h.frame();
    assert!(h.app.model_shot_popup.is_some(), "and stays open");
}

#[test]
fn the_screenshot_menu_opens_clear_of_its_icon_on_a_higher_layer() {
    let (mut h, id) = selected_model("shot_menu_clear");
    click_screenshot_button(&mut h);
    h.frame();
    let button = h.app.model_screenshot_button().expect("the strip is up");
    let menu_id = Id::new(("model_shot_menu", h.app.tab().id, id.0));
    let menu = h
        .ctx
        .memory(|m| m.area_rect(menu_id))
        .expect("the menu was laid out");
    assert!(!menu.intersects(button), "{menu:?} covers {button:?}");
    assert!(h.app.canvas_rect.contains_rect(menu), "{menu:?}");
    let layer = h.ctx.layer_id_at(menu.center()).expect("hit-testable");
    assert_eq!(layer.id, menu_id);
    assert!(
        layer.order > egui::Order::Foreground,
        "strip icons paint on a Foreground layer above every Foreground area: {layer:?}"
    );
}

#[test]
fn export_to_canvas_requests_a_capture_and_says_why_it_cannot() {
    let (mut h, id) = selected_model("shot_export_no_gl");
    click_screenshot_button(&mut h);
    assert!(h.app.model_shot_popup.is_some(), "the menu is open");
    let tab = h.app.tab().id;
    let menu = h
        .ctx
        .memory(|m| m.area_rect(Id::new(("model_shot_menu", tab, id.0))))
        .expect("the menu was laid out");
    let nodes = h.app.doc().scene.nodes.len();
    // "Export to canvas" is the upper of the two left-aligned buttons.
    click(
        &mut h,
        Pos2::new(menu.left() + 24.0, menu.top() + menu.height() * 0.25),
    );
    assert!(h.app.model_shot_popup.is_none(), "choosing closes the menu");
    assert!(
        h.app
            .toasts
            .iter()
            .any(|(m, _)| m.starts_with("Screenshot failed") && m.contains("GPU rendering")),
        "the capture ran and its failure is visible: {:?}",
        h.app.toasts.iter().map(|(m, _)| m).collect::<Vec<_>>()
    );
    assert!(
        !h.app
            .toasts
            .iter()
            .any(|(m, _)| m.starts_with("3D viewports")),
        "the menu click did not reach the board beneath it"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), nodes);
}

#[test]
fn a_rendered_screenshot_lands_as_one_image_node_and_one_file() {
    let (mut h, id) = selected_model("shot_writer");
    let info = h.app.model_node_info(id).unwrap();
    let (w, hh) = (8u32, 4u32);
    let cam = ModelCamera {
        yaw: 0.7,
        distance: 5.0,
        ..info.cam
    };
    let meta = h
        .app
        .view_meta_input(id, &info, cam, w, hh, &ImageAdjust::default())
        .unwrap();
    let shot = ModelShot {
        rgba: vec![200; (w * hh * 4) as usize],
        w,
        h: hh,
        meta,
    };
    let before = h.app.doc().scene.nodes.len();
    h.app.save_model_screenshot(id, shot, None);
    let scene = &h.app.doc().scene;
    assert_eq!(scene.nodes.len(), before + 1, "exactly one node added");
    let placed = scene.nodes.last().unwrap();
    let NodeKind::Image(img) = &placed.kind else {
        panic!("an image node");
    };
    let stored = h.app.doc().item(img.item).unwrap().path.clone();
    let path =
        slate_doc::scene::resolve_source(h.app.tab().path.as_deref(), &stored.to_string_lossy());
    assert!(
        path.starts_with(h.base.join("assets")),
        "stored {} resolved {}",
        stored.display(),
        path.display()
    );
    let files: Vec<_> = std::fs::read_dir(h.base.join("assets")).unwrap().collect();
    assert_eq!(files.len(), 1, "exactly one file written");
    let restored = view_meta::read_view_meta(&path)
        .unwrap()
        .expect("view packet");
    assert!((restored.camera.yaw - 0.7).abs() < 1e-4);
    assert_eq!(
        h.app.board_sel.iter().copied().collect::<Vec<_>>(),
        vec![placed.id]
    );
}

fn parsed(camera: ModelCamera) -> view_meta::ViewMetaParsed {
    view_meta::ViewMetaParsed {
        camera,
        model_name: "model.obj".into(),
        model_path: "model.obj".into(),
        model_hash: String::new(),
        model_size: 1,
        node_id: 1,
    }
}

#[test]
fn view_drop_result_never_crosses_tabs_with_colliding_node_ids() {
    let mut h = Harness::new("view_drop_tabs");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let first_id = add_model(&mut h.app, h.base.join("first.obj"));
    let first_tab = h.app.tab().id;
    let restored = ModelCamera {
        yaw: 1.25,
        distance: 8.0,
        ..ModelCamera::default()
    };
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(ViewDropMsg::Restored(parsed(restored), None))
        .unwrap();
    h.app.pending_view_drop = Some(PendingViewDrop {
        tab_id: first_tab,
        model: first_id,
        image_path: h.base.join("first.png"),
        picture: None,
        pointer: Pos2::ZERO,
        rx,
    });

    h.app.new_tab();
    let second_id = add_model(&mut h.app, h.base.join("second.obj"));
    assert_eq!(first_id, second_id, "fixture must exercise an id collision");
    h.app.maintain_view_drop();
    let second_camera = match &h.app.doc().scene.node(second_id).unwrap().kind {
        NodeKind::Image(image) => image.model,
        _ => unreachable!(),
    };
    assert_ne!(second_camera, restored);

    h.app.switch_tab(0);
    let first_camera = match &h.app.doc().scene.node(first_id).unwrap().kind {
        NodeKind::Image(image) => image.model,
        _ => unreachable!(),
    };
    assert_ne!(first_camera, restored, "inactive result must be discarded");
}

#[test]
fn view_drop_refuses_read_only_workbook() {
    let mut h = Harness::new("view_drop_read_only");
    h.app.leave_home();
    h.app.ensure_work_tab();
    let id = add_model(&mut h.app, h.base.join("readonly.obj"));
    let before = match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(image) => image.model,
        _ => unreachable!(),
    };
    let restored = ModelCamera {
        yaw: 0.9,
        distance: 6.0,
        ..before
    };
    h.app.tab_mut().read_only = true;
    h.app.apply_view_drop(id, parsed(restored));
    let after = match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(image) => image.model,
        _ => unreachable!(),
    };
    assert_eq!(after, before);
    assert!(h
        .app
        .toasts
        .iter()
        .any(|(message, _)| message.contains("read-only")));
}
