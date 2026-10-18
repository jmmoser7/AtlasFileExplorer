//! Feedback regressions: gradient arrowheads and transient intent curves.
use super::*;

#[test]
fn ctrl_shift_field_pick_routes_vertex_property_command_to_fill_only() {
    use slate_doc::scene::*;
    let mut h = grip_board("field_command_route");
    let center = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let id = add_filled_rect(&mut h.app, center.x - 100.0, center.y - 80.0, 200.0, 160.0);
    if let NodeKind::Shape(shape) = &mut h.app.doc_mut().scene.node_mut(id).unwrap().kind {
        shape.stroke = Stroke {
            width: 12.0,
            color: Rgba::BLACK,
            ..Default::default()
        };
    }
    h.app.board_sel.clear();
    h.frame();
    let screen = h.app.board_xf().w2s(center);
    press_primary(&mut h, screen, SUB_OBJECT);
    assert_eq!(h.app.field_selection(), Some(id));
    let corner = h.app.board_xf().w2s(center - EVec2::new(100.0, 80.0));
    press_primary(&mut h, corner, SUB_OBJECT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0])));
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    let request = board_properties::PropertyRequest {
        ids: vec![id],
        points: vec![0],
        edits: vec![board_properties::Property::FillRgb([255, 0, 0])],
    };
    assert!(h
        .app
        .shape_property_command(Some(&serde_json::to_string(&request).unwrap())));
    let NodeKind::Shape(shape) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("shape")
    };
    let NodeKind::Shape(old) = &before.kind else {
        panic!("shape")
    };
    assert_eq!(shape.stroke, old.stroke);
    let colors = slate_doc::field::colors(shape).unwrap();
    assert_eq!(&colors[0].0[..3], &[255, 0, 0]);
    assert_eq!(colors[1], Rgba::WHITE);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
}

#[test]
fn fade_curve_creation_never_mutates_scene_journal_or_dirty_state() {
    let mut h = grip_board("fade_is_intent_ink");
    h.app.tab_mut().dirty = false;
    let count = h.app.doc().scene.nodes.len();
    let depth = h.app.tab().journal.undo_depth();
    h.app.set_curve_fade_armed(Some(true));
    let path = slate_doc::scene::PathData {
        start: [0.0, 0.0],
        segs: vec![slate_doc::scene::PathSeg::Line { to: [1.0, 1.0] }],
        ..Default::default()
    };
    let ids = h.app.commit_drawn_path(
        slate_doc::StrokeTool::Polyline,
        slate_doc::scene::WorldRect::new(10.0, 10.0, 100.0, 100.0),
        path,
        false,
        board_path::DrawnTips::None,
    );
    assert!(ids.is_empty());
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    assert!(!h.app.tab().dirty);
    assert_eq!(h.app.direct.fading.len(), 1);
}

#[test]
fn arrowhead_retains_curve_gradient_and_authored_independent_size() {
    use slate_doc::scene::*;
    let mut doc = slate_doc::SlateDoc::new("Arrow gradient");
    let stroke = Stroke {
        width: 10.0,
        color: Rgba([0, 0, 255, 255]),
        arrow_end: true,
        arrow_sizes: [None, Some([40.0, 60.0])],
        ..Default::default()
    };
    let path = PathData {
        start: [0.0, 0.0],
        segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
        tips: vec![
            StrokeSpan {
                width: 10.0,
                color: Rgba([255, 0, 0, 255]),
                ..StrokeSpan::of(&stroke)
            },
            StrokeSpan::of(&stroke),
        ],
        ..Default::default()
    };
    let shape = ShapeNode {
        shape: ShapeKind::Path,
        fill: None,
        stroke,
        corner: Corner::Square,
        phase_deg: 0.0,
        flip: false,
        path: Some(path.clone().into()),
        text: None,
        sides: 6,
    };
    let node = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 10.0),
        NodeKind::Shape(shape.clone()),
    );
    let ink = board_path::vector_stroke_ink(&node, &shape, &path, 1.0);
    assert!(
        board_path::hit_path_node(&node, &shape, 65.0, 24.0, 1.0),
        "independent wing is pickable outside the host width"
    );
    assert!(
        !board_path::hit_path_node(&node, &shape, 80.0, 29.0, 1.0),
        "empty space past the sloping wing remains empty"
    );
    let mut large = node.clone();
    if let NodeKind::Shape(shape) = &mut large.kind {
        shape.stroke.arrow_sizes[1] = Some([4000.0, 6000.0]);
    }
    assert!(
        slate_doc::node_aabb(&large).contains(-1000.0, 600.0),
        "head larger than stroke limits remains in spatial queries"
    );
    let colors = &ink.colors[ink.colors.len() - 6..];
    assert_ne!(colors[0], colors[1], "the base continues the host gradient");
    doc.scene.apply(&SceneCmd::Add { index: 0, node });
    let html = slate_artifact::render_html(&doc, &slate_artifact::AssetMap::default());
    assert!(html.contains("linearGradient id=\"arrow"));
    assert!(html.contains("L 60.0 -30.0") || html.contains("L 60.0 30.0"));
}

#[test]
fn field_gradient_exports_before_its_centered_stroke() {
    use slate_doc::scene::*;
    let mut doc = slate_doc::SlateDoc::new("Field stacking");
    let mut shape = ShapeNode {
        shape: ShapeKind::Rect,
        sides: 4,
        phase_deg: 0.0,
        fill: Some(Rgba::WHITE),
        stroke: Stroke {
            width: 8.0,
            color: Rgba::BLACK,
            ..Default::default()
        },
        corner: Corner::Square,
        flip: false,
        path: None,
        text: None,
    };
    slate_doc::field::edit_colors(&mut shape, &[0], |color| *color = Rgba([255, 0, 0, 255]));
    let node = doc.scene.build_node(
        WorldRect::new(0.0, 0.0, 100.0, 100.0),
        NodeKind::Shape(shape),
    );
    doc.scene.apply(&SceneCmd::Add { index: 0, node });
    let html = slate_artifact::render_html(&doc, &slate_artifact::AssetMap::default());
    let fill = html
        .find("linearGradient id=\"field")
        .expect("authored field");
    let stroke = html[fill..]
        .find("stroke-width=\"8.0\"")
        .map(|offset| fill + offset)
        .expect("stroke follows field");
    assert!(fill < stroke);
}

#[test]
fn parent_field_selection_does_not_hide_a_nested_workbooks_same_id_stroke() {
    let mut h = grip_board("field_board_namespace");
    let id = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    h.app.direct.field = Some(id);
    let parent = h.app.doc().scene.node(id).unwrap().clone();
    let before = h.app.field_paint_node(&parent).unwrap();
    let repeat = h.app.field_paint_node(&parent).unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&before, &repeat),
        "unchanged field paint reuses its transient node"
    );
    let mut preview = parent.clone();
    preview.opacity = 0.5;
    let changed = h.app.field_paint_node(&preview).unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&before, &changed),
        "property preview changes invalidate field paint"
    );
    h.app.set_slate_nesting_for_test(true);
    assert!(
        h.app.field_paint_node(&parent).is_none(),
        "nested scene owns its stroke even when node identities coincide"
    );
    h.app.set_slate_nesting_for_test(false);
    assert!(h.app.field_paint_node(&parent).is_some());
}
