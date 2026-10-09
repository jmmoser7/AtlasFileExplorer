//! Brush boards, drag frames, and shift-continue checks.

use super::*;

pub(crate) fn brush_board(tag: &str) -> Harness {
    let mut h = line_board(tag);
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.tab_mut().cam.z = 1.0;
    h.app.board_osnap.enabled = false;
    h.app.board_smart_guides = false;
    h.app.board_snap_grid = false;
    h.app.brush_width = 12.0;
    h.frame();
    h.frame();
    h
}

/// One frame per event: a press, each move in its own frame, then the
/// release, with `modifiers` held throughout.
pub(crate) fn press_drag_release_frames(
    h: &mut Harness,
    world: &[Pos2],
    modifiers: egui::Modifiers,
    mut during: impl FnMut(&Harness),
) {
    let xf = h.app.board_xf();
    let pts: Vec<Pos2> = world.iter().map(|w| xf.w2s(*w)).collect();
    let (first, last) = (pts[0], *pts.last().unwrap());
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerMoved(first));
    });
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerButton {
            pos: first,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers,
        });
    });
    for p in &pts[1..] {
        h.frame_with(|i| {
            i.modifiers = modifiers;
            i.events.push(egui::Event::PointerMoved(*p));
        });
        during(h);
    }
    h.frame_with(|i| {
        i.modifiers = modifiers;
        i.events.push(egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers,
        });
    });
    h.frame();
}

pub(crate) fn path_vertices(node: &slate_doc::Node) -> Vec<Pos2> {
    let NodeKind::Shape(s) = &node.kind else {
        panic!("a shape")
    };
    let bez =
        board_path::path_data_to_world_bez(s.path.as_ref().unwrap(), node.rect, node.rotation_deg);
    bez.elements()
        .iter()
        .filter_map(|el| match el {
            vector_ink::kurbo::PathEl::MoveTo(p) | vector_ink::kurbo::PathEl::LineTo(p) => {
                Some(kpt(*p))
            }
            vector_ink::kurbo::PathEl::CurveTo(_, _, p)
            | vector_ink::kurbo::PathEl::QuadTo(_, p) => Some(kpt(*p)),
            vector_ink::kurbo::PathEl::ClosePath => None,
        })
        .collect()
}

/// Draw two freehand brush strokes and return their ids, oldest first.
pub(crate) fn two_brush_marks(h: &mut Harness) -> (NodeId, NodeId) {
    let first = [
        Pos2::new(40.0, 40.0),
        Pos2::new(80.0, 60.0),
        Pos2::new(120.0, 40.0),
    ];
    press_drag_release_frames(h, &first, egui::Modifiers::NONE, |_| {});
    let second = [
        Pos2::new(40.0, 160.0),
        Pos2::new(80.0, 180.0),
        Pos2::new(120.0, 160.0),
    ];
    press_drag_release_frames(h, &second, egui::Modifiers::NONE, |_| {});
    let nodes = &h.app.doc().scene.nodes;
    assert_eq!(nodes.len(), 2);
    (nodes[0].id, nodes[1].id)
}

/// Shift-drag from `press` and assert the segment starts at the end of
/// `mark` and extends it.
pub(crate) fn assert_shift_continues(h: &mut Harness, mark: NodeId, press: Pos2) {
    let drawn = path_vertices(h.app.doc().scene.node(mark).unwrap());
    let end = *drawn.last().unwrap();
    press_drag_release_frames(
        h,
        &[
            press,
            press + egui::vec2(60.0, 10.0),
            press + egui::vec2(120.0, 10.0),
        ],
        egui::Modifiers::SHIFT,
        |h| {
            let (from, _, node) = h.app.brush_straight_from().expect("a Shift drag");
            assert!(
                near_px(from, end),
                "starts at {from:?}, the mark ends at {end:?}"
            );
            assert_eq!(node, Some(mark));
        },
    );
    assert_eq!(
        path_vertices(h.app.doc().scene.node(mark).unwrap()).len(),
        drawn.len() + 1
    );
}

/// Add copies of the stamped stroke `proto`, moved by `offset`, until the
/// active document has a node numbered `id`.
pub(crate) fn stamped_node_numbered(
    h: &mut Harness,
    proto: &slate_doc::Node,
    offset: egui::Vec2,
    id: NodeId,
) {
    for _ in 0..=id.0 {
        if h.app.doc().scene.node(id).is_some() {
            break;
        }
        let rect = proto.rect.translated(offset.x, offset.y);
        let node = h.app.doc_mut().scene.build_node(rect, proto.kind.clone());
        h.app.add_nodes(vec![node]);
    }
    let n = h.app.doc().scene.node(id).expect("a node with that number");
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    assert!(!n.locked && !n.hidden && s.stroke.paints_as_stamp());
    assert!(!s.path.as_ref().unwrap().closed);
}
