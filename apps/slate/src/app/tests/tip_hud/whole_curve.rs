//! Whole-curve tip HUD scale, color, fade, and style row.

use super::*;

/// User, 28 September 2026 (ed1): "implementthis function when the user
/// has a full curveselected. simpl alt + drag". Alt+right-drag away from
/// the grips scales every tip by one factor, so the taper is kept. One
/// Ctrl+Z restores and Ctrl+Y re-applies (ed4).
#[test]
fn select_tool_alt_right_drag_scales_the_whole_selected_curve() {
    let (mut h, id, _, away) = whole_curve_board("hud_select_whole_width");
    let before = painted_vertex_tips(&h, id);
    let depth = h.app.tab().journal.undo_depth();
    hud_scrub(&mut h, away, EVec2::new(40.0, 0.0), egui::Modifiers::ALT);
    let tips = painted_vertex_tips(&h, id);
    assert!(tips[1].0 > before[1].0 + 10.0, "the curve widens: {tips:?}");
    for k in [0, 2] {
        let ratio = tips[k].0 / tips[1].0;
        assert!(
            (ratio - 0.5).abs() < 1e-3,
            "vertex {k} keeps the taper: {tips:?}"
        );
        assert_eq!(tips[k].1, before[k].1, "vertex {k} keeps its color");
    }
    assert!(h.app.board_sel.contains(&id), "the curve stays selected");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(painted_vertex_tips(&h, id), before, "one Ctrl+Z restores");
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(painted_vertex_tips(&h, id), tips, "Ctrl+Y re-applies");
}

/// User, 28 September 2026 (ed2): "for color holding ctrl it acts like the
/// phoitoshop hue picker sifting the rgb values from thercurent position
/// rther tahn ovewriting them. so gradient curve staysgradient but whth
/// translated rgb". The wheel opens on the curve's first color; the pick
/// moves every vertex by the same hue, saturation and value change.
#[test]
fn select_tool_ctrl_right_drag_shifts_the_whole_curve_color() {
    let (mut h, id, _, away) = whole_curve_board("hud_select_whole_color");
    let before = painted_vertex_tips(&h, id);
    let depth = h.app.tab().journal.undo_depth();
    let center = open_wheel_frames(&mut h, away);
    match h.app.brush_hud {
        Some(board_color::BrushHud::Wheel { fg0, .. }) => {
            assert_eq!(
                fg0[..3],
                [200, 40, 40],
                "the wheel opens on the curve's color"
            )
        }
        ref other => panic!("the wheel, got {other:?}"),
    }
    let target = center + EVec2::new(-25.0, 20.0);
    h.frame_with(ctrl_right(target, None));
    h.frame_with(ctrl_right(target, Some(false)));
    h.frame();
    assert!(h.app.brush_hud.is_none());
    let tips = painted_vertex_tips(&h, id);
    let rgb = |c: [u8; 4]| [c[0], c[1], c[2]];
    let pick = rgb(tips[0].1);
    assert_ne!(pick, [200, 40, 40], "the pick moved the color");
    for (k, tip) in tips.iter().enumerate() {
        let want = board_color::shift_hsv(rgb(before[k].1), [200, 40, 40], pick);
        assert_eq!(rgb(tip.1), want, "vertex {k} shifts from its own color");
        assert_eq!(tip.0, before[k].0, "vertex {k} keeps its width");
    }
    let hue = |c: [u8; 4]| board_color::rgb_to_hsv(rgb(c))[0];
    assert!(
        (hue(tips[1].1) - 1.0 / 3.0).abs() < 0.02,
        "green stays green: {tips:?}"
    );
    assert!(
        (hue(tips[2].1) - 2.0 / 3.0).abs() < 0.02,
        "blue stays blue: {tips:?}"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(painted_vertex_tips(&h, id), before);
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    assert_eq!(painted_vertex_tips(&h, id), tips);

    // Esc restores and journals nothing (ed5).
    let center = open_wheel_frames(&mut h, away);
    h.frame_with(ctrl_right(center + EVec2::new(30.0, -30.0), None));
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.brush_hud.is_none(), "Esc closes the wheel");
    h.frame_with(ctrl_right(away, Some(false)));
    h.frame();
    assert_eq!(painted_vertex_tips(&h, id), tips, "Esc restores");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}

/// ed1 names every curve kind: a Line and a Bézier span selected with the
/// Select tool take the whole-curve size HUD too.
#[test]
fn select_tool_whole_curve_hud_reaches_lines_and_bezier_spans() {
    let mut h = bezier_board("hud_select_whole_kinds");
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    for (press, handle) in [
        (
            c + EVec2::new(-150.0, -150.0),
            c + EVec2::new(-150.0, -150.0),
        ),
        (c + EVec2::new(0.0, -150.0), c + EVec2::new(40.0, -150.0)),
        (c + EVec2::new(150.0, -100.0), c + EVec2::new(150.0, -100.0)),
    ] {
        h.app.bezier_anchor_press(press);
        h.app.bezier_anchor_release(press, handle, false);
    }
    assert!(h.app.path_tool_try_finish());
    let bez = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_board_tool(board::BoardTool::Line);
    let line = h
        .app
        .commit_line(c + EVec2::new(-150.0, 0.0), c + EVec2::new(150.0, 0.0))
        .expect("a line");
    h.app.set_board_tool(board::BoardTool::Select);
    let away = h.app.board_xf().w2s(c + EVec2::new(0.0, 200.0));
    assert!(h.app.canvas_rect.contains(away));
    for id in [bez, line] {
        h.app.board_sel = [id].into_iter().collect();
        h.frame();
        let w0 = curve_shape(&h, id).1.stroke.width;
        let depth = h.app.tab().journal.undo_depth();
        hud_scrub(&mut h, away, EVec2::new(40.0, 0.0), egui::Modifiers::ALT);
        let w = curve_shape(&h, id).1.stroke.width;
        assert!(w > w0 + 10.0, "{id:?} widens: {w0} -> {w}");
        assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    }
}

/// The relative shift itself: a hue rotation carries every color around
/// the wheel together, and a gray has no hue to rotate.
#[test]
fn a_relative_color_shift_rotates_every_hue_together() {
    use board_color::shift_hsv;
    let (red, green, blue) = ([200, 40, 40], [40, 200, 40], [40, 40, 200]);
    assert_eq!(
        shift_hsv(red, red, [10, 200, 30]),
        [10, 200, 30],
        "the press color lands on the pick"
    );
    assert_eq!(shift_hsv(green, red, green), blue, "a third of a turn");
    assert_eq!(shift_hsv(blue, red, green), red);
    assert_eq!(
        shift_hsv(green, [128, 128, 128], [128, 128, 128]),
        green,
        "no change, no shift"
    );
    let gray = [100, 100, 100];
    assert_eq!(
        shift_hsv(gray, red, [40, 40, 200]),
        gray,
        "same saturation: a gray stays gray"
    );
    let tinted = shift_hsv(gray, [200, 100, 100], [200, 40, 200]);
    assert!(
        tinted[0] == tinted[2] && tinted[0] > tinted[1],
        "more saturation tints a gray with the pick's hue: {tinted:?}"
    );
}

/// User, 28 September 2026 (ed3): Shift+right-drag scales the whole
/// curve's opacity from where each vertex was; it reaches 0 %, and a 0 %
/// curve still selects when its line is clicked.
#[test]
fn select_tool_shift_right_drag_fades_the_whole_curve_relatively() {
    let (mut h, id, pts, away) = whole_curve_board("hud_select_whole_opacity");
    tip_vertices(
        &mut h,
        id,
        &[4.0, 8.0, 4.0],
        &[[200, 40, 40, 255], [40, 200, 40, 128], [40, 40, 200, 255]],
    );
    let before = painted_vertex_tips(&h, id);
    let depth = h.app.tab().journal.undo_depth();
    hud_scrub(&mut h, away, EVec2::new(0.0, 50.0), egui::Modifiers::SHIFT);
    let node_opacity = h.app.doc().scene.node(id).unwrap().opacity;
    assert!(
        node_opacity < 0.9 && node_opacity > 0.0,
        "down fades: {node_opacity}"
    );
    assert_eq!(
        painted_vertex_tips(&h, id),
        before,
        "each vertex keeps its share"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    hud_scrub(&mut h, away, EVec2::new(0.0, 400.0), egui::Modifiers::SHIFT);
    assert_eq!(
        h.app.doc().scene.node(id).unwrap().opacity,
        0.0,
        "down to 0 %"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

    let xf = h.app.board_xf();
    let empty = xf.w2s(pts[1] + EVec2::new(0.0, -300.0));
    press_primary(&mut h, empty, egui::Modifiers::NONE);
    assert!(
        h.app.board_sel.is_empty(),
        "a click on empty board deselects"
    );
    pause(&mut h);
    let on_line = xf.w2s(pts[0].lerp(pts[1], 0.5));
    press_primary(&mut h, on_line, egui::Modifiers::NONE);
    assert!(
        h.app.board_sel.contains(&id),
        "a 0 % curve still selects on its line"
    );
}

/// eb1 (user, 28 September 2026): a painted stroke selected with the Select
/// tool takes the whole-curve size HUD. Every tip scales in proportion,
/// the widest tip lands on the value shown, vertical travel changes
/// softness in proportion too, one undo step per HUD.
#[test]
fn select_tool_alt_right_drag_scales_a_whole_brush_stroke() {
    let mut h = brush_board("hud_select_whole_brush");
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.frame();
    let xf = h.app.board_xf();
    let c = xf.s2w(h.app.canvas_rect.center());
    h.app.finish_freehand_brush(
        (0..=8)
            .map(|k| c + EVec2::new(-160.0 + 40.0 * k as f32, 0.0))
            .collect(),
    );
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    let stamp_tips = |h: &Harness| {
        let (_, s) = curve_shape(h, id);
        assert!(s.stroke.paints_as_stamp(), "painted ink");
        s.path.as_ref().unwrap().paint_tips(&s.stroke)
    };
    {
        let n = h.app.doc_mut().scene.node_mut(id).unwrap();
        let NodeKind::Shape(s) = &mut n.kind else {
            panic!("a shape")
        };
        let mut path = s.path.as_deref().cloned().unwrap();
        let count = path.segs.len() + 1;
        let tip = slate_doc::scene::StrokeSpan::of(&s.stroke);
        path.tips = (0..count)
            .map(|k| {
                let wide = k == count / 2;
                slate_doc::scene::StrokeSpan {
                    width: if wide { 12.0 } else { 6.0 },
                    softness: if wide { 0.4 } else { 0.2 },
                    ..tip
                }
            })
            .collect();
        s.stroke.width = 12.0;
        s.stroke.softness = 0.4;
        s.path = Some(path.into());
    }
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.board_sel = [id].into_iter().collect();
    h.frame();
    let before = stamp_tips(&h);
    let depth = h.app.tab().journal.undo_depth();
    let away = xf.w2s(c + EVec2::new(0.0, 160.0));

    h.frame_with(alt_right(away, None));
    h.frame_with(alt_right(away, Some(true)));
    assert!(
        matches!(h.app.brush_hud, Some(board_color::BrushHud::Size { .. })),
        "Alt+right opens the size HUD on the selected stroke"
    );
    h.frame_with(alt_right(away + EVec2::new(30.0, 0.0), None));
    let shown = h.app.active_tip().0;
    let during = stamp_tips(&h);
    let widest = during.iter().map(|t| t.width).fold(0.0_f32, f32::max);
    assert!(
        (widest - shown).abs() < 1e-3,
        "the widest tip is the value shown"
    );
    assert!(widest > 12.0 + 10.0, "the stroke widens: {widest}");
    for (t, b) in during.iter().zip(&before) {
        assert!(
            (t.width / widest - b.width / 12.0).abs() < 1e-3,
            "in proportion"
        );
    }
    h.frame_with(alt_right(away + EVec2::new(30.0, -40.0), None));
    let softer = stamp_tips(&h);
    let softest = softer.iter().map(|t| t.softness).fold(0.0_f32, f32::max);
    assert!(softest > 0.4 + 0.05, "up softens: {softest}");
    for (t, b) in softer.iter().zip(&before) {
        assert!(
            (t.softness / softest - b.softness / 0.4).abs() < 1e-3,
            "softness in proportion"
        );
    }
    h.frame_with(alt_right(away + EVec2::new(30.0, -40.0), Some(false)));
    h.frame();
    assert!(h.app.brush_hud.is_none());
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(stamp_tips(&h), before, "one Ctrl+Z restores");
}

/// ed6 on the whole-curve path: the modifier lands a frame after the right
/// press, and still neither pan nor the context menu happens.
#[test]
fn a_late_modifier_on_the_whole_curve_hud_neither_pans_nor_opens_a_menu() {
    for mods in [egui::Modifiers::ALT, egui::Modifiers::CTRL] {
        let (mut h, id, _, away) = whole_curve_board("hud_select_whole_late");
        let before = painted_vertex_tips(&h, id);
        let cam = h.app.tab().cam.offset;
        let button =
            |pos: Pos2, pressed: bool, modifiers: egui::Modifiers| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers,
            };
        let end = away + EVec2::new(40.0, 0.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(away)));
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerMoved(away));
            i.events.push(button(away, true, egui::Modifiers::NONE));
        });
        h.frame_with(|i| {
            i.modifiers = mods;
            i.events.push(egui::Event::PointerMoved(away));
        });
        assert!(
            h.app.brush_hud.is_some(),
            "{mods:?}: the late modifier opens the HUD"
        );
        h.frame_with(|i| {
            i.modifiers = mods;
            i.events.push(egui::Event::PointerMoved(end));
        });
        h.frame_with(|i| {
            i.modifiers = mods;
            i.events.push(button(end, false, mods));
        });
        h.frame();
        assert!(h.app.brush_hud.is_none());
        assert_eq!(h.app.tab().cam.offset, cam, "{mods:?}: no pan");
        assert!(h.app.board_menu.is_none(), "{mods:?}: no context menu");
        assert!(
            h.app.board_empty_menu.is_none(),
            "{mods:?}: no context menu"
        );
        if mods.alt {
            assert!(
                painted_vertex_tips(&h, id)[1].0 > before[1].0,
                "the drag sized the curve"
            );
        }
    }
}

/// es1 under the Select tool: the style row under the size circle restyles
/// the whole selected curve, one undo step.
#[test]
fn select_tool_style_row_restyles_the_whole_selected_curve() {
    use board_tip_hud::palette_slot;
    let (mut h, id, _, away) = whole_curve_board("hud_select_whole_style");
    let depth = h.app.tab().journal.undo_depth();
    let width = curve_shape(&h, id).1.stroke.width;
    h.frame_with(alt_right(away, None));
    h.frame_with(alt_right(away, Some(true)));
    assert!(matches!(
        h.app.brush_hud,
        Some(board_color::BrushHud::Size { .. })
    ));
    let n = h.app.tip_choices().len();
    assert_eq!(n, 5, "the curve style row");
    let r = (h.app.active_tip().0 * 0.5 * h.app.tab().cam.z).max(1.5);
    let arrow = palette_slot(away, r, 2, n);
    h.frame_with(alt_right(Pos2::new(away.x, (away.y + arrow.y) * 0.5), None));
    h.frame_with(alt_right(arrow, None));
    h.frame_with(alt_right(arrow, Some(false)));
    h.frame();
    let (_, s) = curve_shape(&h, id);
    assert!(s.stroke.arrow_end, "arrow at the end");
    assert_eq!(s.stroke.width, width, "the row holds the width");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
}

/// User, 28 September 2026: "general issue using charicters in this way as
/// it launches the search fetur on the canvas". A held tool letter's
/// auto-repeat is not typing: it never opens command entry, inside the
/// hold window or after it. Two real key presses still do.
#[test]
fn a_held_tool_letter_never_opens_command_entry() {
    let mut h = grip_board("held_letter");
    let at = h.app.canvas_rect.center();
    let key = |key: egui::Key, pressed: bool, repeat: bool| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat,
        modifiers: egui::Modifiers::NONE,
    };
    let auto_repeat = |h: &mut Harness| {
        h.frame_with(|i| {
            i.events.push(key(egui::Key::A, true, true));
            i.events.push(egui::Event::Text("a".into()));
        });
    };
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(at));
        i.events.push(key(egui::Key::A, true, false));
        i.events.push(egui::Event::Text("a".into()));
    });
    for _ in 0..3 {
        auto_repeat(&mut h);
        assert!(!h.app.palette_state.open, "a repeat inside the hold window");
    }
    h.app
        .bare_letter_hold
        .as_mut()
        .expect("the A hold is still pending")
        .started_at = std::time::Instant::now() - std::time::Duration::from_secs(1);
    h.frame();
    assert_eq!(h.app.board_tool, board::BoardTool::DirectSelect, "A arms");
    for _ in 0..3 {
        auto_repeat(&mut h);
        assert!(!h.app.palette_state.open, "a repeat after the hold window");
        assert!(
            h.app.bare_letter_hold.is_none(),
            "a repeat starts no new hold"
        );
    }
    h.frame_with(|i| i.events.push(key(egui::Key::A, false, false)));
    h.frame_with(|i| {
        i.events.push(key(egui::Key::R, true, false));
        i.events.push(egui::Event::Text("r".into()));
    });
    h.frame_with(|i| {
        i.events.push(key(egui::Key::E, true, false));
        i.events.push(egui::Event::Text("e".into()));
    });
    assert!(
        h.app.palette_state.open,
        "typed letters still open command entry"
    );
    assert_eq!(h.app.palette_state.query, "re");
}
