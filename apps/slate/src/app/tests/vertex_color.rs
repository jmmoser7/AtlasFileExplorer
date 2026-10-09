//! Per-vertex stroke color tests.

use super::*;

/// User request (2026-09-26): with a vertex picked, the Stroke color edits
/// only that vertex and a polyline blends straight to its neighbors; with
/// no vertex picked, the color edit sets every vertex.
#[test]
fn a_vertex_color_edit_blends_straight_to_its_neighbors() {
    let mut h = grip_board("vertex_color_linear");
    let pts = [
        Pos2::new(0.0, 0.0),
        Pos2::new(100.0, 0.0),
        Pos2::new(200.0, 0.0),
    ];
    let id = commit_polyline(&mut h, &pts, false);
    let c0 = curve_shape(&h, id).1.stroke.color.0;
    let red = [255, 0, 0, c0[3]];
    let depth = h.app.tab().journal.undo_depth();
    vertex_stringer_color(&mut h, id, &[1], [255, 0, 0]);
    assert_eq!(tip_colors(&h, id), vec![c0, red, c0]);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one patch");
    for (x, t) in [(50.0, 0.5), (25.0, 0.75), (150.0, 0.5)] {
        let got = ink_color_at(&h, id, Pos2::new(x, 0.0));
        assert_color_close(got, mix_rgba(red, c0, t), 1.5, &format!("x={x}"));
    }

    vertex_stringer_width(&mut h, id, &[0], 12.0);
    vertex_stringer_color(&mut h, id, &[], [0, 0, 255]);
    let blue = [0, 0, 255, c0[3]];
    assert_eq!(tip_colors(&h, id), vec![blue; 3], "whole curve sets all");
    assert_eq!(curve_shape(&h, id).1.stroke.color.0, blue);
    assert_eq!(tip_widths(&h, id)[0], 12.0, "widths are kept");
}

/// A Bézier span blends vertex colors by the same smoothstep as widths.
#[test]
fn bezier_vertex_colors_blend_smoothly() {
    let mut h = bezier_board("vertex_color_smooth");
    for x in [0.0, 100.0, 200.0] {
        bezier_place(&mut h, Pos2::new(x, 0.0), Pos2::new(x + 30.0, 0.0));
    }
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let c0 = curve_shape(&h, id).1.stroke.color.0;
    let red = [255, 0, 0, c0[3]];
    vertex_stringer_color(&mut h, id, &[1], [255, 0, 0]);
    for x in [25.0_f32, 50.0, 90.0] {
        let got = ink_color_at(&h, id, Pos2::new(x, 0.0));
        let want = mix_rgba(c0, red, smoothstep(x / 100.0));
        assert_color_close(got, want, 2.0, &format!("smooth at x={x}"));
    }
}

/// The export paints the same color blend the board paints, with SVG
/// linear gradients (Art. IV: SVG-expressible).
#[test]
fn vertex_colors_export_as_gradients_matching_the_board() {
    let mut h = bezier_board("vertex_color_export");
    for x in [0.0, 100.0, 200.0] {
        bezier_place(&mut h, Pos2::new(x, 0.0), Pos2::new(x + 30.0, 0.0));
    }
    assert!(h.app.finish_path_draft());
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    vertex_stringer_width(&mut h, id, &[1], 16.0);
    vertex_stringer_color(&mut h, id, &[1], [255, 0, 0]);
    vertex_stringer_color(&mut h, id, &[2], [0, 0, 255]);
    let (n, _) = curve_shape(&h, id);
    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    assert!(
        html.contains("<linearGradient"),
        "the blend exports as gradients"
    );
    let local = |x: f32, y: f32| [x - n.rect.x, y - n.rect.y];
    for x in [12.0_f32, 50.0, 88.0, 130.0, 170.0] {
        for y in [0.0_f32, 0.5] {
            let board = ink_color_at(&h, id, Pos2::new(x, y));
            let export = export_color_at(&html, local(x, y))
                .unwrap_or_else(|| panic!("the export paints ({x}, {y})"));
            assert_color_close(export, board, 3.0, &format!("export at ({x}, {y})"));
        }
    }
}
