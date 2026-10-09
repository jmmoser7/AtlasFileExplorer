//! Curve style HUD, textures, opacity, arrows, and taper.

use super::*;

#[test]
fn curve_tools_take_the_size_color_and_opacity_hud() {
    let mut h = Harness::new("curve_hud");
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.tab_mut().cam.z = 1.0;
    let w0 = h.app.stroke_for_new_curve().width;
    h.app.alt_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(30.0, -80.0)), true, false));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(30.0, -80.0)), false, false));
    let s = h.app.stroke_for_new_curve();
    assert!((s.width - (w0 + 60.0)).abs() < 0.5, "width {}", s.width);
    assert_eq!(s.softness, 0.0, "vector curves have no softness");
    h.app.alt_down = false;
    h.app.shift_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 50.0)), true, false));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 50.0)), false, false));
    assert!((h.app.opacity_for_new_node(false) - 0.5).abs() < 0.02);
    h.app.shift_down = false;
    h.app.ctrl_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Wheel { .. })
    ));
    h.app.set_active_rgb([10, 200, 30]);
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 0.0)), false, false));
    assert_eq!(h.app.stroke_for_new_curve().color.0[..3], [10, 200, 30]);
}

#[test]
fn the_size_hud_palette_picks_curve_styles_and_brush_textures() {
    use board_tip_hud::{palette_slot, CurveStyle, TipChoice};
    let mut h = Harness::new("tip_palette");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Line);
    h.app.alt_down = true;
    let press = Pos2::new(200.0, 200.0);
    assert!(h.app.drive_brush_hud(Some(press), true, true));
    let r = (h.app.active_tip().0 * 0.5).max(1.5);
    let n = h.app.tip_choices().len();
    let arrow = palette_slot(press, r, 2, n);
    assert!(h.app.drive_brush_hud(Some(arrow), true, false));
    assert!(h.app.drive_brush_hud(Some(arrow), false, false));
    assert_eq!(
        h.app.current_tip_choice(),
        Some(TipChoice::Curve(CurveStyle::Arrow))
    );
    assert!(h.app.stroke_for_new_curve().arrow_end);
    h.app
        .apply_tip_choice(TipChoice::Curve(CurveStyle::TaperBoth));
    assert!(matches!(
        h.app.stroke_for_new_curve().profile,
        slate_doc::scene::WidthProfile::Ends { .. }
    ));

    h.app.set_board_tool(board::BoardTool::Brush);
    h.app
        .apply_tip_choice(TipChoice::Texture(slate_doc::scene::BrushTexture::Pencil));
    h.app
        .finish_freehand_brush(vec![Pos2::new(0.0, 0.0), Pos2::new(80.0, 0.0)]);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("brush path");
    };
    assert_eq!(shape.stroke.texture, slate_doc::scene::BrushTexture::Pencil);
}

/// Dragging down toward the style row drives softness to its hardest; the
/// row is a full-width band where size and softness hold wherever the
/// pointer goes, so the row never moves under it; the reached softness is
/// what the release saves.
#[test]
fn the_style_band_enters_each_texture_hard_and_holds_size() {
    use board_tip_hud::{palette_band_y, palette_hit, palette_slot};
    let mut h = Harness::new("tip_band");
    h.app.tab_mut().cam.z = 1.0;
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 20.0;
    h.app.brush_softness = 0.8;
    h.app.alt_down = true;
    let press = Pos2::new(300.0, 200.0);
    assert!(h.app.drive_brush_hud(Some(press), true, true));
    let r = 10.0;
    let band = palette_band_y(press, r);
    // Halfway down to the band: already harder than a plain scrub.
    let mid = Pos2::new(press.x, press.y + (band - press.y) * 0.5);
    assert!(h.app.drive_brush_hud(Some(mid), true, false));
    assert!(
        h.app.brush_softness <= 0.4 + 1e-4,
        "{}",
        h.app.brush_softness
    );
    let n = h.app.tip_choices().len();
    let row: Vec<Pos2> = (0..n).map(|i| palette_slot(press, r, i, n)).collect();
    for p in [
        Pos2::new(press.x, band + 1.0),
        Pos2::new(press.x + 900.0, band + 4.0),
        Pos2::new(press.x - 900.0, band + 300.0),
        row[0] + EVec2::new(-60.0, 0.0),
        row[1],
    ] {
        assert!(h.app.drive_brush_hud(Some(p), true, false));
        assert_eq!(h.app.brush_width, 20.0, "size held in the band at {p:?}");
        assert_eq!(h.app.brush_softness, 0.0, "hardest in the band at {p:?}");
        let r_now = h.app.active_tip().0 * 0.5;
        for (i, at) in row.iter().enumerate() {
            assert_eq!(palette_slot(press, r_now, i, n), *at, "the row moved");
        }
    }
    assert_eq!(palette_hit(press, r, n, row[1]), Some(1));
    assert!(h.app.drive_brush_hud(Some(row[1]), false, false));
    assert_eq!(
        h.app.brush_softness, 0.0,
        "the release keeps the hardest edge"
    );
    assert_eq!(h.app.current_tip_choice(), Some(h.app.tip_choices()[1]));
}

/// Opacity goes all the way to 0 %, and a 0 % stroke is still picked by
/// its geometry.
#[test]
fn opacity_reaches_zero_and_a_clear_stroke_still_picks() {
    let mut h = Harness::new("opacity_zero");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_opacity = 0.3;
    h.app.shift_down = true;
    assert!(h.app.drive_brush_hud(Some(Pos2::new(0.0, 0.0)), true, true));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 400.0)), true, false));
    assert!(h
        .app
        .drive_brush_hud(Some(Pos2::new(0.0, 400.0)), false, false));
    assert_eq!(h.app.brush_opacity, 0.0);
    h.app.shift_down = false;
    h.app.brush_width = 12.0;
    h.app.finish_freehand_brush(vec![
        Pos2::new(100.0, 100.0),
        Pos2::new(160.0, 110.0),
        Pos2::new(220.0, 100.0),
    ]);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    let picked = board_path::board_pick_node(&h.app.doc().scene, 160.0, 108.0, 1.0);
    assert_eq!(picked, Some(id), "a 0 % brush stroke stays pickable");

    let line = add_seg(&mut h.app, Pos2::new(0.0, 300.0), Pos2::new(200.0, 300.0));
    h.app.patch_nodes(&[line], |n| {
        if let slate_doc::scene::NodeKind::Shape(sh) = &mut n.kind {
            sh.stroke.color.0[3] = 0;
            sh.stroke.width = 6.0;
        }
    });
    let picked = board_path::board_pick_node(&h.app.doc().scene, 100.0, 301.0, 1.0);
    assert_eq!(picked, Some(line), "a 0 % curve stays pickable");
}

/// A committed arrow curve ends in a real head on the board mesh and in the
/// HTML export: the tip on the curve's end, the base back on the curve (not
/// aimed along a last-moment hook), and the body stopping under the head.
#[test]
fn a_committed_arrow_curve_has_its_head_on_board_and_in_export() {
    use board_tip_hud::{CurveStyle, TipChoice};
    let mut h = Harness::new("arrow_commit");
    h.app.set_board_tool(board::BoardTool::Pen);
    h.app.apply_tip_choice(TipChoice::Curve(CurveStyle::Arrow));
    let pts: Vec<Pos2> = (0..=20)
        .map(|k| Pos2::new(10.0 * k as f32, 40.0 * (k as f32 * 0.15).sin()))
        .chain([Pos2::new(203.0, 12.0)])
        .collect();
    let (rect, data) = board_path::points_to_path_data(&pts, false);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Pen, rect, data, false);
    let node = h.app.doc().scene.nodes.last().unwrap().clone();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("curve");
    };
    assert!(shape.stroke.arrow_end);
    let path = shape.path.as_ref().unwrap();
    let bez = board_path::shape_path_world_bez(&node, shape, path);
    let (tip, into) = slate_doc::geom::path_end_arrow(&bez, shape.stroke.width).unwrap();
    let end = *pts.last().unwrap();
    assert!(Pos2::new(tip[0], tip[1]).distance(end) < 0.5);
    let len = slate_doc::geom::arrow_len(shape.stroke.width);
    let base = Pos2::new(tip[0] + into[0] * len, tip[1] + into[1] * len);
    let mut flat = Vec::new();
    vector_ink::kurbo::flatten(bez.iter(), 0.02, |el| match el {
        vector_ink::kurbo::PathEl::MoveTo(p) | vector_ink::kurbo::PathEl::LineTo(p) => {
            flat.push(Pos2::new(p.x as f32, p.y as f32))
        }
        _ => {}
    });
    let near_curve = flat
        .windows(2)
        .map(|w| board_color::dist_point_segment(base, w[0], w[1]))
        .fold(f32::MAX, f32::min);
    assert!(
        near_curve < 1.5,
        "the head's base sits {near_curve} off the curve"
    );

    let ink = board_path::vector_stroke_ink(&node, shape, path, 1.0);
    let n = ink.vertices.len();
    assert!(n > 6);
    let head: Vec<Pos2> = ink.vertices[n - 6..n - 3]
        .iter()
        .map(|v| Pos2::new(v.pos[0], v.pos[1]))
        .collect();
    assert_eq!(head[0], Pos2::new(tip[0], tip[1]), "board head tip");
    let mid = Pos2::new((head[1].x + head[2].x) * 0.5, (head[1].y + head[2].y) * 0.5);
    assert!(mid.distance(base) < 1e-3, "board head base");
    let trim = slate_doc::geom::arrow_trim(shape.stroke.width) as f32;
    let body_reach = ink.vertices[..n - 6]
        .iter()
        .map(|v| Pos2::new(v.pos[0], v.pos[1]).distance(end))
        .fold(f32::MAX, f32::min);
    assert!(
        body_reach > trim - shape.stroke.width * 2.0,
        "the body runs {body_reach} from the tip, under a {trim} trim"
    );

    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    let want = format!("M {:.1} {:.1} L", tip[0], tip[1]);
    let at = html
        .find(&want)
        .unwrap_or_else(|| panic!("export head {want}"));
    let nums: Vec<f32> = html[at..]
        .split('Z')
        .next()
        .unwrap()
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    assert_eq!(nums.len(), 6, "a triangle");
    let emid = Pos2::new((nums[2] + nums[4]) * 0.5, (nums[3] + nums[5]) * 0.5);
    assert!(
        emid.distance(base) < 0.2,
        "export head base {emid:?} vs {base:?}"
    );
}

/// A taper-both straight Line swells to full width in the middle instead of
/// staying a hairline between its two anchors.
#[test]
fn a_taper_both_line_swells_in_the_middle() {
    use board_tip_hud::{CurveStyle, TipChoice};
    let mut h = Harness::new("taper_line");
    h.app.set_board_tool(board::BoardTool::Line);
    h.app
        .apply_tip_choice(TipChoice::Curve(CurveStyle::TaperBoth));
    h.app.set_vector_tip(20.0, 0.0, 1.0);
    let (rect, data) =
        board_path::points_to_path_data(&[Pos2::new(0.0, 0.0), Pos2::new(300.0, 0.0)], false);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Line, rect, data, false);
    let node = h.app.doc().scene.nodes.last().unwrap().clone();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("line");
    };
    let path = shape.path.as_ref().unwrap();
    let ink = board_path::vector_stroke_ink(&node, shape, path, 1.0);
    let half_at = |x0: f32, x1: f32| {
        ink.vertices
            .iter()
            .filter(|v| v.alpha >= 0.99 && v.pos[0] >= x0 && v.pos[0] <= x1)
            .map(|v| v.pos[1].abs())
            .fold(0.0_f32, f32::max)
    };
    let w = shape.stroke.width;
    assert!(
        half_at(140.0, 160.0) > w * 0.4,
        "middle half-width {}",
        half_at(140.0, 160.0)
    );
    assert!(
        half_at(0.0, 3.0) < w * 0.3,
        "start tapers, {}",
        half_at(0.0, 3.0)
    );
    assert!(
        half_at(297.0, 300.0) < w * 0.3,
        "end tapers, {}",
        half_at(297.0, 300.0)
    );
}
