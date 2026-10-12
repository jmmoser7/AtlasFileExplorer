//! The real intake, journal and crop gesture preserve authored shape/image data.
use super::*;
use crate::app::{board::BoardDrag, tests::Harness};
use slate_doc::{
    image_paint::{ImageResultId, ImageResultLayers, PaintLayerId},
    scene::*,
};

#[path = "tests/generated.rs"]
mod generated;

fn fixture(tag: &str, kind: ShapeKind) -> (Harness, NodeId, NodeId) {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.app.set_board_tool(BoardTool::Select);
    let path = h.base.join("source.png");
    image::RgbaImage::from_pixel(200, 100, image::Rgba([40, 80, 120, 255]))
        .save(&path)
        .unwrap();
    let item = h.app.add_paths(&[path])[0];
    let mut image = ImageNode::new(item);
    image.adjust.brightness = 1.2;
    image.flip_x = true;
    let mut stack = ImageResultLayers::new(ImageResultId::Item(item));
    stack.add_layer(PaintLayer::new(PaintLayerId(9)));
    image.result_layers.push(stack);
    let source = h.app.doc_mut().scene.build_node(
        WorldRect::new(0.0, 0.0, 200.0, 100.0),
        NodeKind::Image(image),
    );
    let mut target = h.app.doc_mut().scene.build_node(
        WorldRect::new(400.0, 0.0, 100.0, 100.0),
        NodeKind::Shape(ShapeNode {
            shape: kind,
            fill: Some(Rgba::WHITE),
            stroke: Stroke {
                width: 3.0,
                ..Stroke::default()
            },
            corner: Corner::Square,
            sides: 5,
            phase_deg: 10.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    target.rotation_deg = 25.0;
    target.opacity = 0.7;
    let ids = (source.id, target.id);
    h.app.add_nodes(vec![source, target]);
    h.app
        .zoom_to_rect(WorldRect::new(-100.0, -100.0, 750.0, 300.0));
    h.frame();
    (h, ids.0, ids.1)
}
fn image(h: &Harness, id: NodeId) -> &ImageNode {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Image(image) => image,
        _ => panic!("not an image"),
    }
}
fn undo(h: &mut Harness) {
    let tab = h.app.tab_mut();
    assert!(tab.journal.undo(&mut tab.doc.scene));
}

#[test]
fn direct_fill_preserves_source_stack_and_target_geometry_in_one_undo() {
    let (mut h, source, target) = fixture("fill_ellipse", ShapeKind::Ellipse);
    let before = h.app.doc().scene.nodes.clone();
    let original_image = image(&h, source).clone();
    let original_target = h.app.doc().scene.node(target).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    assert!(h
        .app
        .fill_shape_with_image(target, original_image.clone(), 2.0, Some(source)));
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(h.app.doc().scene.node(source).is_none());
    let filled = h.app.doc().scene.node(target).unwrap();
    assert_eq!(filled.rect, original_target.rect);
    assert_eq!(filled.rotation_deg, original_target.rotation_deg);
    assert_eq!(filled.opacity, original_target.opacity);
    assert!(!slate_doc::scene::supports_corners(filled));
    assert!(filled
        .clip
        .as_ref()
        .unwrap()
        .segs
        .iter()
        .any(|seg| matches!(seg, PathSeg::Cubic { .. })));
    let mut expected = original_image;
    expected.crop = Crop {
        x: 0.25,
        y: 0.0,
        w: 0.5,
        h: 1.0,
    };
    expected.stroke = Stroke {
        width: 3.0,
        ..Stroke::default()
    };
    assert_eq!(image(&h, target), &expected);
    undo(&mut h);
    assert_eq!(h.app.doc().scene.nodes, before);
}

#[test]
fn actual_move_and_alt_drop_are_each_one_journal_group() {
    for alt in [false, true] {
        let (mut h, source, target) = fixture("shape_drag", ShapeKind::RegularPolygon);
        let before = h.app.doc().scene.nodes.clone();
        let depth = h.app.tab().journal.undo_depth();
        let mods = egui::Modifiers {
            alt,
            ..Default::default()
        };
        h.app.alt_down = alt;
        h.app.board_sel = std::iter::once(source).collect();
        let start = Pos2::new(100.0, 50.0);
        let end = Pos2::new(450.0, 50.0);
        let xf = h.app.board_xf();
        h.app.board_drag = h.app.begin_gesture_for_test(xf.w2s(start), start, mods);
        assert!(matches!(h.app.board_drag, Some(BoardDrag::Move { .. })));
        h.app.update_gesture_for_test(end, mods);
        assert!(h.app.image_drop_armed());
        h.app.end_gesture_for_test(end, Some(xf.w2s(end)), mods);
        assert!(matches!(
            h.app.doc().scene.node(target).unwrap().kind,
            NodeKind::Image(_)
        ));
        assert_eq!(h.app.doc().scene.node(source).is_some(), alt);
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
        undo(&mut h);
        assert_eq!(h.app.doc().scene.nodes, before);
    }
}

#[test]
fn file_intake_fills_directly_without_a_choice_popover() {
    let (mut h, source, target) = fixture("shape_file", ShapeKind::Rect);
    let item = image(&h, source).item;
    h.app.offer_image_file_drop(target, item, Pos2::ZERO);
    assert!(h.app.image_drop.is_none());
    assert!(h.app.image_drop_screen.is_none());
    assert_eq!(
        image(&h, target).crop,
        Crop {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0
        }
    );
    assert!(h.app.doc().scene.node(source).is_some());
}

#[test]
fn crop_pan_and_content_scale_leave_the_rotated_mask_fixed() {
    let (mut h, source, target) = fixture("shape_crop", ShapeKind::Ellipse);
    let source_image = image(&h, source).clone();
    assert!(h.app.fill_shape_with_image(target, source_image, 2.0, None));
    let before = h.app.doc().scene.node(target).unwrap().clone();
    h.app.board_double_click_for_test(Pos2::new(450.0, 50.0));
    assert_eq!(h.app.board_crop, Some(target));
    let start = Pos2::new(450.0, 50.0);
    let xf = h.app.board_xf();
    h.app.board_drag = h
        .app
        .begin_gesture_for_test(xf.w2s(start), start, egui::Modifiers::NONE);
    assert!(matches!(h.app.board_drag, Some(BoardDrag::CropPan { .. })));
    let end = start + egui::vec2(10.0, 0.0);
    h.app.update_gesture_for_test(end, egui::Modifiers::NONE);
    h.app
        .end_gesture_for_test(end, Some(xf.w2s(end)), egui::Modifiers::NONE);
    let panned = h.app.doc().scene.node(target).unwrap();
    assert_eq!(panned.rect, before.rect);
    assert_eq!(panned.clip, before.clip);
    assert_ne!(image(&h, target).crop.x, 0.25);
    undo(&mut h);
    assert_eq!(h.app.doc().scene.node(target).unwrap(), &before);
    let (rect, crop) = crate::app::board_crop::edge_drag_node(&before, 4, (700.0, 200.0));
    assert_eq!(rect, before.rect);
    let content = slate_doc::geom::image_content_rect(rect, crop);
    assert!((content.w / content.h - 2.0).abs() < 1e-4);
}

#[test]
fn read_only_and_open_paths_refuse_fill_and_command_uses_same_owner() {
    let (mut h, source, target) = fixture("shape_gates", ShapeKind::Ellipse);
    let original = image(&h, source).clone();
    h.app.tab_mut().read_only = true;
    assert!(!h
        .app
        .fill_shape_with_image(target, original.clone(), 2.0, None));
    h.app.tab_mut().read_only = false;
    let detail = format!(
        r#"{{"source":{},"target":{},"keep_source":true}}"#,
        source.0, target.0
    );
    assert!(h.app.shape_fill_command(Some(&detail)));
    assert!(h.app.doc().scene.node(source).is_some());
    let open = h.app.doc_mut().scene.build_node(
        WorldRect::new(600.0, 0.0, 100.0, 100.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: Stroke::default(),
            corner: Corner::Square,
            sides: 5,
            phase_deg: 0.0,
            flip: false,
            path: Some(std::sync::Arc::new(PathData {
                start: [0.0, 0.0],
                segs: vec![PathSeg::Line { to: [1.0, 1.0] }],
                ..Default::default()
            })),
            text: None,
        }),
    );
    let id = open.id;
    h.app.add_nodes(vec![open]);
    assert!(!h.app.fill_shape_with_image(id, original, 2.0, None));
}

#[test]
fn hosted_text_stays_with_the_mask_and_undo_restores_the_shape() {
    let (mut h, source, target) = fixture("shape_text", ShapeKind::Rect);
    h.app.patch_nodes(&[target], |node| {
        let NodeKind::Shape(shape) = &mut node.kind else {
            unreachable!()
        };
        let mut text = ShapeText::new(Rgba::BLACK);
        text.body = "Caption".into();
        shape.text = Some(text);
    });
    let cover_image = image(&h, source).clone();
    let cover = h.app.doc_mut().scene.build_node(
        WorldRect::new(400.0, 0.0, 100.0, 100.0),
        NodeKind::Image(cover_image),
    );
    let cover_id = cover.id;
    h.app.add_nodes(vec![cover]);
    let before = h.app.doc().scene.nodes.clone();
    let picture = image(&h, source).clone();
    assert!(h
        .app
        .fill_shape_with_image(target, picture, 2.0, Some(source)));
    let filled = h.app.doc().scene.node(target).unwrap();
    let target_index = h.app.doc().scene.index_of(target).unwrap();
    let overlay = &h.app.doc().scene.nodes[target_index + 1];
    assert_eq!(
        h.app.doc().scene.nodes.last().unwrap().id,
        cover_id,
        "hosted text stays below later nodes"
    );
    assert!(matches!(&overlay.kind, NodeKind::Text(text) if text.text == "Caption"));
    assert_eq!(filled.group, overlay.group);
    assert!(filled.group.is_some());
    assert_eq!(filled.clip, overlay.clip);
    assert_eq!(filled.rotation_deg, overlay.rotation_deg);
    undo(&mut h);
    assert_eq!(h.app.doc().scene.nodes, before);
}

#[test]
fn compound_path_holes_do_not_accept_a_picture_drop() {
    let (mut h, source, target) = fixture("shape_hole", ShapeKind::Ellipse);
    let mut path =
        slate_doc::geom::shape_image_clip(h.app.doc().scene.node(target).unwrap()).unwrap();
    path.extra.push(PathContour {
        start: [0.4, 0.4],
        segs: vec![
            PathSeg::Line { to: [0.6, 0.4] },
            PathSeg::Line { to: [0.6, 0.6] },
            PathSeg::Line { to: [0.4, 0.6] },
        ],
        closed: true,
    });
    h.app.patch_nodes(&[target], |node| {
        let NodeKind::Shape(shape) = &mut node.kind else {
            unreachable!()
        };
        shape.shape = ShapeKind::Path;
        shape.path = Some(std::sync::Arc::new(path.clone()));
        node.rotation_deg = 0.0;
    });
    h.app
        .update_image_drop_offer(Pos2::new(450.0, 50.0), &[source]);
    assert!(!h.app.image_drop_armed());
    h.app
        .update_image_drop_offer(Pos2::new(430.0, 50.0), &[source]);
    assert!(h.app.image_drop_armed());
    assert!(h.app.commit_image_drop(None, false));
    let filled = h.app.doc().scene.node(target).unwrap();
    assert_eq!(filled.clip.as_ref(), Some(&path));
    assert!(!slate_doc::geom::point_in_node_outline(
        filled, 450.0, 50.0, 0.05
    ));
}
