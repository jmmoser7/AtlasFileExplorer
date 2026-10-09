//! Image paint and picture tests.

use super::*;

/// tip18 inside an image-paint session: the Shift drag previews every frame
/// and extends the last mark on the image's layer as one path (D03), so
/// the joint keeps the stroke's opacity; each segment is one undo step.
#[test]
fn brush_shift_drag_in_an_image_paint_session_previews() {
    let mut h = Harness::new("tip18_img");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 400.0, 300.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.app.brush_opacity = 0.5;
    h.app.brush_softness = 0.0;
    h.frame();
    h.frame();
    assert!(h.app.image_paint_session().is_some());
    press_drag_release_frames(
        &mut h,
        &[
            Pos2::new(40.0, 40.0),
            Pos2::new(80.0, 60.0),
            Pos2::new(120.0, 40.0),
        ],
        egui::Modifiers::NONE,
        |_| {},
    );
    let layer_nodes = |h: &Harness| match &h.app.doc().scene.node(image_id).unwrap().kind {
        slate_doc::scene::NodeKind::Image(img) => img
            .paint_layers
            .iter()
            .map(|l| l.nodes.len())
            .sum::<usize>(),
        _ => 0,
    };
    assert_eq!(layer_nodes(&h), 1);
    let mark = |h: &Harness| match &h.app.doc().scene.node(image_id).unwrap().kind {
        slate_doc::scene::NodeKind::Image(img) => img.paint_layers[0].nodes[0].clone(),
        _ => panic!("the image"),
    };
    let drawn = mark(&h);
    let from = h.app.brush_line_anchor().expect("the mark's end").pos;
    let raw_end = Pos2::new(300.0, 250.0);
    press_drag_release_frames(
        &mut h,
        &[Pos2::new(200.0, 200.0), Pos2::new(250.0, 150.0), raw_end],
        egui::Modifiers::SHIFT,
        |h| {
            assert!(h.app.brush_straight.is_some());
            assert!(h.app.brush_live.as_ref().is_some_and(|c| c.showing_line()));
        },
    );
    assert_eq!(
        layer_nodes(&h),
        1,
        "the segment extends the mark on the layer"
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "and not on the board");
    let host = h.app.doc().scene.node(image_id).unwrap().clone();
    let slate_doc::scene::NodeKind::Image(img) = &host.kind else {
        panic!("the image");
    };
    let world = slate_doc::image_paint::layer_node_to_world(&host, img, &mark(&h));
    let v = path_vertices(&world);
    let end = board_snap::ortho_snap_point(from, raw_end);
    assert!(
        near_px(v[v.len() - 2], from) && near_px(v[v.len() - 1], end),
        "{v:?}"
    );
    // One stamp: the joint is no more opaque than the stroke's body.
    let mut stamps = Default::default();
    let rgba = slate_artifact::rasterize_paint_layers(&host, img, 400, 300, None, &mut stamps)
        .expect("the layer rasters");
    let alpha = |p: Pos2| rgba[(p.y as usize * 400 + p.x as usize) * 4 + 3];
    let body = alpha(from + (end - from) * 0.5);
    assert!(body > 100, "the segment paints: {body}");
    assert!(
        alpha(from) <= body + 2,
        "the joint builds past the stroke's opacity: {} over {body}",
        alpha(from)
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(mark(&h), drawn, "one undo takes back just the segment");
}

/// Review r9 finding 9: on an offset image turned 30°, two Shift segments
/// each extend the layer mark through the rotated basis; each Ctrl+Z takes
/// back one segment and each Ctrl+Y restores exactly what it took.
#[test]
fn brush_shift_segments_on_a_rotated_image_undo_and_redo_one_at_a_time() {
    let (mut h, image_id) = image_paint_board(
        "tip18_img_rotated",
        slate_doc::scene::WorldRect::new(150.0, 80.0, 400.0, 300.0),
        30.0,
        &[
            Pos2::new(300.0, 200.0),
            Pos2::new(340.0, 220.0),
            Pos2::new(380.0, 200.0),
        ],
    );
    let mark = |h: &Harness| {
        let marks = layer_marks(h, image_id);
        assert_eq!(
            marks.iter().map(Vec::len).sum::<usize>(),
            1,
            "one layer mark"
        );
        marks[0][0].clone()
    };
    let world_end = |h: &Harness| {
        let host = h.app.doc().scene.node(image_id).unwrap().clone();
        let NodeKind::Image(img) = &host.kind else {
            panic!("the image");
        };
        let world = slate_doc::image_paint::layer_node_to_world(&host, img, &mark(h));
        let v = path_vertices(&world);
        (v[v.len() - 2], v[v.len() - 1], v.len())
    };
    let mut marks = vec![mark(&h)];
    let (_, _, drawn_len) = world_end(&h);
    for (k, (press, raw_end)) in [
        (Pos2::new(390.0, 230.0), Pos2::new(430.0, 250.0)),
        (Pos2::new(420.0, 260.0), Pos2::new(330.0, 262.0)),
    ]
    .into_iter()
    .enumerate()
    {
        let from = h.app.brush_line_anchor().expect("the mark's end").pos;
        press_drag_release_frames(
            &mut h,
            &[press, press + (raw_end - press) * 0.5, raw_end],
            egui::Modifiers::SHIFT,
            |_| {},
        );
        assert_eq!(
            h.app.doc().scene.nodes.len(),
            1,
            "segment {k} stays on the layer"
        );
        let end = board_snap::ortho_snap_point(from, raw_end);
        let (a, b, len) = world_end(&h);
        assert!(
            near_px(a, from) && near_px(b, end),
            "segment {k}: {a:?} {b:?}"
        );
        assert_eq!(len, drawn_len + k + 1, "segment {k} adds one vertex");
        marks.push(mark(&h));
    }
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        mark(&h),
        marks[1],
        "the first undo takes back the second segment"
    );
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(mark(&h), marks[0], "the second undo takes back the first");
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(
        mark(&h),
        marks[1],
        "the first redo restores the first segment"
    );
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(mark(&h), marks[2], "the second redo restores the second");
}

#[test]
fn image_paint_brush_commits_into_active_layer_not_scene() {
    let mut h = Harness::new("image_paint_brush");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let node = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = node.id;
    h.app.add_nodes(vec![node]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.sync_image_paint_for_tool();
    h.app
        .finish_freehand_brush(vec![Pos2::new(50.0, 50.0), Pos2::new(150.0, 50.0)]);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "stroke stays on the layer"
    );
    let host = h.app.doc().scene.node(image_id).unwrap();
    let slate_doc::scene::NodeKind::Image(img) = &host.kind else {
        panic!("image");
    };
    assert_eq!(img.paint_layers.len(), 1);
    assert_eq!(img.paint_layers[0].nodes.len(), 1);
}

#[test]
fn image_paint_session_clears_before_drawing_off_another_selection() {
    let mut h = Harness::new("image_paint_session_clear");
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = image.id;
    h.app.add_nodes(vec![image]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.set_board_tool(board::BoardTool::Brush);
    assert!(h.app.image_paint_session().is_some());

    let other = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(300.0, 0.0, 20.0, 20.0),
        slate_doc::scene::NodeKind::Shape(slate_doc::scene::ShapeNode {
            shape: slate_doc::scene::ShapeKind::Rect,
            sides: 6,
            phase_deg: 0.0,
            fill: Some(slate_doc::scene::Rgba::opaque(0, 0, 0)),
            stroke: slate_doc::scene::Stroke::none(),
            corner: Default::default(),
            flip: false,
            path: None,
            text: None,
        }),
    );
    let other_id = other.id;
    h.app.add_nodes(vec![other]);
    h.app.board_sel = std::iter::once(other_id).collect();
    h.app.set_board_tool(board::BoardTool::RectShape);
    h.app.finish_draw(
        Pos2::new(400.0, 200.0),
        Pos2::new(500.0, 300.0),
        board::BoardTool::RectShape,
        egui::Modifiers::NONE,
    );
    assert!(h.app.image_paint_session().is_none());
    assert_eq!(h.app.doc().scene.nodes.len(), 3);
    let host = h.app.doc().scene.node(image_id).unwrap();
    let slate_doc::scene::NodeKind::Image(img) = &host.kind else {
        panic!("image");
    };
    assert!(img.paint_layers.is_empty());
}

#[test]
fn image_drop_replace_is_one_undo_group() {
    let mut h = Harness::new("image_drop_replace_undo");
    h.app.ensure_work_tab();
    let a = h.base.join("a.png");
    let b = h.base.join("b.png");
    std::fs::write(&a, b"a").unwrap();
    std::fs::write(&b, b"b").unwrap();
    let items = h.app.add_paths(&[a, b]);
    let target = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[0])),
    );
    let source = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(150.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[1])),
    );
    let (target_id, source_id) = (target.id, source.id);
    h.app.add_nodes(vec![target, source]);
    assert!(h
        .app
        .replace_image_item_preserve_layers(target_id, items[1], Some(source_id)));
    assert!(h.app.doc().scene.node(source_id).is_none());
    h.app.board_undo();
    assert!(h.app.doc().scene.node(source_id).is_some());
    let slate_doc::scene::NodeKind::Image(img) = &h.app.doc().scene.node(target_id).unwrap().kind
    else {
        panic!("image");
    };
    assert_eq!(img.item, items[0]);
}

#[test]
fn image_drop_layer_on_rotated_host_is_one_undo_group() {
    let mut h = Harness::new("image_drop_layer_rotated_undo");
    h.app.ensure_work_tab();
    let a = h.base.join("a.png");
    let b = h.base.join("b.png");
    std::fs::write(&a, b"a").unwrap();
    std::fs::write(&b, b"b").unwrap();
    let items = h.app.add_paths(&[a, b]);
    let mut target = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[0])),
    );
    target.rotation_deg = 37.0;
    let source = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(250.0, 0.0, 100.0, 100.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(items[1])),
    );
    let (target_id, source_id) = (target.id, source.id);
    h.app.add_nodes(vec![target, source]);
    assert!(h
        .app
        .add_dropped_image_as_layer(target_id, items[1], Some(source_id)));
    assert!(h.app.doc().scene.node(source_id).is_none());
    let slate_doc::scene::NodeKind::Image(img) = &h.app.doc().scene.node(target_id).unwrap().kind
    else {
        panic!("image");
    };
    assert_eq!(img.paint_layers.len(), 1);
    assert!(img.paint_layers[0].nodes[0].rotation_deg.abs() < 1e-4);
    h.app.board_undo();
    assert!(h.app.doc().scene.node(source_id).is_some());
    let slate_doc::scene::NodeKind::Image(img) = &h.app.doc().scene.node(target_id).unwrap().kind
    else {
        panic!("image");
    };
    assert!(img.paint_layers.is_empty());
}
