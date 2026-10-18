use super::*;
use crate::app::tests::Harness;
use serde_json::json;
use slate_doc::scene::StrokeSpan;

fn host() -> Harness {
    let mut h = Harness::new("targeted_drawing");
    h.app.ensure_work_tab();
    h.app.at_home = false;
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h.frame();
    h
}

#[test]
fn cubic_input_keeps_authored_control_points_as_a_curve() {
    let points = [
        Pos2::new(0.0, 0.0),
        Pos2::new(10.0, 0.0),
        Pos2::new(10.0, 20.0),
        Pos2::new(30.0, 20.0),
    ];
    let (_, path) = board_path::explicit_bezier_path(&points, false).unwrap();
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

#[test]
fn arc_draw_uses_shared_result_and_rejects_collinear_points() {
    let h = host();
    let args = json!({"strokes":[{"tool":"arc","points":[[0,0],[20,0],[10,10]]}]});
    let plan = h
        .app
        .prepare_targeted_command_with_context(&h.ctx, "board.paths.draw", &[], &args)
        .unwrap();
    let SceneCmd::Add { node, .. } = &plan.cmds[0] else {
        panic!("arc should add a path")
    };
    assert!(
        matches!(&node.kind, NodeKind::Shape(shape) if shape.shape == slate_doc::scene::ShapeKind::Path && shape.path.as_ref().is_some_and(|p| !p.is_empty()))
    );
    let degenerate = json!({"strokes":[{"tool":"arc","points":[[0,0],[20,0],[10,0]]}]});
    assert!(h
        .app
        .prepare_targeted_command_with_context(&h.ctx, "board.paths.draw", &[], &degenerate)
        .is_err());
}

#[test]
fn human_arc_result_keeps_the_collinear_line_fallback() {
    let mut scene = slate_doc::Scene::default().node_preparation();
    let tip = slate_doc::vertex_style::PlacedTip {
        width: 2.0,
        color: slate_doc::scene::Rgba::BLACK,
        opacity: 1.0,
    };
    let node = board_path::arc_path_node(
        &mut scene,
        [
            Pos2::new(0.0, 0.0),
            Pos2::new(20.0, 0.0),
            Pos2::new(10.0, 0.0),
        ],
        board_path::default_curve_stroke(tip.color),
        tip,
        [tip; 3],
    );
    assert!(!board_path::arc_points_define_circle([
        Pos2::new(0.0, 0.0),
        Pos2::new(20.0, 0.0),
        Pos2::new(10.0, 0.0),
    ]));
    assert!(
        matches!(&node.kind, NodeKind::Shape(shape) if shape.path.as_ref().is_some_and(|path| matches!(path.segs.as_slice(), [slate_doc::scene::PathSeg::Line { .. }])))
    );
}

#[test]
fn parametric_shape_draw_retains_recipe_node_kinds_and_bounded_sides() {
    let h = host();
    for (tool, kind) in [
        ("rect", slate_doc::scene::ShapeKind::Rect),
        ("ellipse", slate_doc::scene::ShapeKind::Ellipse),
        ("polygon", slate_doc::scene::ShapeKind::RegularPolygon),
    ] {
        let args = if tool == "polygon" {
            json!({"strokes":[{"tool":tool,"points":[[20,30],[0,0]],"sides":7}]})
        } else {
            json!({"strokes":[{"tool":tool,"points":[[20,30],[0,0]]}]})
        };
        let plan = h
            .app
            .prepare_targeted_command_with_context(&h.ctx, "board.paths.draw", &[], &args)
            .unwrap();
        let SceneCmd::Add { node, .. } = &plan.cmds[0] else {
            panic!("shape should add one node")
        };
        assert!(matches!(&node.kind, NodeKind::Shape(shape) if shape.shape == kind));
        if tool == "polygon" {
            assert!(matches!(&node.kind, NodeKind::Shape(shape) if shape.sides == 7));
        }
    }
    let invalid = json!({"strokes":[{"tool":"polygon","points":[[0,0],[20,20]],"sides":129}]});
    assert!(h
        .app
        .prepare_targeted_command_with_context(&h.ctx, "board.paths.draw", &[], &invalid)
        .is_err());
}
