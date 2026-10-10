//! Curve handle drags and end-condition reach.

use super::*;

/// How far the painted ink of horizontal line `id` reaches: sideways within
/// an arrowhead's length of each end, and past each end along the line.
pub(crate) struct EndReach {
    pub(crate) side: [f32; 2],
    pub(crate) past: [f32; 2],
}

pub(crate) fn end_reach(h: &Harness, id: NodeId, pts: [Pos2; 2]) -> EndReach {
    let node = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Shape(s) = &node.kind else {
        panic!("a shape");
    };
    let ink = board_path::vector_stroke_ink(node, s, s.path.as_ref().unwrap(), 1.0);
    let head = slate_doc::geom::arrow_len(s.stroke.width);
    let solid = ink.vertices.iter().filter(|v| v.alpha > 0.5);
    let mut reach = EndReach {
        side: [0.0; 2],
        past: [f32::MIN; 2],
    };
    for v in solid {
        let (x, dy) = (v.pos[0], (v.pos[1] - pts[0].y).abs());
        if x <= pts[0].x + head {
            reach.side[0] = reach.side[0].max(dy);
        }
        if x >= pts[1].x - head {
            reach.side[1] = reach.side[1].max(dy);
        }
        reach.past[0] = reach.past[0].max(pts[0].x - x);
        reach.past[1] = reach.past[1].max(x - pts[1].x);
    }
    reach
}

pub(crate) fn stroke_of(h: &Harness, id: NodeId) -> slate_doc::scene::Stroke {
    match &h.app.doc().scene.node(id).expect("node").kind {
        NodeKind::Shape(s) => s.stroke,
        _ => panic!("a shape"),
    }
}

/// A Bézier with a smooth middle anchor whose out handle points right.
pub(crate) fn handle_bezier(h: &mut Harness) -> (NodeId, [Pos2; 3]) {
    let (a, b, c) = (
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 80.0),
    );
    h.app.bezier_anchor_press(a);
    h.app.bezier_anchor_release(a, a, false);
    h.app.bezier_anchor_press(b);
    h.app
        .bezier_anchor_release(b, b + EVec2::new(40.0, 0.0), false);
    h.app.bezier_anchor_press(c);
    h.app.bezier_anchor_release(c, c, false);
    assert!(h.app.path_tool_try_finish());
    h.frame();
    (h.app.doc().scene.nodes[0].id, [a, b, c])
}

/// Real frames: press on `from`, drag to `to` with `mods` held, release.
pub(crate) fn grip_drag(h: &mut Harness, from: Pos2, to: Pos2, mods: egui::Modifiers) {
    hold_drag(h, from, to, mods);
    let p = h.app.board_xf().w2s(to);
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: mods,
        });
    });
    h.frame();
}

/// World (in, out) handle tips of anchor `idx` of curve `id`.
pub(crate) fn handle_pair(h: &Harness, id: NodeId, idx: usize) -> (Pos2, Pos2) {
    let a = h.app.direct_anchors_of(id).unwrap().0[idx];
    (kpt(a.handle_in.unwrap()), kpt(a.handle_out.unwrap()))
}

pub(crate) fn near_eps(a: Pos2, b: Pos2, eps: f32) -> bool {
    (a - b).length() < eps
}

// ---------- sub-object edges (Ctrl+Shift+click, Rhino) ----------

pub(crate) const SUB_OBJECT: egui::Modifiers = egui::Modifiers::CTRL.plus(egui::Modifiers::SHIFT);

pub(crate) fn mid(a: Pos2, b: Pos2) -> Pos2 {
    a + (b - a) * 0.5
}

/// Five-vertex open polyline around the canvas middle, not selected.
pub(crate) fn edge_polyline(h: &mut Harness) -> (NodeId, [Pos2; 5]) {
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let pts = [
        c + EVec2::new(-150.0, -50.0),
        c + EVec2::new(0.0, -50.0),
        c + EVec2::new(0.0, 50.0),
        c + EVec2::new(150.0, 50.0),
        c + EVec2::new(150.0, -100.0),
    ];
    let id = commit_polyline(h, &pts, false);
    h.app.board_sel.clear();
    h.frame();
    (id, pts)
}

/// A press, a drag through real frames, and a release; `esc` presses
/// Escape before the release.
pub(crate) fn drag_screen(h: &mut Harness, from: Pos2, to: Pos2, esc: bool) {
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(from)));
    h.frame_with(|i| i.events.push(button(from, true)));
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(mid(from, to))));
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(to)));
    if esc {
        press_key_with(h, egui::Key::Escape, egui::Modifiers::NONE);
    }
    h.frame_with(|i| i.events.push(button(to, false)));
    h.frame();
}

/// Open paths painted in `color` (the picked-edge highlight).
pub(crate) fn painted_paths_in(
    out: &egui::FullOutput,
    color: egui::Color32,
) -> Vec<egui::epaint::PathShape> {
    fn walk(shape: &egui::Shape, color: egui::Color32, out: &mut Vec<egui::epaint::PathShape>) {
        match shape {
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, color, out)),
            egui::Shape::Path(p)
                if !p.closed
                    && p.points.len() >= 2
                    && p.stroke.color == egui::epaint::ColorMode::Solid(color) =>
            {
                out.push(p.clone())
            }
            _ => {}
        }
    }
    let mut paths = Vec::new();
    for clipped in &out.shapes {
        walk(&clipped.shape, color, &mut paths);
    }
    paths
}
