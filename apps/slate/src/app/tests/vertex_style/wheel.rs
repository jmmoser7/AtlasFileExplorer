//! Color wheel picks and the style band through frames.

use super::*;

/// Through real frames: crossing the strip between the disk and the hue
/// ring keeps the color and never samples; the ring changes the hue.
#[test]
fn a_wheel_drag_across_the_gap_keeps_the_color_until_the_ring() {
    use board_color::{BrushHud, WHEEL_HUE_INNER, WHEEL_HUE_OUTER, WHEEL_SV_RADIUS};
    let mut h = line_board("wheel_gap_drag");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.set_active_rgb([40, 160, 60]);
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(ctrl_right(c, None));
    h.frame_with(ctrl_right(c, Some(true)));
    let center = match h.app.brush_hud {
        Some(BrushHud::Wheel { center, .. }) => center,
        ref other => panic!("wheel opened, got {other:?}"),
    };
    let rgb0 = h.app.active_rgba();
    let steps = 6;
    for k in 0..=steps {
        let d = WHEEL_SV_RADIUS
            + 0.5
            + (WHEEL_HUE_INNER - WHEEL_SV_RADIUS - 1.0) * k as f32 / steps as f32;
        h.frame_with(ctrl_right(center + EVec2::new(0.0, -d), None));
        assert_eq!(
            h.app.active_rgba(),
            rgb0,
            "the gap at {d} changed the color"
        );
        assert!(
            matches!(
                h.app.brush_hud,
                Some(BrushHud::Wheel {
                    sampling: false,
                    ..
                })
            ),
            "the gap at {d} reached for the eyedropper"
        );
    }
    let ring = (WHEEL_HUE_INNER + WHEEL_HUE_OUTER) * 0.5;
    h.frame_with(ctrl_right(center + EVec2::new(0.0, -ring), None));
    assert_ne!(h.app.active_rgba(), rgb0, "the hue ring picks");
    let Some(BrushHud::Wheel { hsv, .. }) = h.app.brush_hud else {
        panic!("wheel stays open");
    };
    assert!(
        (hsv[0] - 0.25).abs() < 0.02,
        "straight up is a quarter turn, hue {}",
        hsv[0]
    );
    h.frame_with(ctrl_right(center + EVec2::new(0.0, -ring), Some(false)));
}

/// The pointer warp lands on the swatch the wheel painted, including the
/// usual case where slot 0 already holds the current color.
#[test]
fn the_swatch_warp_lands_on_the_painted_swatch_center() {
    use board_color::{wheel_slot_center, BrushHud, WHEEL_DOT_RADIUS};
    let mut h = line_board("wheel_warp");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.set_active_rgb([200, 30, 30]);
    h.app.tab_mut().doc.view.recent_colors = Some(vec![[200, 30, 30], [20, 90, 220]]);
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(ctrl_right(c, None));
    h.frame_with(ctrl_right(c, Some(true)));
    let center = match h.app.brush_hud {
        Some(BrushHud::Wheel { center, .. }) => center,
        ref other => panic!("wheel opened, got {other:?}"),
    };
    for slot in [0usize, 1, 0] {
        let want = wheel_slot_center(center, slot);
        let near = want + EVec2::new(5.0, -3.0);
        let out = h.frame_output(ctrl_right(near, None));
        let warps: Vec<Pos2> = out
            .viewport_output
            .values()
            .flat_map(|v| v.commands.iter())
            .filter_map(|cmd| match cmd {
                egui::ViewportCommand::CursorPosition(p) => Some(*p),
                _ => None,
            })
            .collect();
        assert_eq!(warps, vec![want], "slot {slot} warp");
        let mut painted = Vec::new();
        fn walk(shape: &egui::Shape, acc: &mut Vec<Pos2>) {
            match shape {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
                egui::Shape::Circle(c)
                    if (c.radius - WHEEL_DOT_RADIUS).abs() < 1e-3 && c.fill.a() > 0 =>
                {
                    acc.push(c.center)
                }
                _ => {}
            }
        }
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut painted);
        }
        assert!(
            painted.contains(&want),
            "slot {slot}: warp {want:?} is not a painted swatch center {painted:?}"
        );
        // Once there, the swatch does not pull again.
        let out = h.frame_output(ctrl_right(want, None));
        assert!(out.viewport_output.values().all(|v| !v
            .commands
            .iter()
            .any(|c| matches!(c, egui::ViewportCommand::CursorPosition(_)))));
        h.frame_with(ctrl_right(center, None));
    }
    h.frame_with(ctrl_right(center, Some(false)));
}

/// Review r7 finding 20: a real drag onto the disk's white and black snap
/// regions lands on exact white and exact black, and the release keeps it.
#[test]
fn a_wheel_drag_onto_the_white_and_black_snaps_lands_exactly() {
    use board_color::{wheel_snaps, WHEEL_SNAP_RADIUS};
    let mut h = line_board("wheel_snap_drag");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.set_active_rgb([40, 160, 60]);
    h.frame();
    let at = h.app.canvas_rect.center();
    let center = open_wheel_frames(&mut h, at);
    let [(white, _), (black, _)] = wheel_snaps();
    let inside = |at: EVec2| at - at.normalized() * (WHEEL_SNAP_RADIUS * 0.6);
    h.frame_with(ctrl_right(center + white * 0.6, None));
    assert_ne!(
        h.app.active_rgba()[..3],
        [255, 255, 255],
        "short of the snap"
    );
    h.frame_with(ctrl_right(center + inside(white), None));
    assert_eq!(h.app.active_rgba()[..3], [255, 255, 255], "the white snap");
    h.frame_with(ctrl_right(center + black * 0.6, None));
    assert_ne!(h.app.active_rgba()[..3], [0, 0, 0], "short of the snap");
    let on_black = center + inside(black);
    h.frame_with(ctrl_right(on_black, None));
    assert_eq!(h.app.active_rgba()[..3], [0, 0, 0], "the black snap");
    h.frame_with(ctrl_right(on_black, Some(false)));
    h.frame();
    assert!(h.app.brush_hud.is_none());
    assert_eq!(
        h.app.active_rgba()[..3],
        [0, 0, 0],
        "the release keeps black"
    );
}

/// Review r7 finding 4 (Art. II): the open wheel repaints the same disk and
/// ring meshes each frame; a hue change rebuilds the disk only.
#[test]
fn the_open_wheel_repaints_its_cached_meshes() {
    use board_color::{WHEEL_HUE_INNER, WHEEL_HUE_OUTER};
    /// The meshes centered on the wheel, narrowest (the disk) first.
    fn meshes(out: &egui::FullOutput, center: Pos2) -> Vec<std::sync::Arc<egui::Mesh>> {
        fn walk(shape: &egui::Shape, acc: &mut Vec<std::sync::Arc<egui::Mesh>>) {
            match shape {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
                egui::Shape::Mesh(m) if m.vertices.len() > 100 => acc.push(m.clone()),
                _ => {}
            }
        }
        let mut acc = Vec::new();
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut acc);
        }
        acc.retain(|m| (m.calc_bounds().center() - center).length() < 2.0);
        acc.sort_by(|a, b| a.calc_bounds().width().total_cmp(&b.calc_bounds().width()));
        acc
    }
    let mut h = line_board("wheel_mesh_cache");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.set_active_rgb([200, 30, 30]);
    h.frame();
    let at = h.app.canvas_rect.center();
    let center = open_wheel_frames(&mut h, at);
    let a = meshes(&h.frame_output(ctrl_right(at, None)), center);
    let b = meshes(
        &h.frame_output(ctrl_right(at + EVec2::new(3.0, 0.0), None)),
        center,
    );
    assert_eq!(a.len(), 2, "the disk and the ring");
    assert_eq!(b.len(), 2);
    assert!(std::sync::Arc::ptr_eq(&a[0], &b[0]), "the disk is reused");
    assert!(std::sync::Arc::ptr_eq(&a[1], &b[1]), "the ring is reused");
    let ring = center + EVec2::new(0.0, -(WHEEL_HUE_INNER + WHEEL_HUE_OUTER) * 0.5);
    h.frame_with(ctrl_right(ring, None));
    let c = meshes(&h.frame_output(ctrl_right(ring, None)), center);
    assert_eq!(c.len(), 2);
    assert!(
        !std::sync::Arc::ptr_eq(&a[0], &c[0]),
        "a new hue, a new disk"
    );
    assert!(
        std::sync::Arc::ptr_eq(&a[1], &c[1]),
        "the ring never changes"
    );
    h.frame_with(ctrl_right(ring, Some(false)));
}

/// Review r7 finding 3 (P1.curve.create-style): a curve tool's wheel pick
/// sets that tool's color and leaves the brush color where it was.
#[test]
fn a_curve_tool_wheel_pick_keeps_the_brush_color() {
    let mut h = line_board("curve_wheel_own_color");
    h.frame();
    let fg = h.app.board_colors.fg;
    let before = h.app.stroke_for_new_curve().color;
    let at = h.app.canvas_rect.center();
    let center = open_wheel_frames(&mut h, at);
    let target = bright_on_disk(center);
    h.frame_with(ctrl_right(target, None));
    h.frame_with(ctrl_right(target, Some(false)));
    h.frame();
    assert!(h.app.brush_hud.is_none());
    let after = h.app.stroke_for_new_curve().color;
    assert_ne!(after.0[..3], before.0[..3], "the Line tool takes the color");
    assert_eq!(h.app.board_colors.fg, fg, "the brush color does not move");
}

/// Review r7 finding 20: `the_style_band_enters_each_texture_hard_and_holds_size`
/// through real frames. Alt+right on the board, down toward the band
/// (softness falls), across the band (size and softness hold), release on
/// a texture.
#[test]
fn the_style_band_through_frames_enters_a_texture_hard_and_holds_size() {
    use board_tip_hud::{palette_band_y, palette_hit, palette_slot};
    let mut h = brush_board("tip_band_frames");
    h.app.brush_width = 20.0;
    h.app.brush_softness = 0.8;
    h.frame();
    let press = h.app.canvas_rect.center();
    h.frame_with(alt_right(press, None));
    h.frame_with(alt_right(press, Some(true)));
    assert!(
        matches!(h.app.brush_hud, Some(board_color::BrushHud::Size { .. })),
        "Alt+right opens the size HUD"
    );
    let r = 10.0;
    let band = palette_band_y(press, r);
    h.frame_with(alt_right(
        Pos2::new(press.x, press.y + (band - press.y) * 0.5),
        None,
    ));
    assert!(
        h.app.brush_softness <= 0.4 + 1e-4,
        "{}",
        h.app.brush_softness
    );
    let n = h.app.tip_choices().len();
    let row: Vec<Pos2> = (0..n).map(|i| palette_slot(press, r, i, n)).collect();
    for p in [
        Pos2::new(press.x, band + 1.0),
        Pos2::new(press.x + 300.0, band + 4.0),
        row[0] + EVec2::new(-60.0, 0.0),
        row[1],
    ] {
        h.frame_with(alt_right(p, None));
        assert_eq!(h.app.brush_width, 20.0, "size held in the band at {p:?}");
        assert_eq!(h.app.brush_softness, 0.0, "hardest in the band at {p:?}");
    }
    assert_eq!(palette_hit(press, r, n, row[1]), Some(1));
    h.frame_with(alt_right(row[1], Some(false)));
    h.frame();
    assert!(h.app.brush_hud.is_none());
    assert_eq!(h.app.brush_width, 20.0);
    assert_eq!(
        h.app.brush_softness, 0.0,
        "the release keeps the hardest edge"
    );
    assert_eq!(h.app.current_tip_choice(), Some(h.app.tip_choices()[1]));
}

/// Review r7 findings 5 and 13: a Select-tool grip pick takes the Ctrl and
/// Shift+right chords through frames. A vertex's opacity reads and writes
/// as the opacity it paints at (node opacity × its alpha): raising one
/// vertex past the node's opacity lifts the node, and the others keep
/// painting as before.
#[test]
fn a_select_grip_pick_takes_the_wheel_and_painted_opacity_through_frames() {
    let mut h = grip_board("hud_select_grip_chords");
    let (id, pts) = hud_polyline(&mut h);
    h.app.patch_nodes(&[id], |n| n.opacity = 0.5);
    h.frame();
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(pts[2]), egui::Modifiers::NONE);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![2])),
        "the click picks vertex 2"
    );
    let away = xf.w2s(pts[1] + EVec2::new(0.0, 160.0));
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(away)));
    let (_, _, opacity) = h.app.vector_tip().expect("a readout");
    assert!(
        (opacity - 0.5).abs() < 1e-3,
        "the readout is painted opacity: {opacity}"
    );
    let before = painted_vertex_tips(&h, id);
    let depth = h.app.tab().journal.undo_depth();

    let center = open_wheel_frames(&mut h, away);
    let target = bright_on_disk(center);
    h.frame_with(ctrl_right(target, None));
    h.frame_with(ctrl_right(target, Some(false)));
    h.frame();
    let colored = painted_vertex_tips(&h, id);
    assert_ne!(
        colored[2].1[..3],
        before[2].1[..3],
        "vertex 2 takes the color"
    );
    assert_eq!(colored[0], before[0], "vertex 0 keeps its color");
    assert_eq!(colored[1], before[1], "vertex 1 keeps its color");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);

    hud_scrub(
        &mut h,
        away,
        EVec2::new(0.0, -120.0),
        egui::Modifiers::SHIFT,
    );
    let node_opacity = h.app.doc().scene.node(id).unwrap().opacity;
    assert!(
        (node_opacity - 1.0).abs() < 1e-3,
        "the node rises to vertex 2: {node_opacity}"
    );
    let tips = painted_vertex_tips(&h, id);
    let painted = |k: usize| node_opacity * tips[k].1[3] as f32 / 255.0;
    assert!(
        (painted(2) - 1.0).abs() < 1e-3,
        "vertex 2 paints fully opaque"
    );
    for k in [0, 1] {
        assert!(
            (painted(k) - 0.5).abs() < 0.01,
            "vertex {k} still paints at 50 %: {tips:?}"
        );
    }
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().opacity,
        0.5,
        "one Ctrl+Z"
    );
    assert_eq!(painted_vertex_tips(&h, id), colored);
}
