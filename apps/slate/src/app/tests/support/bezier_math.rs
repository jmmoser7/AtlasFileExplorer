//! Bezier path comparisons and hover loops.

use super::*;

pub(crate) fn cmds_bez(cmds: &[slate_doc::wire::PathCmd]) -> vector_ink::kurbo::BezPath {
    use slate_doc::wire::PathCmd;
    use vector_ink::kurbo::Point;
    let pt = |p: [f32; 2]| Point::new(p[0] as f64, p[1] as f64);
    let mut bez = vector_ink::kurbo::BezPath::new();
    for cmd in cmds {
        match *cmd {
            PathCmd::Move(p) => bez.move_to(pt(p)),
            PathCmd::Line(p) => bez.line_to(pt(p)),
            PathCmd::Cubic { c1, c2, to } => bez.curve_to(pt(c1), pt(c2), pt(to)),
        }
    }
    bez
}

pub(crate) fn assert_bez_near(got: &vector_ink::kurbo::BezPath, want: &vector_ink::kurbo::BezPath) {
    use vector_ink::kurbo::PathEl;
    let points = |el: &PathEl| -> Vec<vector_ink::kurbo::Point> {
        match *el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
            PathEl::QuadTo(a, b) => vec![a, b],
            PathEl::CurveTo(a, b, c) => vec![a, b, c],
            PathEl::ClosePath => vec![],
        }
    };
    let (g, w) = (got.elements(), want.elements());
    assert_eq!(g.len(), w.len(), "{got:?} != {want:?}");
    for (a, b) in g.iter().zip(w) {
        for (p, q) in points(a).into_iter().zip(points(b)) {
            assert!((p - q).hypot() < 1e-2, "{got:?} != {want:?}");
        }
    }
}

/// The committed path of `id` is one circular arc from `s` to `e` passing
/// through `m`, within the arc tool's own fitting tolerance.
pub(crate) fn assert_circular_arc_through(h: &Harness, id: NodeId, s: Pos2, m: Pos2, e: Pos2) {
    use vector_ink::kurbo::{ParamCurve, PathSeg as KSeg, Point};
    let n = h.app.doc().scene.node(id).unwrap();
    let NodeKind::Shape(shape) = &n.kind else {
        panic!("a shape")
    };
    let bez = board_path::path_data_to_world_bez(shape.path.as_ref().unwrap(), n.rect, 0.0);
    let (sx, sy, mx, my, ex, ey) = (
        s.x as f64, s.y as f64, m.x as f64, m.y as f64, e.x as f64, e.y as f64,
    );
    let d = 2.0 * (sx * (my - ey) + mx * (ey - sy) + ex * (sy - my));
    let (s2, m2, e2) = (sx * sx + sy * sy, mx * mx + my * my, ex * ex + ey * ey);
    let c = Point::new(
        (s2 * (my - ey) + m2 * (ey - sy) + e2 * (sy - my)) / d,
        (s2 * (ex - mx) + m2 * (sx - ex) + e2 * (mx - sx)) / d,
    );
    let r = (Point::new(sx, sy) - c).hypot();
    let mut samples = Vec::new();
    for seg in bez.segments() {
        for i in 0..=256 {
            samples.push(match seg {
                KSeg::Line(l) => l.eval(i as f64 / 256.0),
                KSeg::Quad(q) => q.eval(i as f64 / 256.0),
                KSeg::Cubic(k) => k.eval(i as f64 / 256.0),
            });
        }
    }
    let first = *samples.first().unwrap();
    let last = *samples.last().unwrap();
    assert!(
        (first - Point::new(sx, sy)).hypot() < 0.05,
        "starts at {s:?}: {first:?}"
    );
    assert!(
        (last - Point::new(ex, ey)).hypot() < 0.05,
        "ends at {e:?}: {last:?}"
    );
    for p in &samples {
        assert!(
            ((*p - c).hypot() - r).abs() < 0.3,
            "{p:?} is off the circle"
        );
    }
    let through = samples
        .iter()
        .map(|p| (*p - Point::new(mx, my)).hypot())
        .fold(f64::INFINITY, f64::min);
    assert!(through < 0.5, "passes through {m:?} (closest {through})");
}

// ---------- closing a Bézier span on its start anchor (bezier-span.md) ----------

/// Hover `at` from `t0` for `dwell` seconds (two frames, no movement between).
pub(crate) fn bezier_hover(h: &mut Harness, at: Pos2, t0: f64, dwell: f64) {
    let s = h.app.board_xf().w2s(at);
    h.frame_with(|i| {
        i.time = Some(t0);
        i.events.push(egui::Event::PointerMoved(s));
    });
    h.frame_with(|i| i.time = Some(t0 + dwell));
}

/// Three anchors: the start (weighted when `start_out` is non-zero), a
/// corner, and a corner that loops back toward the start.
pub(crate) fn bezier_loop(h: &mut Harness, start_out: EVec2) {
    bezier_place(h, Pos2::ZERO, Pos2::ZERO + start_out);
    bezier_place(h, Pos2::new(200.0, 0.0), Pos2::new(200.0, 0.0));
    bezier_place(h, Pos2::new(100.0, 150.0), Pos2::new(100.0, 150.0));
    assert_eq!(bezier_draft(h).len(), 3);
}

pub(crate) fn only_shape(h: &Harness) -> (NodeId, slate_doc::scene::ShapeNode) {
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "exactly one node");
    let n = &h.app.doc().scene.nodes[0];
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    (n.id, s.clone())
}

// ----- image crop: handle hits, first grab, multi-crop, repeat -------------------
