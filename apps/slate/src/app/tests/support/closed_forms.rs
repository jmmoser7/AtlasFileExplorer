//! Closed-form boards and their painted tips.

use super::*;

/// A stroked rectangle or hexagon in the middle of the canvas, selected,
/// with its world vertices in vertex order (a rectangle: top-left,
/// top-right, bottom-right, bottom-left).
pub(crate) fn closed_form_board(
    tag: &str,
    kind: slate_doc::scene::ShapeKind,
) -> (Harness, NodeId, Vec<Pos2>) {
    use slate_doc::scene::{Corner, ShapeKind, ShapeNode, Stroke};
    let mut h = grip_board(tag);
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let rect = WorldRect::new(c.x - 120.0, c.y - 90.0, 240.0, 180.0);
    let mut stroke = Stroke::none();
    stroke.width = 4.0;
    stroke.color = Rgba([20, 20, 20, 255]);
    let node = h.app.doc_mut().scene.build_node(
        rect,
        NodeKind::Shape(ShapeNode {
            shape: kind,
            fill: Some(Rgba([230, 230, 230, 255])),
            stroke,
            corner: Corner::Square,
            sides: 6,
            phase_deg: 0.0,
            flip: false,
            path: None,
            text: None,
        }),
    );
    let id = h.app.add_nodes(vec![node])[0];
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    for _ in 0..3 {
        h.frame();
    }
    let v = match kind {
        ShapeKind::RegularPolygon => slate_doc::scene::regular_polygon_vertices(rect, 6, 0.0)
            .into_iter()
            .map(|[x, y]| Pos2::new(x, y))
            .collect(),
        _ => vec![
            Pos2::new(rect.x, rect.y),
            Pos2::new(rect.x + rect.w, rect.y),
            Pos2::new(rect.x + rect.w, rect.y + rect.h),
            Pos2::new(rect.x, rect.y + rect.h),
        ],
    };
    (h, id, v)
}

/// Painted width and color at each vertex of closed form `id`, read from
/// the stored per-vertex tips (the widest paints at the stroke width).
pub(crate) fn closed_tips(h: &Harness, id: NodeId, n: usize) -> Vec<(f32, [u8; 4])> {
    let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("a shape")
    };
    let tips = s.path.as_ref().map(|p| p.tips.clone()).unwrap_or_default();
    if tips.len() != n {
        return vec![(s.stroke.width, s.stroke.color.0); n];
    }
    let widest = tips.iter().map(|t| t.width).fold(0.0_f32, f32::max);
    tips.iter()
        .map(|t| (t.width * s.stroke.width / widest.max(1e-6), t.color.0))
        .collect()
}

pub(crate) fn closed_corner_overrides(h: &Harness, id: NodeId, n: usize) -> Vec<Option<f32>> {
    let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
        panic!("a shape")
    };
    match s.path.as_ref().map(|p| p.corner_amounts.clone()) {
        Some(c) if c.len() == n => c,
        _ => vec![None; n],
    }
}

/// The closed form as both interpreters paint it once it stores vertex
/// style: the path shape `vertex_style::closed_form_paint_shape` derives.
pub(crate) fn closed_paint_shape(
    h: &Harness,
    id: NodeId,
) -> (slate_doc::scene::Node, slate_doc::scene::ShapeNode) {
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    let styled = slate_doc::vertex_style::closed_form_paint_shape(s, n.rect)
        .expect("the closed form paints its vertex style");
    (n, styled)
}

/// Full painted stroke width at each vertex of closed form `id`.
pub(crate) fn closed_painted_widths(h: &Harness, id: NodeId) -> Vec<f32> {
    let (n, styled) = closed_paint_shape(h, id);
    let tipped = slate_doc::geom::tipped_stroke(
        styled.path.as_ref().unwrap(),
        &styled.stroke,
        n.rect,
        n.rotation_deg,
        styled.corner,
    )
    .expect("a per-vertex stroke");
    tipped.widths
}

pub(crate) fn shape_kind_of(h: &Harness, id: NodeId) -> slate_doc::scene::ShapeKind {
    match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Shape(s) => s.shape,
        _ => panic!("a shape"),
    }
}

/// A closed three-anchor Bézier, selected under the Select tool.
pub(crate) fn closed_bezier(h: &mut Harness) -> (NodeId, [Pos2; 3]) {
    bezier_loop(h, EVec2::new(0.0, -40.0));
    let t = h.ctx.input(|i| i.time);
    bezier_hover(h, Pos2::ZERO, t + 0.1, 0.4);
    bezier_click(h, Pos2::ZERO);
    let (id, s) = only_shape(h);
    assert!(s.path.as_ref().unwrap().closed, "a closed path");
    h.app.board_sel = std::iter::once(id).collect();
    for _ in 0..3 {
        h.frame();
    }
    (
        id,
        [Pos2::ZERO, Pos2::new(200.0, 0.0), Pos2::new(100.0, 150.0)],
    )
}

// ---------- Bézier span drafting and editing (contracts/bezier-span.md) ----------
