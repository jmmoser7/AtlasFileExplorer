use super::*;
use slate_doc::scene::{ConnectorEnd, ConnectorNode, FrameNode, ImageNode, Scene, SceneCmd};
use slate_doc::{ItemId, WireRouting};

fn filled_rect() -> ShapeNode {
    ShapeNode {
        shape: ShapeKind::Rect,
        fill: Some(Rgba::WHITE),
        stroke: Stroke::none(),
        corner: slate_doc::scene::Corner::Square,
        sides: slate_doc::scene::default_regular_sides(),
        phase_deg: 0.0,
        flip: false,
        path: None,
        text: None,
    }
}

fn rectangle(rect: WorldRect) -> Node {
    Scene::default().build_node(rect, NodeKind::Shape(filled_rect()))
}

fn shape(node: &Node) -> &ShapeNode {
    let NodeKind::Shape(shape) = &node.kind else {
        panic!("expected a shape");
    };
    shape
}

// The unchanged pre-optimization branch for shapes without an authored path.
fn outline_reference(node: &Node, point: [f32; 2], zoom: f32) -> bool {
    slate_doc::geom::point_in_node_outline(node, point[0], point[1], 0.25)
        || hit_shape_stroke(node, shape(node), point[0], point[1], zoom)
}

#[test]
fn filled_rect_boundary_and_distant_grid_match_precise_outline_at_each_zoom() {
    for rect in [
        WorldRect::new(0.0, 0.0, 100.0, 80.0),
        WorldRect::new(-483.5, -26.0, 27.0, 191.0),
        WorldRect::new(0.0001, -0.0002, 0.0003, 0.0004),
        WorldRect::new(1.0e9, -1.0e9, 32768.0, 65536.0),
    ] {
        let node = rectangle(rect);
        for zoom in [0.02, 0.125, 1.0, 8.0, 128.0] {
            let slop = pick_slop_world(zoom);
            let xs = [
                rect.x - 4.0 * slop,
                rect.x - slop,
                rect.x,
                rect.x + rect.w * 0.5,
                rect.x + rect.w,
                rect.x + rect.w + slop,
                rect.x + rect.w + 4.0 * slop,
            ];
            let ys = [
                rect.y - 4.0 * slop,
                rect.y - slop,
                rect.y,
                rect.y + rect.h * 0.5,
                rect.y + rect.h,
                rect.y + rect.h + slop,
                rect.y + rect.h + 4.0 * slop,
            ];
            for x in xs {
                for y in ys {
                    assert_eq!(
                        hit_path_node(&node, shape(&node), x, y, zoom),
                        outline_reference(&node, [x, y], zoom),
                        "rect={rect:?}, point={x},{y}, zoom={zoom}"
                    );
                }
            }
        }
    }
}

#[test]
fn nonfinite_and_overflowing_rect_inputs_retain_precise_fallback() {
    let mut node = rectangle(WorldRect::new(0.0, 0.0, 100.0, 80.0));
    for (rect, point) in [
        (WorldRect::new(f32::MAX, 0.0, f32::MAX, 80.0), [50.0, 20.0]),
        (WorldRect::new(-f32::MAX, 0.0, 100.0, 80.0), [50.0, 20.0]),
        (WorldRect::new(0.0, 0.0, 100.0, 80.0), [f32::INFINITY, 20.0]),
        (WorldRect::new(0.0, 0.0, 100.0, 80.0), [f32::NAN, 20.0]),
        (WorldRect::new(0.0, 0.0, 0.0, 80.0), [0.0, 20.0]),
    ] {
        node.rect = rect;
        assert_eq!(
            hit_path_node(&node, shape(&node), point[0], point[1], 1.0),
            outline_reference(&node, point, 1.0)
        );
    }
}

#[test]
fn distant_rects_preserve_topmost_body_filter_and_locked_picks() {
    let mut scene = Scene::default();
    let rect = WorldRect::new(40.0, -10.0, 20.0, 20.0);
    let behind = add(&mut scene, rect, image());
    let top = add(&mut scene, rect, NodeKind::Shape(filled_rect()));
    for i in 0..256 {
        add(
            &mut scene,
            WorldRect::new(2000.0 + i as f32 * 40.0, 2000.0, 20.0, 20.0),
            NodeKind::Shape(filled_rect()),
        );
    }
    assert_eq!(board_pick_node(&scene, 50.0, 0.0, 1.0), Some(top));
    assert_eq!(filtered(&scene, false, |n| n.id != top), Some(behind));
    scene.node_mut(top).unwrap().locked = true;
    assert_eq!(filtered(&scene, false, |_| true), Some(behind));
    assert_eq!(filtered(&scene, true, |_| true), Some(top));
    scene.node_mut(top).unwrap().hidden = true;
    assert_eq!(filtered(&scene, true, |_| true), Some(behind));
}

#[test]
fn rotated_and_rounded_rects_keep_their_precise_visible_regions() {
    let mut node = rectangle(WorldRect::new(0.0, 0.0, 100.0, 20.0));
    node.rotation_deg = 90.0;
    let point = [50.0, -20.0];
    assert!(!node.rect.contains(point[0], point[1]));
    assert!(hit_path_node(&node, shape(&node), point[0], point[1], 1.0));
    assert_eq!(
        hit_path_node(&node, shape(&node), point[0], point[1], 1.0),
        outline_reference(&node, point, 1.0)
    );
    node.rotation_deg = 0.0;
    let NodeKind::Shape(s) = &mut node.kind else {
        unreachable!()
    };
    s.corner = slate_doc::scene::Corner::Rounded { radius: 10.0 };
    for (point, expected) in [([0.5, 0.5], false), ([50.0, 10.0], true)] {
        assert_eq!(
            hit_path_node(&node, shape(&node), point[0], point[1], 1.0),
            expected
        );
        assert_eq!(
            hit_path_node(&node, shape(&node), point[0], point[1], 1.0),
            outline_reference(&node, point, 1.0)
        );
    }
}

fn outside_loop() -> PathData {
    PathData {
        start: [-1.0, -1.0],
        segs: vec![
            PathSeg::Line { to: [2.0, -1.0] },
            PathSeg::Line { to: [2.0, 2.0] },
            PathSeg::Line { to: [-1.0, 2.0] },
        ],
        closed: true,
        ..Default::default()
    }
}

#[test]
fn clips_and_actual_authored_paths_outside_bounds_are_not_rejected() {
    let mut node = rectangle(WorldRect::new(0.0, 0.0, 100.0, 100.0));
    let point = [-50.0, 50.0];
    assert!(!node.rect.contains(point[0], point[1]));
    node.clip = Some(outside_loop());
    assert!(hit_path_node(&node, shape(&node), point[0], point[1], 1.0));
    assert_eq!(
        hit_path_node(&node, shape(&node), point[0], point[1], 1.0),
        outline_reference(&node, point, 1.0)
    );
    node.clip = None;
    let NodeKind::Shape(s) = &mut node.kind else {
        unreachable!()
    };
    s.shape = ShapeKind::Path;
    s.path = Some(outside_loop().into());
    assert!(hit_path_node(&node, shape(&node), point[0], point[1], 1.0));
    // The supplied paint shape must not hide the actual node's expanded outline.
    assert!(hit_path_node(
        &node,
        &filled_rect(),
        point[0],
        point[1],
        1.0
    ));
}

#[test]
fn transparent_positive_width_stroke_keeps_outside_geometry_and_pick_slop() {
    let mut node = rectangle(WorldRect::new(0.0, 0.0, 20.0, 20.0));
    let NodeKind::Shape(s) = &mut node.kind else {
        unreachable!()
    };
    s.stroke.width = 20.0;
    s.stroke.color = Rgba([0, 0, 0, 0]);
    s.path = Some(
        PathData {
            start: [-2.0, 0.5],
            segs: vec![PathSeg::Line { to: [3.0, 0.5] }],
            ..Default::default()
        }
        .into(),
    );
    for zoom in [0.125, 1.0, 8.0] {
        let point = [-20.0, 20.0 + pick_slop_world(zoom) * 0.5];
        assert!(!node.rect.contains(point[0], point[1]));
        assert!(hit_shape_stroke(
            &node,
            shape(&node),
            point[0],
            point[1],
            zoom
        ));
        assert!(hit_path_node(&node, shape(&node), point[0], point[1], zoom));
    }
}

fn add(scene: &mut Scene, rect: WorldRect, kind: NodeKind) -> NodeId {
    let node = scene.build_node(rect, kind);
    let id = node.id;
    scene.apply(&SceneCmd::Add {
        index: scene.nodes.len(),
        node,
    });
    id
}

fn image() -> NodeKind {
    NodeKind::Image(ImageNode::new(ItemId(1)))
}

fn connector(a: ConnectorEnd, b: ConnectorEnd) -> NodeKind {
    NodeKind::Connector(ConnectorNode {
        a,
        b,
        stroke: Stroke {
            width: 2.0,
            color: Rgba([0, 0, 0, 255]),
            ..Default::default()
        },
        color: None,
        routing: None,
        arrow_a: false,
        arrow_b: false,
        label: None,
        display: Default::default(),
        binding: None,
        crosstalk: None,
    })
}

fn frame() -> NodeKind {
    NodeKind::Frame(FrameNode {
        title: "Slide".into(),
        order: 0,
        fill: Rgba::WHITE,
        fill_authored: false,
        assignments: Default::default(),
        stroke: Stroke::none(),
        corner: Default::default(),
    })
}

fn filtered(scene: &Scene, locked: bool, accepts: impl Fn(&Node) -> bool) -> Option<NodeId> {
    board_pick_node_filtered(
        scene,
        [50.0, 0.0],
        1.0,
        locked,
        WireRouting::Bezier,
        accepts,
        slate_doc::scene_wire_hosts(scene),
    )
}

#[test]
fn excluded_authored_body_falls_through_to_the_visible_node_behind_it() {
    let mut scene = Scene::default();
    let rect = WorldRect::new(40.0, -10.0, 20.0, 20.0);
    let back = add(&mut scene, rect, image());
    let carried = add(&mut scene, rect, image());
    assert_eq!(board_pick_node(&scene, 50.0, 0.0, 1.0), Some(carried));
    assert_eq!(filtered(&scene, false, |n| n.id != carried), Some(back));
    assert_eq!(filtered(&scene, false, |_| false), None);
}

#[test]
fn transient_filter_applies_to_frame_fallback_and_locked_escape_hatch() {
    let mut scene = Scene::default();
    let rect = WorldRect::new(0.0, -30.0, 100.0, 60.0);
    let back = add(&mut scene, rect, frame());
    let locked = add(&mut scene, rect, frame());
    scene.node_mut(locked).unwrap().locked = true;
    assert_eq!(filtered(&scene, false, |_| true), Some(back));
    assert_eq!(filtered(&scene, true, |_| true), Some(locked));
    assert_eq!(filtered(&scene, true, |n| n.id != locked), Some(back));
    assert_eq!(filtered(&scene, false, |n| n.id != back), None);
    scene.node_mut(locked).unwrap().hidden = true;
    assert_eq!(filtered(&scene, true, |n| n.id != back), None);
}

#[test]
fn excluded_body_does_not_occlude_wire_but_excluded_wire_still_cannot_be_hit() {
    let mut scene = Scene::default();
    let conn = connector(
        ConnectorEnd::Free { point: [0.0, 0.0] },
        ConnectorEnd::Free {
            point: [100.0, 0.0],
        },
    );
    let wire = add(&mut scene, WorldRect::new(0.0, 0.0, 100.0, 0.0), conn);
    let carried = add(&mut scene, WorldRect::new(45.0, -5.0, 10.0, 10.0), image());
    assert_eq!(filtered(&scene, false, |_| true), Some(carried));
    assert_eq!(filtered(&scene, false, |n| n.id != carried), Some(wire));
    assert_eq!(filtered(&scene, false, |_| false), None);
}

#[test]
fn attached_wire_picks_its_visible_preview_route_and_not_its_old_authored_span() {
    let mut scene = Scene::default();
    let source = add(&mut scene, WorldRect::new(0.0, 0.0, 40.0, 40.0), image());
    let target = add(&mut scene, WorldRect::new(200.0, 0.0, 40.0, 40.0), image());
    let conn = connector(
        ConnectorEnd::Anchored {
            node: source,
            side: slate_doc::scene::Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: target,
            side: slate_doc::scene::Side::Left,
            t: 0.5,
        },
    );
    let wire = add(&mut scene, WorldRect::new(40.0, 20.0, 160.0, 0.0), conn);
    for routing in [WireRouting::Bezier, WireRouting::Orthogonal] {
        let pick = |point| {
            board_pick_node_filtered(
                &scene,
                point,
                1.0,
                false,
                routing,
                |node| node.id != source && node.id != target,
                |id| {
                    scene.node(id).filter(|node| !node.hidden).map(|node| {
                        let mut host = slate_doc::WireHost::from_node(node);
                        host.rect.y += 100.0;
                        host
                    })
                },
            )
        };
        assert_eq!(pick([120.0, 120.0]), Some(wire), "visible {routing:?} span");
        assert_eq!(pick([120.0, 20.0]), None, "old {routing:?} span is absent");
        assert_eq!(
            board_pick_node_routed(&scene, 120.0, 20.0, 1.0, false, routing),
            Some(wire)
        );
    }
    assert_eq!(scene.node(source).unwrap().rect.y, 0.0);
    assert_eq!(scene.node(target).unwrap().rect.y, 0.0);
}
