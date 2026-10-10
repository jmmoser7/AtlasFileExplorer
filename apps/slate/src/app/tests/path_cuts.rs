//! Split and trim of open and closed paths.

use super::*;

/// Ctrl+Shift+T: an open two-leg Polyline divides a regular polygon into
/// two filled pieces; one undo restores the polygon.
#[test]
fn split_polygon_by_an_open_polyline() {
    let mut h = grip_board("split_open_closed_polygon");
    let hex = add_filled_rect(&mut h.app, 0.0, 0.0, 100.0, 100.0);
    h.app.patch_nodes(&[hex], |node| {
        if let NodeKind::Shape(s) = &mut node.kind {
            s.shape = slate_doc::scene::ShapeKind::RegularPolygon;
        }
    });
    let before = h.app.doc().scene.node(hex).unwrap().clone();
    let vee = commit_polyline(
        &mut h,
        &[
            Pos2::new(-10.0, 30.0),
            Pos2::new(50.0, 70.0),
            Pos2::new(110.0, 30.0),
        ],
        false,
    );
    let count = h.app.doc().scene.nodes.len();
    arm_slice_by_key(&mut h, &[vee], true);
    click_at(&mut h, Pos2::new(50.0, 40.0), egui::Modifiers::NONE);

    assert_eq!(h.app.doc().scene.nodes.len(), count + 1, "two pieces");
    let upper = h
        .app
        .node_closed_poly(h.app.doc().scene.node(hex).unwrap())
        .unwrap();
    let rest = h.app.doc().scene.nodes.last().unwrap().clone();
    let lower = h.app.node_closed_poly(&rest).unwrap();
    assert!(vector_ink::point_in_polygon(&upper, [50.0, 40.0]));
    assert!(!vector_ink::point_in_polygon(&upper, [50.0, 90.0]));
    assert!(vector_ink::point_in_polygon(&lower, [50.0, 90.0]));
    assert!(!vector_ink::point_in_polygon(&lower, [50.0, 40.0]));
    for id in [hex, rest.id] {
        assert_eq!(
            shape_of(&h.app, id).fill,
            Some(slate_doc::scene::Rgba::WHITE)
        );
    }
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), count);
    assert_eq!(h.app.doc().scene.node(hex).unwrap(), &before);
}

/// A closed cutter still divides an open target: Ctrl+T on the line
/// outside the ellipse removes only that span.
#[test]
fn trim_open_line_by_a_closed_cutter() {
    let mut h = grip_board("trim_closed_cutter_open_target");
    let ellipse = add_filled_ellipse(&mut h.app, 20.0, 20.0, 60.0, 60.0);
    let line = add_seg(&mut h.app, Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0));
    arm_slice_by_key(&mut h, &[ellipse], false);
    let count = h.app.doc().scene.nodes.len();
    click_at(&mut h, Pos2::new(10.0, 50.0), egui::Modifiers::NONE);

    assert_eq!(h.app.doc().scene.nodes.len(), count + 1, "two spans remain");
    let first = h
        .app
        .node_open_polyline(h.app.doc().scene.node(line).unwrap())
        .unwrap();
    assert!(
        (first[0][0] - 20.0).abs() < 0.2,
        "left span gone: {first:?}"
    );
    assert!(
        h.app.doc().scene.node(ellipse).is_some(),
        "the cutter stays"
    );
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), count);
}

/// Split of a styled closed polyline by an open U cutter: each piece keeps
/// its source vertices' widths and colors, and the cut vertices on the
/// outline take the value the board painted there.
#[test]
fn split_closed_polyline_by_an_open_cutter_keeps_per_vertex_style() {
    let mut h = grip_board("split_open_cutter_vertex_style");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(100.0, 100.0),
        Pos2::new(0.0, 100.0),
    ];
    let id = commit_polyline(&mut h, &pts, true);
    let widths = [2.0, 4.0, 6.0, 8.0];
    let reds = [0u8, 40, 80, 120];
    style_vertices(&mut h, id, &widths, &reds, &[None; 4]);
    let cuts = [Pos2::new(30.0, 0.0), Pos2::new(70.0, 0.0)];
    let at_cut = cuts.map(|p| ink_sample(&h, id, p));
    let u = commit_polyline(
        &mut h,
        &[
            Pos2::new(30.0, -10.0),
            Pos2::new(30.0, 40.0),
            Pos2::new(70.0, 40.0),
            Pos2::new(70.0, -10.0),
        ],
        false,
    );
    let count = h.app.doc().scene.nodes.len();
    arm_slice_by_key(&mut h, &[u], true);
    click_at(&mut h, Pos2::new(50.0, 20.0), egui::Modifiers::NONE);
    assert_eq!(h.app.doc().scene.nodes.len(), count + 1, "two pieces");

    let rest = h.app.doc().scene.nodes.last().unwrap().id;
    let mut seen = [false; 2];
    for piece in [id, rest] {
        let tips = painted_vertex_tips(&h, piece);
        let at = world_vertices(&h, piece);
        assert_eq!(tips.len(), at.len());
        for (k, p) in at.iter().enumerate() {
            let what = format!("vertex {p:?}");
            if let Some(i) = pts.iter().position(|q| near(*q, *p)) {
                assert_vertex(tips[k], widths[i], reds[i] as f32, &what);
            } else if let Some(c) = cuts.iter().position(|q| near(*q, *p)) {
                assert_matches_ink(tips[k], at_cut[c], &what);
                seen[c] = true;
            }
        }
    }
    assert_eq!(seen, [true, true], "both cut vertices were checked");
}
