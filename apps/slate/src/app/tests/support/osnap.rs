//! Object-snap boards and endpoint checks.

use super::*;

pub(crate) fn assert_endpoints(app: &SlateApp, a: Pos2, b: Pos2) -> NodeId {
    assert_eq!(app.doc().scene.nodes.len(), 1, "exactly one node committed");
    let node = &app.doc().scene.nodes[0];
    let (pa, pb) = board_line::line_endpoints(node).expect("a simple line node");
    for (got, want) in [(pa, a), (pb, b)] {
        assert!(
            (got - want).length() < 0.05,
            "endpoint {got:?} != expected {want:?}"
        );
    }
    node.id
}

// ---------- Object snaps (contracts/object-snap.md GP1–GP4) ----------

pub(crate) fn osnap_board(tag: &str) -> Harness {
    let mut h = Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.board_snap_grid = false;
    // These fixtures exercise object snaps independently of alignment guides.
    h.app.board_smart_guides = false;
    h.app.board_osnap = slate_doc::ObjectSnapSet::default();
    h
}

pub(crate) fn add_ellipse(app: &mut SlateApp, x: f32, y: f32, w: f32, h: f32) -> NodeId {
    use slate_doc::scene::{ShapeKind, ShapeNode};
    let rect = slate_doc::scene::WorldRect::new(x, y, w, h);
    let node = app.doc_mut().scene.build_node(
        rect,
        slate_doc::scene::NodeKind::Shape(ShapeNode {
            shape: ShapeKind::Ellipse,
            fill: None,
            stroke: slate_doc::scene::Stroke::default(),
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

pub(crate) fn only_kind(kind: slate_doc::SnapKind) -> slate_doc::ObjectSnapSet {
    let mut set = slate_doc::ObjectSnapSet {
        enabled: true,
        end: false,
        mid: false,
        center: false,
        near: false,
        intersection: false,
        quadrant: false,
        perpendicular: false,
        tangent: false,
    };
    set.set(kind, true);
    set
}

// ---------- tool kits: the result of a gesture comes from data ----------
