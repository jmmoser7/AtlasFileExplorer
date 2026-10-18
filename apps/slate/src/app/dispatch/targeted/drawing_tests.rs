use super::*;
use slate_doc::scene::StrokeSpan;

#[test]
fn cubic_input_keeps_authored_control_points_as_a_curve() {
    let points = [
        Pos2::new(0.0, 0.0),
        Pos2::new(10.0, 0.0),
        Pos2::new(10.0, 20.0),
        Pos2::new(30.0, 20.0),
    ];
    let (_, path) = bezier_path(&points, false).unwrap();
    assert!(matches!(
        path.segs.as_slice(),
        [slate_doc::scene::PathSeg::Cubic { .. }]
    ));
    assert!(!path.closed);
}

#[test]
fn point_parser_rejects_non_finite_and_out_of_bounds_values() {
    for value in [
        serde_json::json!([[0.0, 0.0], ["NaN", 1.0]]),
        serde_json::json!([[0.0, 0.0], [1.0e12, 1.0]]),
    ] {
        assert!(points(&value, 8, 2).is_err());
    }
}

#[test]
fn eraser_sweep_budget_rejects_extreme_sample_and_geometry_work() {
    let path = [Pos2::new(0.0, 0.0), Pos2::new(10_000.0, 0.0)];
    let dense = board_color::eraser_sweep_sample_count(&path, 0.1);
    let ordinary = board_color::eraser_sweep_sample_count(&path, 100.0);
    assert!(dense > board_color::MAX_ERASER_SWEEP_SAMPLES);
    assert!(ordinary <= board_color::MAX_ERASER_SWEEP_SAMPLES);
    assert!(board_color::next_eraser_sweep_work(0, 8192, 300).is_none());
    assert_eq!(
        board_color::next_eraser_sweep_work(0, 8192, 200),
        Some(1_638_400)
    );
}

#[test]
fn shared_path_result_is_invertible_and_uses_a_preparation_scene() {
    let original = slate_doc::Scene::default();
    let mut scratch = original.node_preparation();
    let (rect, path) =
        board_path::points_to_path_data(&[Pos2::new(1.0, 2.0), Pos2::new(20.0, 12.0)], false);
    let node = board_path::drawn_path_node(
        &mut scratch,
        rect,
        path,
        false,
        board_path::default_curve_stroke(slate_doc::scene::Rgba::BLACK),
        slate_doc::vertex_style::PlacedTip {
            width: 2.0,
            color: slate_doc::scene::Rgba::BLACK,
            opacity: 0.75,
        },
        None,
        board_path::DrawnTips::None,
    );
    let add = SceneCmd::Add { index: 0, node };
    let mut scene = original.clone();
    assert!(scene.apply(&add));
    assert_eq!(scene.nodes[0].opacity, 0.75);
    assert!(scene.apply(&add.inverted()));
    assert_eq!(scene.nodes, original.nodes);
    assert_eq!(
        original.nodes.len(),
        0,
        "preparation must not mutate the live scene"
    );
}

#[test]
fn shared_brush_result_keeps_stamp_and_soft_tip_style() {
    let mut scene = slate_doc::Scene::default().node_preparation();
    let (rect, path) =
        board_path::points_to_path_data(&[Pos2::new(5.0, 7.0), Pos2::new(30.0, 19.0)], false);
    let tip = StrokeSpan {
        width: 14.0,
        softness: 0.42,
        color: slate_doc::scene::Rgba([20, 40, 60, 190]),
        texture: slate_doc::scene::BrushTexture::Smooth,
    };
    let style = slate_doc::scene::Stroke {
        stamp: true,
        ..board_path::default_curve_stroke(tip.color)
    };
    let node = board_path::brush_path_node(&mut scene, rect, path, style, vec![tip]).unwrap();
    let NodeKind::Shape(shape) = node.kind else {
        panic!("path result should be a shape");
    };
    assert!(shape.stroke.paints_as_stamp());
    assert_eq!(shape.stroke.width, tip.width);
    assert_eq!(shape.stroke.softness, tip.softness);
    assert_eq!(shape.stroke.color, tip.color);
}
