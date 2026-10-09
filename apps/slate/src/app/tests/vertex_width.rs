//! Per-vertex stroke width tests.

use super::*;

/// User request (2026-09-26): with a polyline vertex picked, the Stroke
/// stringer's width edits only that vertex, as one journaled patch; the whole
/// curve (no point picked) still scales as a whole, as before.
#[test]
fn a_vertex_width_edit_changes_only_that_vertex() {
    let mut h = grip_board("vertex_width_only");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 0.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let w0 = curve_shape(&h, id).1.stroke.width;
    let depth = h.app.tab().journal.undo_depth();
    vertex_stringer_width(&mut h, id, &[1], 20.0);
    assert_eq!(tip_widths(&h, id), vec![w0, 20.0, w0]);
    assert_eq!(curve_shape(&h, id).1.stroke.width, 20.0, "widest vertex");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one patch");
    h.app.board_undo();
    assert!(
        tip_widths(&h, id).is_empty(),
        "undo restores the uniform stroke"
    );
    h.app.board_redo();
    assert_eq!(tip_widths(&h, id), vec![w0, 20.0, w0]);

    vertex_stringer_width(&mut h, id, &[], 40.0);
    let widths = tip_widths(&h, id);
    let (_, s) = curve_shape(&h, id);
    let painted = s.path.as_ref().unwrap().vector_widths(&s.stroke).unwrap();
    assert_eq!(s.stroke.width, 40.0, "the whole curve scales, as before");
    assert_close(painted[1], 40.0, 1e-3, "the widest vertex paints at 40");
    assert_close(painted[0], 40.0 * w0 / 20.0, 1e-3, "the taper is kept");
    assert_eq!(widths.len(), 3);
}

/// A polyline, a line and an arc interpolate vertex widths linearly between
/// their vertices (an arc by its sweep).
#[test]
fn polyline_line_and_arc_vertex_widths_taper_linearly() {
    let mut h = grip_board("vertex_width_linear");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 0.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let w0 = curve_shape(&h, id).1.stroke.width;
    vertex_stringer_width(&mut h, id, &[1], 20.0);
    let up = EVec2::new(0.0, 1.0);
    for x in [25.0, 50.0, 75.0, 125.0, 150.0] {
        let s = 1.0 - (x - 100.0_f32).abs() / 100.0;
        let want = (w0 + (20.0 - w0) * s) * 0.5;
        let got = ink_half_width(&h, id, Pos2::new(x, 0.0), up);
        assert_close(got, want, 0.15, &format!("polyline half-width at x={x}"));
    }

    let line = h
        .app
        .commit_line(Pos2::new(0.0, 200.0), Pos2::new(200.0, 200.0))
        .unwrap();
    let w1 = curve_shape(&h, line).1.stroke.width;
    vertex_stringer_width(&mut h, line, &[1], 20.0);
    assert_eq!(tip_widths(&h, line), vec![w1, 20.0]);
    let got = ink_half_width(&h, line, Pos2::new(50.0, 200.0), up);
    assert_close(got, (w1 + (20.0 - w1) * 0.25) * 0.5, 0.15, "line quarter");

    h.app.set_board_tool(board::BoardTool::Arc);
    let (s, e, m) = (
        Pos2::new(0.0, 400.0),
        Pos2::new(200.0, 400.0),
        Pos2::new(100.0, 460.0),
    );
    for p in [s, e, m] {
        h.app.path_tool_click(p);
    }
    let arc = h.app.doc().scene.nodes.last().unwrap().id;
    h.frame();
    let wa = curve_shape(&h, arc).1.stroke.width;
    vertex_stringer_width(&mut h, arc, &[1], 20.0);
    let widths = tip_widths(&h, arc);
    assert_eq!(widths[0], wa);
    assert_eq!(*widths.last().unwrap(), wa);
    assert!(
        widths.contains(&20.0),
        "the through point carries 20: {widths:?}"
    );
    // The circle through s, m, e has its center below the chord.
    let center = Pos2::new(100.0, 400.0 - 160.0 / 3.0);
    let r = (s - center).length();
    let a0 = (s.y - center.y).atan2(s.x - center.x);
    let am = (m.y - center.y).atan2(m.x - center.x);
    for f in [0.5_f32, 1.0, 1.5] {
        let a = a0 + (am - a0) * f;
        let normal = EVec2::new(a.cos(), a.sin());
        let p = center + normal * r;
        let s = if f <= 1.0 { f } else { 2.0 - f };
        let want = (wa + (20.0 - wa) * s) * 0.5;
        let got = ink_half_width(&h, arc, p, normal);
        assert_close(got, want, 0.2, &format!("arc half-width at sweep {f}/2"));
    }
}

/// A Bézier span blends vertex widths with a smooth (sigmoidal) falloff:
/// the width's slope is zero at each vertex, so the stroke has no chines.
#[test]
fn bezier_vertex_widths_blend_smoothly_with_zero_slope_at_vertices() {
    let mut h = bezier_board("vertex_width_smooth");
    for x in [0.0, 100.0, 200.0] {
        bezier_place(&mut h, Pos2::new(x, 0.0), Pos2::new(x + 30.0, 0.0));
    }
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let w0 = curve_shape(&h, id).1.stroke.width;
    vertex_stringer_width(&mut h, id, &[1], 20.0);
    assert_eq!(tip_widths(&h, id), vec![w0, 20.0, w0]);
    let up = EVec2::new(0.0, 1.0);
    let half = |h: &Harness, x: f32| ink_half_width(h, id, Pos2::new(x, 0.0), up);
    for x in [25.0_f32, 50.0, 90.0] {
        let want = (w0 + (20.0 - w0) * smoothstep(x / 100.0)) * 0.5;
        assert_close(half(&h, x), want, 0.1, &format!("smooth at x={x}"));
    }
    let at = half(&h, 100.0);
    for x in [93.75_f32, 106.25] {
        let slope = (at - half(&h, x)).abs() / 6.25;
        assert!(slope < 0.03, "slope {slope} at the vertex (x={x})");
    }
}

/// A filleted polyline keeps its vertex widths: the taper blends by
/// smoothstep between the original polyline's vertices (P1.curve.tip-chord,
/// 27 September 2026) and each fillet's middle keeps its corner's width.
#[test]
fn a_filleted_polyline_keeps_its_vertex_widths() {
    let mut h = grip_board("vertex_width_fillet");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(100.0, 100.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let w0 = curve_shape(&h, id).1.stroke.width;
    vertex_stringer_width(&mut h, id, &[1], 20.0);
    if let Some(n) = h.app.doc_mut().scene.node_mut(id) {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.corner = slate_doc::scene::Corner::from_parameters(false, false, 20.0);
        }
    }
    let got = ink_half_width(&h, id, Pos2::new(10.0, 0.0), EVec2::new(0.0, 1.0));
    let want = (w0 + (20.0 - w0) * smoothstep(0.1)) * 0.5;
    assert_close(got, want, 0.15, "near the start");
    let d = std::f32::consts::FRAC_1_SQRT_2;
    let mid = Pos2::new(80.0 + 20.0 * d, 20.0 - 20.0 * d);
    let got = ink_half_width(&h, id, mid, EVec2::new(d, -d));
    assert_close(got, 10.0, 0.2, "the fillet's middle keeps the corner width");
}

/// User, 28 September 2026 (tp4): "it works but taper produces kink at mid
/// fillet". The painted width eases through the fillet's middle with no
/// slope jump: coming in and going out, the width's slope there is zero.
#[test]
fn a_filleted_taper_paints_no_kink_at_the_fillet_middle() {
    let mut h = grip_board("vertex_width_fillet_kink");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(100.0, 100.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    vertex_stringer_width(&mut h, id, &[1], 20.0);
    if let Some(n) = h.app.doc_mut().scene.node_mut(id) {
        if let NodeKind::Shape(s) = &mut n.kind {
            s.corner = slate_doc::scene::Corner::from_parameters(false, false, 20.0);
        }
    }
    // The fillet's circle: center (80, 20), radius 20, middle at -45°.
    let (center, r) = (Pos2::new(80.0, 20.0), 20.0_f32);
    let zoom = 64.0;
    let half_at = |arc: f32| {
        let a = -std::f32::consts::FRAC_PI_4 + arc / r;
        let normal = EVec2::new(a.cos(), a.sin());
        ink_half_width_at(&h, id, center + normal * r, normal, zoom)
    };
    assert_close(
        half_at(0.0),
        10.0,
        0.05,
        "the middle keeps the corner width",
    );
    // One-sided slopes at the middle, extrapolated from 2 and 4 units so the
    // width's curvature cancels and only a kink would remain.
    let one_sided = |dir: f32| {
        let at = |s: f32| (half_at(dir * s) - half_at(0.0)) / s;
        2.0 * at(2.0) - at(4.0)
    };
    let (coming, going) = (-one_sided(-1.0), one_sided(1.0));
    assert!(
        coming.abs() < 0.02 && going.abs() < 0.02,
        "slope {coming} into the middle, {going} out of it"
    );
}

/// Grip drags keep vertex widths; an arc keeps its three grip widths while
/// the sweep is rebuilt.
#[test]
fn grip_drags_keep_vertex_widths() {
    let mut h = grip_board("vertex_width_drag");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 0.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let w0 = curve_shape(&h, id).1.stroke.width;
    vertex_stringer_width(&mut h, id, &[1], 20.0);
    select_drag(
        &mut h,
        Pos2::new(200.0, 0.0),
        Pos2::new(200.0, 50.0),
        egui::Modifiers::NONE,
    );
    assert_eq!(world_anchor_points(&h, id)[2], Pos2::new(200.0, 50.0));
    assert_eq!(tip_widths(&h, id), vec![w0, 20.0, w0]);

    h.app.set_board_tool(board::BoardTool::Arc);
    let (s, e, m) = (
        Pos2::new(0.0, 400.0),
        Pos2::new(200.0, 400.0),
        Pos2::new(100.0, 460.0),
    );
    for p in [s, e, m] {
        h.app.path_tool_click(p);
    }
    let arc = h.app.doc().scene.nodes.last().unwrap().id;
    h.frame();
    let wa = curve_shape(&h, arc).1.stroke.width;
    vertex_stringer_width(&mut h, arc, &[1], 20.0);
    let m2 = Pos2::new(100.0, 500.0);
    select_drag(&mut h, m, m2, egui::Modifiers::NONE);
    assert_circular_arc_through(&h, arc, s, m2, e);
    let widths = tip_widths(&h, arc);
    assert_eq!(widths[0], wa);
    assert_eq!(*widths.last().unwrap(), wa);
    let got = ink_half_width(&h, arc, m2, EVec2::new(0.0, 1.0));
    assert_close(got, 10.0, 0.2, "the through point keeps its width");
}

/// The export writes the same variable-width outline the board paints.
#[test]
fn vertex_widths_export_as_the_board_paints_them() {
    let mut h = bezier_board("vertex_width_export");
    for x in [0.0, 100.0, 200.0] {
        bezier_place(&mut h, Pos2::new(x, 0.0), Pos2::new(x + 30.0, 0.0));
    }
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    vertex_stringer_width(&mut h, id, &[1], 20.0);
    let (n, _) = curve_shape(&h, id);
    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    let local = |x: f32, y: f32| [x - n.rect.x, y - n.rect.y];
    let outline = html
        .split("d=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter_map(|d| vector_ink::kurbo::BezPath::from_svg(d).ok())
        .map(|bez| vector_ink::flatten_contours(&bez, 0.05))
        .find(|c| vector_ink::point_in_polygon(c, local(100.0, 9.0)))
        .expect("the exported stroke outline");
    let up = EVec2::new(0.0, 1.0);
    for x in [25.0_f32, 50.0, 90.0] {
        let board = ink_half_width(&h, id, Pos2::new(x, 0.0), up);
        assert!(
            vector_ink::point_in_polygon(&outline, local(x, board - 0.2)),
            "export is as wide as the board at x={x}"
        );
        assert!(
            !vector_ink::point_in_polygon(&outline, local(x, board + 0.2)),
            "export is no wider than the board at x={x}"
        );
    }
}
