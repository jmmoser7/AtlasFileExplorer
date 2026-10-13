use super::*;
use crate::scene::{Scene, ShapeNode, Stroke};
fn shape(kind: ShapeKind) -> Node {
    Scene::default().build_node(
        WorldRect::new(10.0, 20.0, 200.0, 100.0),
        NodeKind::Shape(ShapeNode {
            shape: kind,
            fill: None,
            stroke: Stroke::default(),
            corner: Corner::Square,
            sides: 7,
            phase_deg: 12.0,
            flip: false,
            path: None,
            text: None,
        }),
    )
}
fn contains(mask: &PathData, x: f32, y: f32) -> bool {
    point_in_polygon(
        &flatten_contours(
            &path_data_to_world_bez(mask, WorldRect::new(0.0, 0.0, 1.0, 1.0), 0.0),
            1e-5,
        ),
        [x, y],
    )
}
#[test]
fn cover_keeps_pixels_in_proportion_including_panorama_and_portrait() {
    for aspect in [0.01, 0.5, 2.0, 100.0] {
        let frame = WorldRect::new(10.0, 20.0, 120.0, 80.0);
        let crop = cover_crop(frame, aspect, Crop::full());
        let content = image_content_rect(frame, crop);
        assert!((content.w / content.h - aspect).abs() < 1e-4);
        assert!(content.x <= frame.x + 1e-4 && content.y <= frame.y + 1e-4);
        assert!(content.x + content.w >= frame.x + frame.w - 1e-4);
        assert!(content.y + content.h >= frame.y + frame.h - 1e-4);
        assert!((crop.x - (1.0 - crop.w) * 0.5).abs() < 1e-5);
    }
    let source = Crop {
        x: 0.25,
        y: 0.1,
        w: 0.5,
        h: 0.8,
    };
    let crop = cover_crop(WorldRect::new(0.0, 0.0, 100.0, 100.0), 4.0, source);
    assert!(crop.x >= source.x && crop.x + crop.w <= source.x + source.w);
    assert!(crop.y >= source.y && crop.y + crop.h <= source.y + source.h);
}
#[test]
fn ellipse_mask_is_curved_and_rotation_stays_on_the_node() {
    let mut node = shape(ShapeKind::Ellipse);
    let original = shape_image_clip(&node).unwrap();
    node.rotation_deg = 37.0;
    assert_eq!(shape_image_clip(&node).unwrap(), original);
    assert!(original
        .segs
        .iter()
        .any(|seg| matches!(seg, PathSeg::Cubic { .. })));
    assert!(contains(&original, 0.5, 0.5));
    assert!(!contains(&original, 0.02, 0.02));
}
#[test]
fn closed_curve_and_hole_are_preserved_without_flattening() {
    let mut node = shape(ShapeKind::Ellipse);
    let mut path = shape_image_clip(&node).unwrap();
    path.extra.push(PathContour {
        start: [0.4, 0.4],
        segs: vec![
            PathSeg::Line { to: [0.6, 0.4] },
            PathSeg::Line { to: [0.6, 0.6] },
            PathSeg::Line { to: [0.4, 0.6] },
        ],
        closed: true,
    });
    let NodeKind::Shape(s) = &mut node.kind else {
        unreachable!()
    };
    s.shape = ShapeKind::Path;
    s.path = Some(std::sync::Arc::new(path.clone()));
    let clip = shape_image_clip(&node).unwrap();
    assert_eq!(clip, path);
    assert!(contains(&clip, 0.3, 0.5));
    assert!(!contains(&clip, 0.5, 0.5));
}
#[test]
fn existing_trim_intersects_the_shape_and_keeps_holes() {
    let mut node = shape(ShapeKind::Ellipse);
    node.clip = crate::image_paint::region_path(vec![
        vec![[0.0, 0.0], [0.6, 0.0], [0.6, 1.0], [0.0, 1.0]],
        vec![[0.4, 0.4], [0.55, 0.4], [0.55, 0.6], [0.4, 0.6]],
    ]);
    let clip = shape_image_clip(&node).unwrap();
    assert!(contains(&clip, 0.3, 0.5));
    assert!(!contains(&clip, 0.8, 0.5));
    assert!(!contains(&clip, 0.02, 0.02));
    assert!(!contains(&clip, 0.5, 0.5));
}
#[test]
fn rounded_polygon_and_per_vertex_path_fillet_survive() {
    let mut node = shape(ShapeKind::RegularPolygon);
    let NodeKind::Shape(s) = &mut node.kind else {
        unreachable!()
    };
    s.corner = Corner::Rounded { radius: 10.0 };
    assert!(shape_image_clip(&node)
        .unwrap()
        .segs
        .iter()
        .any(|seg| !matches!(seg, PathSeg::Line { .. })));
    let mut node = shape(ShapeKind::Rect);
    let mut path = shape_image_clip(&node).unwrap();
    path.corner_amounts = vec![Some(10.0), None, None, None];
    let NodeKind::Shape(s) = &mut node.kind else {
        unreachable!()
    };
    s.shape = ShapeKind::Path;
    s.path = Some(std::sync::Arc::new(path));
    assert!(shape_image_clip(&node)
        .unwrap()
        .segs
        .iter()
        .any(|seg| !matches!(seg, PathSeg::Line { .. })));
}
