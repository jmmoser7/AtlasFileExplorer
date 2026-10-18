//! New-connection eligibility preserves saved shape anchors.
use super::tests::shape_node;
use super::*;

#[test]
fn only_rectangular_shapes_offer_ports_and_saved_path_anchors_still_resolve() {
    let rect = WorldRect::new(0.0, 0.0, 100.0, 50.0);
    let polyline = |closed: bool| PathData {
        start: [0.0, 0.0],
        segs: vec![
            PathSeg::Line { to: [1.0, 0.0] },
            PathSeg::Line { to: [0.0, 1.0] },
        ],
        closed,
        ..PathData::default()
    };
    let cubic = |closed: bool| PathData {
        start: [0.0, 0.0],
        segs: vec![PathSeg::Cubic {
            c1: [0.3, -0.5],
            c2: [0.7, 1.5],
            to: [1.0, 1.0],
        }],
        closed,
        ..PathData::default()
    };
    let open = [
        shape_node(ShapeKind::Line, rect, 0.0, None),
        shape_node(ShapeKind::Path, rect, 0.0, Some(polyline(false))),
        shape_node(ShapeKind::Path, rect, 0.0, Some(cubic(false))),
    ];
    for node in &open {
        assert!(is_open_shape(node));
        let host = WireHost::from_node(node);
        assert!(!host.offers_ports());
        assert!(host.ports().is_empty(), "open shape offered ports");
        // Wires saved before the gate still resolve onto the stroke.
        let end = host.anchor(Side::End, 1.0);
        assert!(end[0].is_finite() && end[1].is_finite());
    }
    for path in [polyline(true), cubic(true)] {
        let node = shape_node(ShapeKind::Path, rect, 0.0, Some(path));
        assert!(!is_open_shape(&node));
        let host = WireHost::from_node(&node);
        assert!(!host.offers_ports());
        assert!(host.ports().is_empty());
        assert!(host.anchor(Side::Mid, 0.5).iter().all(|v| v.is_finite()));
    }
    for kind in [ShapeKind::Ellipse, ShapeKind::RegularPolygon] {
        let node = shape_node(kind, rect, 0.0, None);
        assert!(!WireHost::from_node(&node).offers_ports());
    }
    let boxed = shape_node(ShapeKind::Rect, rect, 0.0, None);
    assert!(!is_open_shape(&boxed));
    assert_eq!(WireHost::from_node(&boxed).ports().len(), 4);
}
