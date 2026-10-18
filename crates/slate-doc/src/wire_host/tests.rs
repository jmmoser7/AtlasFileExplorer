use super::*;
use crate::scene::{NodeId, NodeKind, PathData, PathSeg, ShapeKind, ShapeNode, Stroke, TextNode};
use crate::scene::{TextAlign, Typeface};

pub(super) fn shape_node(
    kind: ShapeKind,
    rect: WorldRect,
    rot: f32,
    path: Option<PathData>,
) -> Node {
    Node {
        id: NodeId(1),
        rect,
        rotation_deg: rot,
        opacity: 1.0,
        locked: false,
        hidden: false,
        group: None,
        clip: None,
        bumper: None,
        kind: NodeKind::Shape(ShapeNode {
            shape: kind,
            fill: None,
            stroke: Stroke::default(),
            corner: Default::default(),
            sides: crate::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: path.map(std::sync::Arc::new),
            text: None,
        }),
    }
}

#[test]
fn rotated_rect_top_follows_the_local_edge_not_the_aabb() {
    let rect = WorldRect::new(0.0, 0.0, 100.0, 40.0);
    let node = shape_node(ShapeKind::Rect, rect, 90.0, None);
    let host = WireHost::from_node(&node);
    let top = host.anchor(Side::Top, 0.5);
    // Local top mid (50, 0) rotated 90° CW about (50, 20) → (70, 20).
    assert!((top[0] - 70.0).abs() < 1e-4 && (top[1] - 20.0).abs() < 1e-4);
    let aabb_top = connector_anchor_point(rect, Side::Top, 0.5);
    assert!(
        dist(top, aabb_top) > 10.0,
        "rotated top must leave the AABB top, got {top:?} vs {aabb_top:?}"
    );
    let n = host.outward(Side::Top, 0.5);
    // Local (0, −1) rotated 90° CW → (1, 0).
    assert!((n[0] - 1.0).abs() < 1e-4 && n[1].abs() < 1e-4);
}

#[test]
fn saved_ellipse_anchors_lie_on_the_ellipse_after_rotation() {
    let rect = WorldRect::new(0.0, 0.0, 100.0, 40.0);
    let node = shape_node(ShapeKind::Ellipse, rect, 35.0, None);
    let host = WireHost::from_node(&node);
    let (cx, cy) = rect.center();
    let rx = rect.w * 0.5;
    let ry = rect.h * 0.5;
    assert!(host.ports().is_empty());
    for side in Side::BOX {
        let local = unrotate_pt(host.anchor(side, 0.5), cx, cy, 35.0);
        let nx = (local[0] - cx) / rx;
        let ny = (local[1] - cy) / ry;
        let resid = nx * nx + ny * ny;
        assert!(
            (resid - 1.0).abs() < 1e-4,
            "port {:?} not on ellipse (resid={resid})",
            side
        );
    }
}

#[test]
fn open_path_anchors_are_start_mid_end() {
    let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
    let path = PathData {
        start: [0.0, 0.0],
        segs: vec![PathSeg::Line { to: [1.0, 1.0] }],
        ..PathData::default()
    };
    let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
    let host = WireHost::from_node(&node);
    assert!(host.is_open());
    assert!(host.ports().is_empty(), "open shapes offer no ports");
    let start = host.anchor(Side::Start, 0.0);
    let mid = host.anchor(Side::Mid, 0.5);
    let end = host.anchor(Side::End, 1.0);
    assert!((start[0] - 0.0).abs() < 1e-4 && (start[1] - 0.0).abs() < 1e-4);
    assert!((mid[0] - 50.0).abs() < 1e-4 && (mid[1] - 25.0).abs() < 1e-4);
    assert!((end[0] - 100.0).abs() < 1e-4 && (end[1] - 50.0).abs() < 1e-4);
}

#[test]
fn legacy_line_anchors_follow_the_diagonal() {
    let rect = WorldRect::new(10.0, 20.0, 80.0, 40.0);
    let node = shape_node(ShapeKind::Line, rect, 0.0, None);
    let host = WireHost::from_node(&node);
    assert!(host.ports().is_empty(), "open shapes offer no ports");
    assert_eq!(host.anchor(Side::Start, 0.0), [10.0, 20.0]);
    assert_eq!(host.anchor(Side::End, 1.0), [90.0, 60.0]);
    let mid = host.anchor(Side::Mid, 0.5);
    assert!((mid[0] - 50.0).abs() < 1e-4);
    assert!((mid[1] - 40.0).abs() < 1e-4);
}

#[test]
fn old_box_side_on_a_line_snaps_to_a_stroke_feature() {
    let rect = WorldRect::new(0.0, 0.0, 100.0, 0.0);
    let path = PathData {
        start: [0.0, 0.0],
        segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
        ..PathData::default()
    };
    let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
    let host = WireHost::from_node(&node);
    // Historical Right mid of a degenerate line AABB is the end.
    let p = host.anchor(Side::Right, 0.5);
    assert!((p[0] - 100.0).abs() < 1e-3);
    let p = host.anchor(Side::Left, 0.5);
    assert!(p[0].abs() < 1e-3);
}

#[test]
fn text_and_unrotated_rect_share_oriented_mids() {
    let rect = WorldRect::new(0.0, 0.0, 80.0, 20.0);
    let text = Node {
        id: NodeId(2),
        rect,
        rotation_deg: 0.0,
        opacity: 1.0,
        locked: false,
        hidden: false,
        group: None,
        clip: None,
        bumper: None,
        kind: NodeKind::Text(TextNode {
            text: "hi".into(),
            style: Default::default(),
            family: Typeface::Sans,
            size: 14.0,
            color: crate::scene::Rgba::BLACK,
            align: TextAlign::Left,
            fill: None,
            stroke: Default::default(),
            agent: None,
        }),
    };
    let a = WireHost::from_node(&text).anchor(Side::Top, 0.5);
    let b = connector_anchor_point(rect, Side::Top, 0.5);
    assert_eq!(a, b);
}

#[test]
fn open_mid_outward_is_stable_left_normal() {
    let rect = WorldRect::new(0.0, 0.0, 100.0, 0.0);
    let path = PathData {
        start: [0.0, 0.0],
        segs: vec![PathSeg::Line { to: [1.0, 0.0] }],
        ..PathData::default()
    };
    let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
    let n = WireHost::from_node(&node).outward(Side::Mid, 0.5);
    // Tangent +x; left in screen axes is −y.
    assert!(n[0].abs() < 1e-4 && (n[1] + 1.0).abs() < 1e-4);
}

#[test]
fn closed_polyline_snaps_to_the_stroke_not_the_aabb() {
    // Right triangle (0,0)-(80,0)-(0,80). The AABB top-mid is (40, 0) on
    // the base; the AABB right-mid (80, 40) sits in empty space.
    let rect = WorldRect::new(0.0, 0.0, 80.0, 80.0);
    let path = PathData {
        start: [0.0, 0.0],
        segs: vec![
            PathSeg::Line { to: [1.0, 0.0] },
            PathSeg::Line { to: [0.0, 1.0] },
        ],
        closed: true,
        ..PathData::default()
    };
    let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
    let host = WireHost::from_node(&node);
    assert!(
        host.is_open(),
        "closed polylines must not use the AABB host"
    );
    let ghost = [80.0, 40.0];
    let snap = host.snap(ghost);
    let on_hypotenuse = (snap.point[0] + snap.point[1] - 80.0).abs() < 1.0;
    assert!(
        on_hypotenuse,
        "snap from the AABB edge must land on the closing seam, got {:?}",
        snap.point
    );
    assert!(
        dist(snap.point, ghost) > 15.0,
        "the AABB edge itself is not a closed-polyline feature"
    );
}
