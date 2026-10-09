//! Segments, filled shapes, and containment.

use super::*;

pub(crate) fn add_seg(app: &mut SlateApp, a: Pos2, b: Pos2) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let (rect, path) = board_path::points_to_path_data(&[a, b], false);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_draw_stroke(slate_doc::scene::Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(path.into()),

            text: None,
        }),
    );
    app.add_nodes(vec![node])[0]
}

pub(crate) fn add_filled_rect(app: &mut SlateApp, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, w, h);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Rect,
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,

            text: None,
        }),
    );
    app.add_nodes(vec![node])[0]
}

pub(crate) fn add_filled_ellipse(app: &mut SlateApp, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, w, h);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Ellipse,
            fill: Some(slate_doc::scene::Rgba::WHITE),
            stroke: slate_doc::scene::Stroke::none(),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: None,

            text: None,
        }),
    );
    app.add_nodes(vec![node])[0]
}

pub(crate) fn closed_shape_contains(app: &SlateApp, p: [f32; 2]) -> bool {
    app.doc().scene.nodes.iter().any(|n| {
        let slate_doc::scene::NodeKind::Shape(s) = &n.kind else {
            return false;
        };
        let Some(path) = s.path.as_ref() else {
            return false;
        };
        if !path.closed {
            return false;
        }
        let bez = board_path::path_data_to_world_bez(path, n.rect, n.rotation_deg);
        let contours = vector_ink::flatten_contours(&bez, 0.35);
        vector_ink::point_in_polygon(&contours, p)
    })
}

// ---------- trim and split between open and closed forms ----------
// User, 28 September 2026: "allow triming betwee open and closed forms".

/// A two-point diagonal line crossing `(x, x)` at a right angle.
pub(crate) fn crossing_line(app: &mut SlateApp, x: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let (rect, data) = board_path::points_to_path_data(
        &[Pos2::new(x - 12.0, x + 12.0), Pos2::new(x + 12.0, x - 12.0)],
        false,
    );
    let node = app.doc_mut().scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Path,
            fill: None,
            stroke: board_path::default_curve_stroke(Rgba::BLACK),
            corner: slate_doc::scene::Corner::Square,
            sides: slate_doc::scene::default_regular_sides(),
            phase_deg: 0.0,
            flip: false,
            path: Some(data.into()),
            text: None,
        }),
    );
    let id = node.id;
    app.add_nodes(vec![node]);
    id
}
