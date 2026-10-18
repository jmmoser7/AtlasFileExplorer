use super::{
    prepare::{
        init_smooth_polyline, rebuild_smooth_vector_node, route_segment_centers, smooth_contour_at,
    },
    prepare_smooth,
};
use eframe::egui::Pos2;
use slate_doc::scene::{Node, NodeKind, Scene, ShapeKind, ShapeNode, Stroke, WorldRect};

fn line() -> Node {
    Scene::default().build_node(
        WorldRect::new(0.0, 0.0, 120.0, 0.0),
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Line,
            fill: None,
            stroke: Stroke::default(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    )
}

#[test]
fn prepared_result_matches_the_human_pass_kernel_and_does_not_mutate_source() {
    let source = line();
    let original = source.clone();
    let route = [Pos2::new(28.0, -10.0), Pos2::new(91.0, 10.0)];
    let width = 24.0;
    let strength = 0.65;
    let zoom = 1.25;

    let prepared = prepare_smooth(&[source.clone()], &route, width, strength, zoom).unwrap();
    let mut curve = init_smooth_polyline(&source).unwrap();
    let radius = width * 0.5;
    let mut centers = vec![[route[0].x, route[0].y]];
    centers.extend(route_segment_centers(route[0], route[1], radius, false));
    for contour in &mut curve.contours {
        for center in &centers {
            smooth_contour_at(contour, *center, radius, strength, zoom);
        }
    }
    let human_kernel = rebuild_smooth_vector_node(&source, &curve, zoom).unwrap();

    assert_eq!(prepared, vec![human_kernel]);
    assert_eq!(source, original);
}

#[test]
fn untouched_targets_are_local_and_excessive_route_work_is_refused() {
    let source = line();
    let far_route = [Pos2::new(500.0, 500.0)];
    assert!(
        prepare_smooth(&[source.clone()], &far_route, 12.0, 0.5, 1.0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(source.rect, WorldRect::new(0.0, 0.0, 120.0, 0.0));

    let long_route = [Pos2::new(0.0, 0.0), Pos2::new(29_000.0, 0.0)];
    let err = prepare_smooth(&[source], &long_route, 0.5, 1.0, 1.0).unwrap_err();
    assert!(err.contains("8192 brush passes"));
}
