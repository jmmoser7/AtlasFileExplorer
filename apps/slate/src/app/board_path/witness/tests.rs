use super::*;
use slate_doc::scene::{NodeId, PathFillRule, Rgba, ShapeNode, Stroke, StrokeSpan};

fn point(x: f32, y: f32) -> WorldPoint {
    WorldPoint::new(f64::from(x), f64::from(y)).unwrap()
}

fn source_node(rect: slate_doc::scene::WorldRect, rotation_deg: f32) -> Node {
    let stroke = Stroke {
        width: 6.0,
        softness: 0.35,
        stamp: true,
        texture: slate_doc::scene::BrushTexture::Pencil,
        color: Rgba([20, 40, 60, 220]),
        ..Stroke::default()
    };
    let span = |width, color| StrokeSpan {
        width,
        softness: 0.25,
        color: Rgba(color),
        texture: slate_doc::scene::BrushTexture::Pencil,
    };
    let mut path = PathData {
        start: [0.1, 0.2],
        segs: vec![
            PathSeg::Line { to: [0.5, 0.2] },
            PathSeg::Line { to: [0.5, 0.8] },
        ],
        fill_rule: PathFillRule::EvenOdd,
        tips: vec![
            span(2.0, [255, 0, 0, 255]),
            span(6.0, [0, 255, 0, 255]),
            span(10.0, [0, 0, 255, 255]),
        ],
        ..PathData::default()
    };
    path.extra.push(PathContour {
        start: [0.8, 0.8],
        segs: vec![PathSeg::Line { to: [0.9, 0.8] }],
        closed: false,
    });
    path.tips
        .extend([span(5.0, [255, 255, 0, 255]), span(8.0, [255, 0, 255, 255])]);
    Node {
        id: NodeId(42),
        rect,
        rotation_deg,
        opacity: 0.8,
        locked: false,
        hidden: false,
        group: None,
        clip: None,
        bumper: None,
        kind: NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: Some(Rgba([200, 100, 50, 255])),
            stroke,
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(path.into()),
            text: None,
        }),
    }
}

fn preview_path(node: &Node) -> PathData {
    let NodeKind::Shape(shape) = &node.kind else {
        panic!("expected path node")
    };
    shape.path.as_deref().unwrap().clone()
}

#[test]
fn actual_corner_prefix_keeps_vertices_and_does_not_cut_the_turn() {
    let source = source_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        0.0,
    );
    let preview = StrokePreview::new(&source, 0.1).unwrap();
    let node = preview.through(0, 1, point(55.0, 20.0), true, false);
    let path = preview_path(&node);
    assert_eq!(path.start, [0.1, 0.2]);
    assert_eq!(path.segs.len(), 2);
    assert_eq!(path.segs[0], PathSeg::Line { to: [0.5, 0.2] });
    assert_eq!(path.segs[1], PathSeg::Line { to: [0.55, 0.2] });
    assert!(!path.closed);
    assert!(path.extra.is_empty());
    assert_eq!(
        path.tips.len(),
        1 + path.segs.len(),
        "the active interpolated tip corresponds to the one appended path anchor"
    );
}

#[test]
fn pen_up_keeps_contours_disjoint_and_brush_style_tips_aligned() {
    let source = source_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        0.0,
    );
    let before = source.clone();
    let preview = StrokePreview::new(&source, 0.1).unwrap();
    let node = preview.through(1, 0, point(90.0, 80.0), false, false);
    let path = preview_path(&node);
    assert_eq!(
        path.segs,
        vec![
            PathSeg::Line { to: [0.5, 0.2] },
            PathSeg::Line { to: [0.5, 0.8] },
        ],
        "the contour before the pen-up is complete"
    );
    assert_eq!(path.extra.len(), 1);
    assert_eq!(path.extra[0].start, [0.8, 0.8]);
    assert!(path.extra[0].segs.is_empty());
    let preview_anchor_count = 1
        + path.segs.len()
        + path
            .extra
            .iter()
            .map(|contour| 1 + contour.segs.len())
            .sum::<usize>();
    assert_eq!(path.tips.len(), preview_anchor_count);
    let NodeKind::Shape(shape) = &node.kind else {
        unreachable!()
    };
    assert!(shape.fill.is_none());
    assert!(shape.stroke.stamp);
    assert_eq!(shape.stroke.texture, slate_doc::scene::BrushTexture::Pencil);
    assert_eq!(shape.stroke.width, 6.0);
    let mid = preview.through(0, 0, point(30.0, 20.0), true, false);
    assert!((preview_path(&mid).tips[1].width - 4.0).abs() < 1e-4);
    assert_eq!(source, before, "preview construction does not mutate input");
}

#[test]
fn world_contours_round_trip_through_rect_and_rotation_and_final_is_exact() {
    let source = source_node(
        slate_doc::scene::WorldRect::new(20.0, 30.0, 200.0, 100.0),
        90.0,
    );
    let original = source.clone();
    let preview = StrokePreview::new(&source, 0.1).unwrap();
    assert_eq!(preview.contours().len(), 2);
    let node = preview.through(0, 1, preview.contours()[0][1], false, false);
    let path = preview_path(&node);
    assert!((path.start[0] - 0.1).abs() < 1e-5);
    assert!((path.start[1] - 0.2).abs() < 1e-5);
    let PathSeg::Line { to } = path.segs[0] else {
        panic!("expected flattened line")
    };
    assert!((to[0] - 0.5).abs() < 1e-5);
    assert!((to[1] - 0.2).abs() < 1e-5);
    assert_eq!(
        preview.through(1, 0, point(0.0, 0.0), false, true),
        original
    );
}

#[test]
fn unsupported_shapes_invalid_tolerance_and_oversized_paths_are_rejected() {
    let source = source_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        0.0,
    );
    assert!(StrokePreview::new(&source, 0.0).is_none());
    assert!(StrokePreview::new(&source, f64::NAN).is_none());
    let mut unsupported = source.clone();
    if let NodeKind::Shape(shape) = &mut unsupported.kind {
        shape.shape = ShapeKind::Rect;
    }
    assert!(StrokePreview::new(&unsupported, 0.1).is_none());
    let mut large = source.clone();
    if let NodeKind::Shape(shape) = &mut large.kind {
        let mut data = shape.path.as_deref().unwrap().clone();
        data.segs = vec![PathSeg::Line { to: [0.2, 0.2] }; PREVIEW_VERTEX_LIMIT];
        shape.path = Some(data.into());
    }
    assert!(StrokePreview::new(&large, 0.1).is_none());
}

#[test]
fn world_coordinates_over_presence_limit_are_rejected_before_flattening() {
    let source = source_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 4_000_000_000.0, 100.0),
        0.0,
    );
    assert!(StrokePreview::new(&source, 0.1).is_none());
}

#[test]
fn uniform_brush_without_path_tips_gets_one_stroke_style_per_route_point() {
    let mut source = source_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        0.0,
    );
    let NodeKind::Shape(shape) = &mut source.kind else {
        unreachable!()
    };
    let mut path = shape.path.as_deref().unwrap().clone();
    path.tips.clear();
    shape.path = Some(path.into());
    let expected = StrokeSpan::of(&shape.stroke);

    let preview = StrokePreview::new(&source, 0.1).unwrap();
    let (points, tips) = preview.brush_contour(0).unwrap();
    assert_eq!(points.len(), tips.len());
    assert!(tips.iter().all(|tip| *tip == expected));
    assert_eq!(preview.brush_tip_at(0, points[1]), Some(expected));
}

#[test]
fn legacy_tween_without_path_tips_uses_canonical_tipped_contour() {
    let mut source = source_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 100.0, 100.0),
        0.0,
    );
    let NodeKind::Shape(shape) = &mut source.kind else {
        unreachable!()
    };
    let mut path = shape.path.as_deref().unwrap().clone();
    path.tips.clear();
    let mut from = StrokeSpan::of(&shape.stroke);
    from.width = 2.0;
    shape.stroke.tween_from = Some(from);
    shape.path = Some(path.into());
    let current_width = shape.stroke.width;

    let preview = StrokePreview::new(&source, 0.1).unwrap();
    let (points, tips) = preview.brush_contour(0).unwrap();
    assert_eq!(points.len(), tips.len());
    assert_eq!(tips.first().unwrap().width, 2.0);
    assert_eq!(tips.last().unwrap().width, current_width);
    let end_of_first_segment = points
        .iter()
        .position(|point| (point.x - 50.0).abs() < 0.01 && (point.y - 20.0).abs() < 0.01)
        .expect("canonical tipped contour retains the authored corner");
    assert!((tips[end_of_first_segment].width - current_width).abs() < 1e-4);
}

#[test]
fn parametric_closed_shapes_trace_the_shared_outline_and_keep_exact_source_on_finish() {
    for kind in [
        ShapeKind::Rect,
        ShapeKind::Ellipse,
        ShapeKind::RegularPolygon,
    ] {
        let mut source = source_node(
            slate_doc::scene::WorldRect::new(20.0, 30.0, 180.0, 90.0),
            27.0,
        );
        let NodeKind::Shape(shape) = &mut source.kind else {
            unreachable!()
        };
        shape.shape = kind;
        shape.sides = 7;
        shape.phase_deg = 13.0;
        let original = source.clone();
        let tolerance = 0.1_f64.max(90.0 / 4_096.0);
        let expected = slate_doc::geom::node_closed_poly(&source, tolerance as f32).unwrap();

        let preview = StrokePreview::new(&source, 0.1).unwrap();
        assert_eq!(preview.contours().len(), 1);
        assert_eq!(preview.contours()[0].len(), expected[0].len() + 1);
        for (actual, expected) in preview.contours()[0].iter().zip(&expected[0]) {
            assert!((actual.x - f64::from(expected[0])).abs() < 1e-5);
            assert!((actual.y - f64::from(expected[1])).abs() < 1e-5);
        }
        assert_eq!(preview.contours()[0].first(), preview.contours()[0].last());

        let partial = preview.through(0, 2, preview.contours()[0][2], false, false);
        let NodeKind::Shape(shape) = &partial.kind else {
            unreachable!()
        };
        assert_eq!(shape.shape, ShapeKind::Path);
        assert!(shape.fill.is_none());
        assert_eq!(
            shape.path.as_deref().unwrap().segs.len(),
            2,
            "the preview contains the traced outline prefix"
        );
        assert_eq!(
            preview.through(0, 0, point(0.0, 0.0), false, true),
            original,
            "completion restores the exact parameterized recipe node for {kind:?}"
        );
    }
}

#[test]
fn parametric_outline_rejects_world_points_outside_the_bounded_route_domain() {
    let mut source = source_node(
        slate_doc::scene::WorldRect::new(0.0, 0.0, 4_000_000_000.0, 100.0),
        0.0,
    );
    let NodeKind::Shape(shape) = &mut source.kind else {
        unreachable!()
    };
    shape.shape = ShapeKind::Ellipse;
    assert!(StrokePreview::new(&source, 0.1).is_none());
}
