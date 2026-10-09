//! Per-vertex style across trim, split, and join.

use super::*;

/// User request (2026-09-26): a trimmed polyline keeps each vertex's width,
/// color and corner override; the cut vertex takes the stroke's value at
/// the cut as the board paints it. The corner overrides fillet it, so that
/// value is the smoothstep between its neighbors (P1.curve.tip-chord).
#[test]
fn trim_keeps_per_vertex_style_and_interpolates_the_cut() {
    let mut h = grip_board("trim_vertex_style");
    let id = styled_cut_polyline(&mut h);
    let at_cut = ink_sample(&h, id, Pos2::new(25.0, 0.0));
    let cutter = add_seg(&mut h.app, Pos2::new(25.0, -10.0), Pos2::new(25.0, 10.0));
    select_trim(&mut h.app, &[id, cutter]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(10.0, 0.0), false));
    h.frame();

    assert_eq!(world_vertices(&h, id)[0], Pos2::new(25.0, 0.0));
    let tips = painted_vertex_tips(&h, id);
    assert_eq!(tips.len(), 4, "cut vertex plus the three kept vertices");
    let s = smoothstep(0.25);
    assert_vertex(tips[0], 2.0 + 18.0 * s, 200.0 * s, "cut vertex");
    assert_vertex(tips[1], 20.0, 200.0, "vertex 1");
    assert_vertex(tips[2], 6.0, 100.0, "vertex 2");
    assert_vertex(tips[3], 12.0, 50.0, "vertex 3");
    assert_matches_ink(tips[0], at_cut, "cut vertex");
    assert_eq!(
        corner_overrides(&h, id),
        vec![None, Some(8.0), Some(4.0), None]
    );
}

/// A trimmed Bézier span's cut vertex takes the smoothstep blend the board
/// paints on curves, and every vertex of the piece sits on that blend.
#[test]
fn trim_interpolates_a_bezier_cut_by_smoothstep() {
    let mut h = bezier_board("trim_vertex_style_bezier");
    for x in [0.0, 100.0, 200.0] {
        bezier_place(&mut h, Pos2::new(x, 0.0), Pos2::new(x + 30.0, 0.0));
    }
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    style_vertices(&mut h, id, &[2.0, 20.0, 2.0], &[0, 255, 0], &[]);
    let at_cut = ink_sample(&h, id, Pos2::new(25.0, 0.0));
    let cutter = add_seg(&mut h.app, Pos2::new(25.0, -10.0), Pos2::new(25.0, 10.0));
    select_trim(&mut h.app, &[id, cutter]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(10.0, 0.0), false));
    h.frame();

    let tips = painted_vertex_tips(&h, id);
    let at = world_vertices(&h, id);
    assert_eq!(tips.len(), at.len());
    assert_close(at[0].x, 25.0, 0.01, "the piece starts at the cut");
    for (tip, p) in tips.iter().zip(&at) {
        let s = if p.x <= 100.0 {
            smoothstep(p.x / 100.0)
        } else {
            1.0 - smoothstep((p.x - 100.0) / 100.0)
        };
        let what = format!("vertex at x={}", p.x);
        assert_vertex(*tip, 2.0 + 18.0 * s, 255.0 * s, &what);
    }
    assert_close(tips[0].0, 2.0 + 18.0 * smoothstep(0.25), 0.02, "cut width");
    assert_matches_ink(tips[0], at_cut, "cut vertex");
}

/// User request (2026-09-26): both pieces of a split keep their source
/// vertices' widths, colors and corner overrides and share the cut's
/// interpolated value.
#[test]
fn split_keeps_per_vertex_style_on_every_piece() {
    let mut h = grip_board("split_vertex_style");
    let id = styled_cut_polyline(&mut h);
    let at_cut = ink_sample(&h, id, Pos2::new(25.0, 0.0));
    let cutter = add_seg(&mut h.app, Pos2::new(25.0, -10.0), Pos2::new(25.0, 10.0));
    select_trim(&mut h.app, &[id, cutter]);
    h.app.set_board_tool(board::BoardTool::Split);
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.trim_click(Pos2::new(10.0, 0.0), false));
    h.frame();
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1, "two pieces");
    let rest = h.app.doc().scene.nodes.last().unwrap().id;

    let first = painted_vertex_tips(&h, id);
    assert_eq!(world_vertices(&h, id)[1], Pos2::new(25.0, 0.0));
    assert_eq!(first.len(), 2);
    assert_vertex(first[0], 2.0, 0.0, "first piece start");
    let s = smoothstep(0.25);
    let (cut_w, cut_r) = (2.0 + 18.0 * s, 200.0 * s);
    assert_vertex(first[1], cut_w, cut_r, "first piece cut");
    assert!(corner_overrides(&h, id).iter().all(Option::is_none));

    let second = painted_vertex_tips(&h, rest);
    assert_eq!(world_vertices(&h, rest)[0], Pos2::new(25.0, 0.0));
    assert_eq!(second.len(), 4);
    assert_vertex(second[0], cut_w, cut_r, "second piece cut");
    assert_vertex(second[1], 20.0, 200.0, "vertex 1");
    assert_vertex(second[2], 6.0, 100.0, "vertex 2");
    assert_vertex(second[3], 12.0, 50.0, "vertex 3");
    assert_matches_ink(first[1], at_cut, "first piece cut");
    assert_matches_ink(second[0], at_cut, "second piece cut");
    assert_eq!(
        corner_overrides(&h, rest),
        vec![None, Some(8.0), Some(4.0), None]
    );
}

/// A split closed polyline: every piece keeps the corners it inherits and
/// the cut vertices take the edge's value at the cut.
#[test]
fn split_closed_polyline_keeps_corner_overrides_and_tips() {
    let mut h = grip_board("split_vertex_style_closed");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(100.0, 100.0),
        Pos2::new(0.0, 100.0),
    ];
    let id = commit_polyline(&mut h, &pts, true);
    let widths = [2.0, 4.0, 6.0, 8.0];
    let reds = [0u8, 40, 80, 120];
    let corners = [Some(5.0), Some(10.0), Some(15.0), Some(20.0)];
    style_vertices(&mut h, id, &widths, &reds, &corners);
    let cutter = add_seg(&mut h.app, Pos2::new(50.0, -10.0), Pos2::new(50.0, 110.0));
    select_trim(&mut h.app, &[id, cutter]);
    h.app.set_board_tool(board::BoardTool::Split);
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.trim_click(Pos2::new(25.0, 50.0), false));
    h.frame();
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1, "two pieces");
    let rest = h.app.doc().scene.nodes.last().unwrap().id;
    for piece in [id, rest] {
        let tips = painted_vertex_tips(&h, piece);
        let overrides = corner_overrides(&h, piece);
        let at = world_vertices(&h, piece);
        assert_eq!(tips.len(), at.len());
        for (k, p) in at.iter().enumerate() {
            let what = format!("vertex {p:?}");
            let near = |q: Pos2| (*p - q).length() < 0.01;
            if let Some(i) = pts.iter().position(|q| near(*q)) {
                assert_vertex(tips[k], widths[i], reds[i] as f32, &what);
                assert_eq!(overrides[k], corners[i], "{what} keeps its corner");
            } else if near(Pos2::new(50.0, 0.0)) {
                assert_vertex(tips[k], 3.0, 20.0, &what);
                assert_eq!(overrides[k], None, "{what}: a cut has no override");
            } else if near(Pos2::new(50.0, 100.0)) {
                assert_vertex(tips[k], 7.0, 100.0, &what);
                assert_eq!(overrides[k], None, "{what}: a cut has no override");
            } else {
                panic!("unexpected {what}");
            }
        }
    }
}

/// User request (2026-09-26): a Direct Select join that merges the two
/// ends drops the merged end's entry; every other vertex keeps its width,
/// color and corner override.
#[test]
fn direct_join_merge_drops_only_the_merged_vertex_style() {
    let mut h = grip_board("direct_join_vertex_style");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(100.0, 100.0),
        Pos2::new(0.0, 100.0),
        Pos2::new(0.0, 2.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    style_vertices(
        &mut h,
        id,
        &[2.0, 4.0, 6.0, 8.0, 10.0],
        &[0, 40, 80, 120, 160],
        &[None, Some(5.0), Some(10.0), Some(15.0), None],
    );
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.app.direct.anchors = [0, 4].into_iter().collect();
    assert!(h.app.cmd_join());
    h.frame();
    let (_, s) = curve_shape(&h, id);
    assert!(s.path.as_ref().unwrap().closed, "the join closed the path");
    let tips = painted_vertex_tips(&h, id);
    assert_eq!(tips.len(), 4, "the merged end is gone");
    for (k, (w, r)) in [(2.0, 0.0), (4.0, 40.0), (6.0, 80.0), (8.0, 120.0)]
        .into_iter()
        .enumerate()
    {
        assert_vertex(tips[k], w, r, &format!("vertex {k}"));
    }
    assert_eq!(
        corner_overrides(&h, id),
        vec![None, Some(5.0), Some(10.0), Some(15.0)]
    );
}

/// A Direct Select edit that inserts a vertex (a closed path's seam turns
/// curved, so the closing span ends on a copy of the start) gives it the
/// start's style; every other vertex keeps its own.
#[test]
fn direct_seam_vertex_takes_the_start_style() {
    let mut h = grip_board("direct_seam_vertex_style");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(100.0, 100.0),
        Pos2::new(0.0, 100.0),
    ];
    let id = commit_polyline(&mut h, &pts, true);
    style_vertices(
        &mut h,
        id,
        &[2.0, 4.0, 6.0, 8.0],
        &[0, 40, 80, 120],
        &[Some(5.0), Some(10.0), Some(15.0), Some(20.0)],
    );
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    let screen = h.app.board_xf().w2s(Pos2::new(0.0, 0.0));
    assert!(h.app.direct_double_click(screen));
    h.frame();
    let tips = painted_vertex_tips(&h, id);
    assert_eq!(tips.len(), 5, "the curved seam adds a closing vertex");
    for (k, (w, r)) in [
        (2.0, 0.0),
        (4.0, 40.0),
        (6.0, 80.0),
        (8.0, 120.0),
        (2.0, 0.0),
    ]
    .into_iter()
    .enumerate()
    {
        assert_vertex(tips[k], w, r, &format!("vertex {k}"));
    }
    assert_eq!(
        corner_overrides(&h, id),
        vec![Some(5.0), Some(10.0), Some(15.0), Some(20.0), None]
    );
}

/// User request (2026-09-26): an arrow-key nudge of picked anchors keeps
/// every vertex's width, color and corner override.
#[test]
fn direct_nudge_keeps_vertex_style() {
    let mut h = grip_board("direct_nudge_vertex_style");
    let id = styled_cut_polyline(&mut h);
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.app.direct.anchors = [1].into_iter().collect();
    assert!(h.app.direct_nudge(0.0, 10.0));
    h.frame();
    assert_eq!(world_vertices(&h, id)[1], Pos2::new(100.0, 10.0));
    let tips = painted_vertex_tips(&h, id);
    assert_eq!(tips.len(), 4);
    for (k, (w, r)) in [(2.0, 0.0), (20.0, 200.0), (6.0, 100.0), (12.0, 50.0)]
        .into_iter()
        .enumerate()
    {
        assert_vertex(tips[k], w, r, &format!("vertex {k}"));
    }
    assert_eq!(
        corner_overrides(&h, id),
        vec![None, Some(8.0), Some(4.0), None]
    );
}

/// User request (2026-09-26): the object-level Join concatenates both
/// sources' per-vertex widths, colors and corner overrides in joined order;
/// a source the join reverses has its style reversed with it.
#[test]
fn object_join_concatenates_vertex_style_in_joined_order() {
    let mut h = grip_board("object_join_vertex_style");
    let a = commit_polyline(
        &mut h,
        &[
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 0.0),
            Pos2::new(100.0, 100.0),
        ],
        false,
    );
    style_vertices(
        &mut h,
        a,
        &[2.0, 4.0, 6.0],
        &[0, 40, 80],
        &[None, Some(5.0), None],
    );
    // Drawn away from `a`, so the join reverses it.
    let b = commit_polyline(
        &mut h,
        &[
            Pos2::new(300.0, 200.0),
            Pos2::new(300.0, 100.0),
            Pos2::new(150.0, 100.0),
        ],
        false,
    );
    style_vertices(
        &mut h,
        b,
        &[12.0, 10.0, 8.0],
        &[200, 160, 120],
        &[None, Some(9.0), None],
    );
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.cmd_join());
    h.frame();
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    assert_eq!(
        world_vertices(&h, id),
        vec![
            Pos2::new(0.0, 0.0),
            Pos2::new(100.0, 0.0),
            Pos2::new(100.0, 100.0),
            Pos2::new(150.0, 100.0),
            Pos2::new(300.0, 100.0),
            Pos2::new(300.0, 200.0),
        ]
    );
    let tips = painted_vertex_tips(&h, id);
    let want = [
        (2.0, 0.0),
        (4.0, 40.0),
        (6.0, 80.0),
        (8.0, 120.0),
        (10.0, 160.0),
        (12.0, 200.0),
    ];
    assert_eq!(tips.len(), want.len());
    for (k, (w, r)) in want.into_iter().enumerate() {
        assert_vertex(tips[k], w, r, &format!("vertex {k}"));
    }
    assert_eq!(
        corner_overrides(&h, id),
        vec![None, Some(5.0), None, None, Some(9.0), None]
    );
}

/// An object-level Join that reverses the first source and merges a
/// coincident seam keeps the first source's style at the seam.
#[test]
fn object_join_reverses_the_first_source_and_merges_the_seam() {
    let mut h = grip_board("object_join_vertex_style_merge");
    let a = commit_polyline(
        &mut h,
        &[
            Pos2::new(100.0, 100.0),
            Pos2::new(100.0, 0.0),
            Pos2::new(0.0, 0.0),
        ],
        false,
    );
    style_vertices(
        &mut h,
        a,
        &[6.0, 4.0, 2.0],
        &[80, 40, 0],
        &[None, Some(5.0), None],
    );
    let b = commit_polyline(
        &mut h,
        &[
            Pos2::new(100.0, 101.0),
            Pos2::new(200.0, 101.0),
            Pos2::new(200.0, 200.0),
        ],
        false,
    );
    style_vertices(
        &mut h,
        b,
        &[20.0, 10.0, 12.0],
        &[250, 160, 200],
        &[None, Some(9.0), None],
    );
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.cmd_join());
    h.frame();
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    let tips = painted_vertex_tips(&h, id);
    let want = [
        (2.0, 0.0),
        (4.0, 40.0),
        (6.0, 80.0),
        (10.0, 160.0),
        (12.0, 200.0),
    ];
    assert_eq!(tips.len(), want.len(), "the seam merged into one vertex");
    for (k, (w, r)) in want.into_iter().enumerate() {
        assert_vertex(tips[k], w, r, &format!("vertex {k}"));
    }
    assert_eq!(
        corner_overrides(&h, id),
        vec![None, Some(5.0), None, Some(9.0), None]
    );
}

/// User request (2026-09-26): a trimmed Bézier keeps painting the widths
/// and colors its source painted between vertices, not just at them: the
/// piece is cut in curve parameter space instead of on the flattened
/// polyline.
#[test]
fn trim_keeps_the_curve_profile_between_vertices() {
    let mut h = bezier_board("trim_curve_profile");
    for x in [0.0, 100.0, 200.0] {
        bezier_place(&mut h, Pos2::new(x, 0.0), Pos2::new(x + 30.0, 0.0));
    }
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    style_vertices(&mut h, id, &[2.0, 20.0, 2.0], &[0, 255, 0], &[]);
    let xs = [
        35.0, 50.0, 62.5, 80.0, 95.0, 110.0, 125.0, 150.0, 175.0, 190.0,
    ];
    let before: Vec<_> = xs
        .iter()
        .map(|x| ink_sample(&h, id, Pos2::new(*x, 0.0)))
        .collect();
    let cutter = add_seg(&mut h.app, Pos2::new(25.0, -10.0), Pos2::new(25.0, 10.0));
    select_trim(&mut h.app, &[id, cutter]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(10.0, 0.0), false));
    h.frame();

    for (x, was) in xs.iter().zip(&before) {
        let now = ink_sample(&h, id, Pos2::new(*x, 0.0));
        let what = format!("x={x}");
        assert_close(
            now.0,
            was.0,
            0.3,
            &format!("{what}: width as the source painted"),
        );
        assert_color_close(
            now.1,
            was.1,
            2.0,
            &format!("{what}: color as the source painted"),
        );
    }
    let (_, s) = curve_shape(&h, id);
    assert!(
        s.path
            .as_ref()
            .unwrap()
            .segs
            .iter()
            .any(|seg| matches!(seg, slate_doc::scene::PathSeg::Cubic { .. })),
        "the piece stays a curve"
    );
}
