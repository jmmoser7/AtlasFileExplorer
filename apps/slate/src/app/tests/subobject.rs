//! Sub-object edge tests.

use super::*;

mod guards;

#[test]
fn tiny_shape_body_moves_before_overlapping_transform_grips() {
    use slate_doc::scene::ShapeKind;
    for zoom in [0.5, 1.0, 8.0] {
        let (mut h, id, _) = closed_form_board("tiny_body_move", ShapeKind::Rect);
        let center = h.app.board_xf().s2w(h.app.canvas_rect.center());
        h.app.patch_nodes(&[id], |n| {
            n.rect = WorldRect::new(center.x, center.y, 4.0, 3.0);
        });
        h.app.tab_mut().cam.z = zoom;
        h.app.tab_mut().cam.offset = center.to_vec2();
        h.frame();
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let xf = h.app.board_xf();
        let grab = xf.w2s(Pos2::new(before.rect.x + 2.0, before.rect.y + 1.5));
        assert_eq!(h.app.tiny_body_owns_pointer(grab), Some(id));
        assert_eq!(
            h.app.transform_hit_at(grab),
            Some((Some(id), board_handles::BoardHitTarget::Body))
        );
        drag_screen(&mut h, grab, grab + EVec2::new(50.0, 35.0), false);
        let after = h.app.doc().scene.node(id).unwrap();
        assert_eq!(shape_kind_of(&h, id), ShapeKind::Rect);
        assert_eq!(after.rect.w, before.rect.w);
        assert_eq!(after.rect.h, before.rect.h);
        assert_ne!(
            after.rect.x, before.rect.x,
            "tiny body moves at zoom {zoom}"
        );
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    }
}

#[test]
fn ctrl_shift_sweeps_pick_contained_or_crossing_segments() {
    let mut h = grip_board("edge_sweep");
    let (id, pts) = edge_polyline(&mut h);
    let xf = h.app.board_xf();
    let depth = h.app.tab().journal.undo_depth();
    let a = xf.w2s(pts[0] + EVec2::new(-20.0, -20.0));
    let b = xf.w2s(pts[2] + EVec2::new(20.0, 20.0));
    let pointer = |h: &mut Harness, pos, button| {
        h.frame_with(|i| {
            i.modifiers = SUB_OBJECT;
            i.events.push(egui::Event::PointerMoved(pos));
            if let Some(pressed) = button {
                i.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: SUB_OBJECT,
                });
            }
        })
    };
    pointer(&mut h, a, Some(true));
    pointer(&mut h, mid(a, b), None);
    pointer(&mut h, b, None);
    pointer(&mut h, b, Some(false));
    assert_eq!(
        h.app
            .direct
            .grip_points
            .edges
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(h.app.board_sel, [id].into_iter().collect());
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
    // Right-to-left crossing touches edge 2 without enclosing its ends.
    let a = xf.w2s(mid(pts[2], pts[3]) + EVec2::new(10.0, -10.0));
    let b = a - EVec2::new(20.0, -20.0);
    pointer(&mut h, a, Some(true));
    pointer(&mut h, b, None);
    pointer(&mut h, b, Some(false));
    assert_eq!(
        h.app
            .direct
            .grip_points
            .edges
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn deleting_a_primitive_vertex_promotes_it_and_undo_restores_it() {
    use slate_doc::scene::ShapeKind;
    for kind in [ShapeKind::Rect, ShapeKind::RegularPolygon] {
        let (mut h, id, v) = closed_form_board("primitive_delete_vertex", kind);
        let before = h.app.doc().scene.node(id).unwrap().clone();
        let count = v.len();
        let at = h.app.board_xf().w2s(v[1]);
        press_primary(&mut h, at, egui::Modifiers::NONE);
        assert_eq!(h.app.picked_vertices(), Some((id, vec![1])));
        press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(shape_kind_of(&h, id), ShapeKind::Path);
        let (anchors, closed) = h.app.direct_anchors_of(id).unwrap();
        assert!(closed);
        assert_eq!(anchors.len(), count - 1);
        let NodeKind::Shape(s) = &h.app.doc().scene.node(id).unwrap().kind else {
            panic!("shape")
        };
        let NodeKind::Shape(old) = &before.kind else {
            panic!("shape")
        };
        assert_eq!(s.fill, old.fill);
        assert_eq!(s.stroke, old.stroke);
        h.app.board_undo();
        assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
    }
}

#[test]
fn deleting_a_rectangle_edge_opens_its_outline() {
    use slate_doc::scene::ShapeKind;
    let (mut h, id, v) = closed_form_board("primitive_delete_edge", ShapeKind::Rect);
    let before = h.app.doc().scene.node(id).unwrap().clone();
    let at = h.app.board_xf().w2s(mid(v[1], v[2]));
    press_primary(&mut h, at, SUB_OBJECT);
    assert_eq!(
        h.app
            .direct
            .grip_points
            .edges
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![1]
    );
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert_eq!(shape_kind_of(&h, id), ShapeKind::Path);
    assert!(!h.app.direct_anchors_of(id).unwrap().1);
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before);
}

/// User (28 September 2026, "dig selection": "lok at rhino president for
/// selecting sub elements. would be good to seect single edges of objects
/// like polylines"): Ctrl+Shift+click on a segment selects that curve alone
/// and picks the edge with its two end vertices; more edges add, a picked
/// one toggles off. The strip then edits those vertices.
#[test]
fn ctrl_shift_click_picks_an_edge_and_its_two_vertices() {
    let mut h = grip_board("edge_pick");
    let (id, pts) = edge_polyline(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(mid(pts[1], pts[2])), SUB_OBJECT);
    assert_eq!(
        h.app.board_sel,
        [id].into_iter().collect(),
        "the curve alone"
    );
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1, 2])), "edge 1");

    pause(&mut h);
    press_primary(&mut h, xf.w2s(mid(pts[2], pts[3])), SUB_OBJECT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1, 2, 3])), "adds");
    pause(&mut h);
    press_primary(&mut h, xf.w2s(mid(pts[2], pts[3])), SUB_OBJECT);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![1, 2])),
        "toggles off; vertex 2 still ends edge 1"
    );
    h.frame();
    assert_eq!(
        h.app.shape_property_points(),
        vec![1, 2],
        "the strip edits them"
    );

    // A plain click on the curve away from the grips targets it whole again.
    pause(&mut h);
    press_primary(&mut h, xf.w2s(mid(pts[0], pts[1])), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), None);
}

/// Dragging a picked edge carries both of its vertices by one snapped step
/// (pm1: the carried vertex snaps to the curve's other points), one undo
/// step; Esc mid-drag puts the curve back.
#[test]
fn a_picked_edge_drags_both_vertices_snapped_in_one_undo_step() {
    let mut h = grip_board("edge_drag");
    let (id, pts) = edge_polyline(&mut h);
    let xf = h.app.board_xf();
    let grab = mid(pts[1], pts[2]);
    press_primary(&mut h, xf.w2s(grab), SUB_OBJECT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1, 2])));
    let depth = h.app.tab().journal.undo_depth();

    // Carry vertex 1 to just off vertex 4: it lands on it exactly.
    let step = pts[4] - pts[1];
    drag_screen(
        &mut h,
        xf.w2s(grab),
        xf.w2s(grab + step + EVec2::new(3.0, 2.0)),
        false,
    );
    let got = world_anchor_points(&h, id);
    assert_eq!(got.len(), 5, "{got:?}");
    assert!(
        near(got[1], pts[4]),
        "vertex 1 snaps onto vertex 4: {got:?}"
    );
    assert!(
        near(got[2], pts[2] + step),
        "vertex 2 moves with it: {got:?}"
    );
    for k in [0, 3, 4] {
        assert!(near(got[k], pts[k]), "vertex {k} stays: {got:?}");
    }
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    h.app.board_undo();
    assert_eq!(world_anchor_points(&h, id), pts.to_vec(), "undo restores");

    h.app.direct.grip_points = Default::default();
    pause(&mut h);
    press_primary(&mut h, xf.w2s(grab), SUB_OBJECT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1, 2])));
    let before = h.app.doc().scene.node(id).unwrap().clone();
    drag_screen(
        &mut h,
        xf.w2s(grab),
        xf.w2s(grab + EVec2::new(40.0, 25.0)),
        true,
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap(), &before, "Esc restores");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// Delete with an edge picked removes that segment through the trim
/// owner: an open curve becomes two curves, a closed curve opens there,
/// and a line's only edge takes the line. One Ctrl+Z each.
#[test]
fn delete_with_a_picked_edge_removes_that_segment() {
    let mut h = grip_board("edge_delete");
    let (id, pts) = edge_polyline(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(mid(pts[1], pts[2])), SUB_OBJECT);
    let count = h.app.doc().scene.nodes.len();
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert_eq!(h.app.doc().scene.nodes.len(), count + 1, "two curves");
    assert_eq!(world_anchor_points(&h, id), vec![pts[0], pts[1]]);
    let other = h.app.doc().scene.nodes.last().unwrap().id;
    assert_eq!(world_anchor_points(&h, other), vec![pts[2], pts[3], pts[4]]);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert_eq!(world_anchor_points(&h, id), pts.to_vec(), "one Ctrl+Z");

    // A closed square opens at the deleted edge.
    let c = pts[0] + EVec2::new(0.0, 250.0);
    let sq = [
        c,
        c + EVec2::new(100.0, 0.0),
        c + EVec2::new(100.0, 100.0),
        c + EVec2::new(0.0, 100.0),
    ];
    let sq_id = commit_polyline(&mut h, &sq, true);
    h.app.board_sel.clear();
    h.frame();
    pause(&mut h);
    press_primary(&mut h, xf.w2s(mid(sq[1], sq[2])), SUB_OBJECT);
    assert_eq!(h.app.picked_vertices(), Some((sq_id, vec![1, 2])));
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    let (anchors, closed) = h.app.direct_anchors_of(sq_id).unwrap();
    assert!(!closed, "the square opens");
    let got: Vec<Pos2> = anchors.iter().map(|a| kpt(a.point)).collect();
    assert_eq!(got, vec![sq[2], sq[3], sq[0], sq[1]]);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert!(
        h.app.direct_anchors_of(sq_id).unwrap().1,
        "one Ctrl+Z closes it"
    );

    // A two-point line has one edge.
    let line = [c + EVec2::new(300.0, 0.0), c + EVec2::new(420.0, 60.0)];
    let line_id = commit_polyline(&mut h, &line, false);
    h.app.board_sel.clear();
    h.frame();
    pause(&mut h);
    press_primary(&mut h, xf.w2s(mid(line[0], line[1])), SUB_OBJECT);
    assert_eq!(h.app.picked_vertices(), Some((line_id, vec![0, 1])));
    press_key_with(&mut h, egui::Key::Delete, egui::Modifiers::NONE);
    assert!(h.app.doc().scene.node(line_id).is_none(), "the line goes");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert!(h.app.doc().scene.node(line_id).is_some());
}

/// Ctrl+Shift+click keeps its group meaning: a grouped rectangle is
/// selected alone, with nothing picked.
#[test]
fn ctrl_shift_click_still_selects_one_group_member() {
    let mut h = grip_board("edge_group_member");
    let (id, pts) = edge_polyline(&mut h);
    let rect = add_rect(&mut h.app, pts[0].x, pts[0].y + 200.0);
    h.app.board_sel = [id, rect].into_iter().collect();
    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("board.group"), None));
    h.app.board_sel.clear();
    h.frame();
    let xf = h.app.board_xf();
    press_primary(
        &mut h,
        xf.w2s(Pos2::new(pts[0].x + 40.0, pts[0].y + 230.0)),
        SUB_OBJECT,
    );
    assert_eq!(
        h.app.board_sel,
        [rect].into_iter().collect(),
        "the member alone"
    );
    assert_eq!(h.app.picked_vertices(), None);
    press_primary(&mut h, xf.w2s(mid(pts[1], pts[2])), SUB_OBJECT);
    assert_eq!(
        h.app.board_sel,
        [id].into_iter().collect(),
        "the member curve"
    );
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![1, 2])),
        "and its edge"
    );
}

/// Review round 8: a picked edge is a canvas object (P0.9), not a path-edit
/// handle. At 8x its highlight is `canvas_scale::px(3, 8)` wide and follows
/// the curve to within a screen pixel.
#[test]
fn a_picked_edge_highlight_scales_and_hugs_the_curve_at_high_zoom() {
    use vector_ink::kurbo::{ParamCurveNearest, Point};
    let mut h = grip_board("edge_highlight_zoom");
    let id = add_bezier_arch(&mut h.app);
    h.app.board_sel.clear();
    h.app.tab_mut().cam.z = 8.0;
    h.app.tab_mut().cam.offset = EVec2::new(50.0, 17.5);
    h.frame();
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(Pos2::new(50.0, 17.5)), SUB_OBJECT);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![0, 1])),
        "the arch's edge"
    );
    // The selection silhouette shares the color: the highlight is what the
    // picked edge adds to the same selection.
    let select = h.app.palette().select;
    let picks = std::mem::take(&mut h.app.direct.grip_points);
    let unpicked = painted_paths_in(&h.frame_output(|_| {}), select);
    h.app.direct.grip_points = picks;
    let paths: Vec<_> = painted_paths_in(&h.frame_output(|_| {}), select)
        .into_iter()
        .filter(|p| !unpicked.contains(p))
        .collect();
    assert!(!paths.is_empty(), "the picked edge is highlighted");
    let (anchors, closed) = h.app.direct_anchors_of(id).unwrap();
    let bez = vector_ink::bezpath_from_anchors(&anchors, closed);
    let seg = bez.segments().next().unwrap();
    let want = atlas_shell::canvas_scale::px(3.0, 8.0);
    let mut worst = 0.0_f64;
    for p in &paths {
        assert!(
            (p.stroke.width - want).abs() < 1e-3,
            "width {} scales",
            p.stroke.width
        );
        for pair in p.points.windows(2) {
            for q in [pair[0], mid(pair[0], pair[1])] {
                let w = xf.s2w(q);
                let d = seg
                    .nearest(Point::new(w.x as f64, w.y as f64), 1e-9)
                    .distance_sq
                    .sqrt();
                worst = worst.max(d * 8.0);
            }
        }
    }
    assert!(
        worst <= 1.0,
        "the highlight strays {worst:.2} screen px off the curve"
    );
}

/// A steady frame with an edge picked reuses the flattened highlight.
#[test]
fn a_picked_edge_highlight_does_not_reflatten_on_a_steady_frame() {
    let mut h = grip_board("edge_highlight_cache");
    let (id, pts) = edge_polyline(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(mid(pts[1], pts[2])), SUB_OBJECT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1, 2])));
    h.frame();
    let before = super::super::board_direct::picked_edges_flattened_on_this_thread();
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(
        super::super::board_direct::picked_edges_flattened_on_this_thread(),
        before,
        "steady frames re-flattened the highlight"
    );
    h.app.tab_mut().cam.z = 2.0;
    h.frame();
    assert!(
        super::super::board_direct::picked_edges_flattened_on_this_thread() > before,
        "a new zoom bucket refines it"
    );
}
