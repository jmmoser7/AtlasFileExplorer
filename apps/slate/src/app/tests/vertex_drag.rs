//! Polyline grip grabs and vertex drags.

use super::*;

/// P1.node.corner-grip × P1.curve.grips: every press on a selected
/// polyline takes the grip it lands on. A corner grip on its segment
/// starts that corner's fillet drag, a vertex starts that vertex's drag,
/// and where a sharp corner's grip reach overlaps its vertex the nearer
/// grip wins.
#[test]
fn pressing_each_polyline_grip_grabs_that_grip() {
    let mut h = grip_board("polyline_grip_press");
    let (id, pts) = filleted_u_polyline(&mut h);
    let xf = h.app.board_xf();
    let grips = {
        let node = h.app.doc().scene.node(id).unwrap();
        h.app.corner_grips(node, &xf)
    };
    let vertices: Vec<_> = grips.iter().map(|g| g.0).collect();
    assert_eq!(
        vertices,
        vec![Some(1), Some(2)],
        "a grip per turning vertex"
    );
    let press = |h: &mut Harness, screen: Pos2| {
        let drag = h
            .app
            .begin_gesture_for_test(screen, xf.s2w(screen), egui::Modifiers::NONE);
        h.app.board_drag = None;
        drag
    };
    let takes_fillet = |drag: &Option<board::BoardDrag>, want: usize| {
        matches!(drag, Some(board::BoardDrag::FilletRadius { id: got, vertex: Some(v), .. })
            if *got == id && *v == want)
    };
    let takes_vertex = |drag: &Option<board::BoardDrag>, want: usize| {
        matches!(
            drag,
            Some(board::BoardDrag::Direct(super::super::board_direct::DirectDrag::Anchors {
                node,
                indices,
                ..
            })) if *node == id && indices[..] == [want]
        )
    };

    for (vertex, grip) in &grips {
        let v = vertex.unwrap();
        let drag = press(&mut h, *grip);
        assert!(takes_fillet(&drag, v), "corner grip {v} starts its fillet");
    }
    for (i, p) in pts.iter().enumerate() {
        let drag = press(&mut h, xf.w2s(*p));
        assert!(takes_vertex(&drag, i), "vertex {i} starts its own drag");
    }

    // Vertex 2 is sharp, so its corner grip rests just up the incoming
    // segment and the two reaches overlap.
    let (v2, g2) = (xf.w2s(pts[2]), grips[1].1);
    let gap = v2.distance(g2);
    assert!(
        gap < 2.0 * super::super::path_edit_overlay::HIT_PX,
        "the reaches overlap: {gap}"
    );
    let toward = (g2 - v2) / gap;
    let drag = press(&mut h, v2 + toward * 4.0);
    assert!(takes_vertex(&drag, 2), "nearer the vertex: the vertex");
    let drag = press(&mut h, v2 + toward * 6.5);
    assert!(takes_fillet(&drag, 2), "nearer the corner grip: the fillet");
}

/// P1.node.corner-grip × P1.curve.grips: a vertex drag keeps every
/// per-vertex corner override on its own vertex, re-applied to the new
/// edges and clamped per corner.
#[test]
fn a_vertex_drag_keeps_per_vertex_corner_overrides() {
    let mut h = grip_board("polyline_grip_overrides");
    let (id, pts) = filleted_u_polyline(&mut h);
    h.app.patch_nodes(&[id], |n| {
        slate_doc::scene::set_vertex_corner_amount(n, 2, 45.0);
    });
    h.frame();
    let moved = Pos2::new(-60.0, 180.0);
    select_drag(&mut h, pts[3], moved, egui::Modifiers::NONE);

    let n = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &n.kind else {
        panic!("a shape")
    };
    let path = s.path.as_ref().unwrap();
    assert_eq!(
        path.corner_amounts,
        vec![None, None, Some(45.0), None],
        "the override stays on vertex 2"
    );
    let drawn =
        slate_doc::geom::path_data_to_world_bez_with_fillet(path, n.rect, n.rotation_deg, s.corner);
    let world = [pts[0], pts[1], pts[2], moved].map(|p| [p.x, p.y]);
    let want = cmds_bez(&slate_doc::wire::filleted_vertex_path_each(
        &world,
        &[30.0, 30.0, 45.0, 30.0],
        false,
        false,
    ));
    assert_bez_near(&drawn, &want);
}

/// P0.1 × P1.curve.grips: a vertex grip drag edits the curve live, so Esc
/// and a release outside the window both put it back unjournaled.
#[test]
fn esc_or_release_outside_mid_vertex_drag_restores_the_curve() {
    let mut h = grip_board("esc_vertex_grip");
    let (_, pts) = filleted_u_polyline(&mut h);
    let before = scene_nodes(&h);
    let depth = h.app.tab().journal.undo_depth();
    let to = Pos2::new(160.0, 140.0);

    hold_drag(&mut h, pts[2], to, egui::Modifiers::NONE);
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Direct(_))),
        "the vertex drag is live before Esc"
    );
    assert_ne!(scene_nodes(&h), before, "the vertex moved mid-drag");
    escape_then_release(&mut h, to, egui::Modifiers::NONE);
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before, "Esc puts the curve back exactly");
    assert_eq!(h.app.tab().journal.undo_depth(), depth, "nothing journaled");

    hold_drag(&mut h, pts[2], to, egui::Modifiers::NONE);
    assert_ne!(scene_nodes(&h), before, "the vertex moved again");
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerButton {
            pos: Pos2::new(-40.0, -40.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        i.events.push(egui::Event::PointerGone);
    });
    h.frame();
    assert!(h.app.board_drag.is_none());
    assert_eq!(scene_nodes(&h), before, "a release outside restores it");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}
