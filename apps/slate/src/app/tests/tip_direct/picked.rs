//! Tip HUD on picked curve vertices and end conditions.

use super::*;

/// User (27 September 2026, tip29): with vertices picked, Alt, Shift and
/// Ctrl+right-drag edit only those vertices' width, opacity and color, one
/// undo step per HUD, and Esc puts the curve back.
#[test]
fn direct_select_hud_edits_only_the_picked_anchors() {
    let mut h = grip_board("hud_direct_picked");
    let (id, pts) = hud_polyline(&mut h);
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.app.direct.anchors = [1].into_iter().collect();
    h.frame();
    let xf = h.app.board_xf();
    let away = xf.w2s(pts[1] + EVec2::new(0.0, 160.0));
    let before = painted_vertex_tips(&h, id);
    let depth = h.app.tab().journal.undo_depth();

    hud_scrub(&mut h, away, EVec2::new(40.0, 0.0), egui::Modifiers::ALT);
    let tips = painted_vertex_tips(&h, id);
    assert!(
        tips[1].0 > before[1].0 + 10.0,
        "the picked vertex widens: {tips:?}"
    );
    assert_eq!(tips[0], before[0], "vertex 0 keeps its width");
    assert_eq!(tips[2], before[2], "vertex 2 keeps its width");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");

    hud_scrub(&mut h, away, EVec2::new(0.0, 60.0), egui::Modifiers::SHIFT);
    let faded = painted_vertex_tips(&h, id);
    assert!(faded[1].1[3] < 200, "the picked vertex fades: {faded:?}");
    assert_eq!(faded[0].1[3], 255);
    assert_eq!(faded[2].1[3], 255);
    assert_eq!(faded[1].0, tips[1].0, "opacity keeps the width");
    assert_eq!(h.app.doc().scene.node(id).unwrap().opacity, 1.0);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

    hud_open_wheel(&mut h, away);
    h.app.set_active_rgb([10, 200, 30]);
    hud_release_wheel(&mut h, away);
    let colored = painted_vertex_tips(&h, id);
    assert_eq!(colored[1].1[..3], [10, 200, 30]);
    assert_eq!(colored[0].1, before[0].1, "vertex 0 keeps its color");
    assert_eq!(colored[2].1, before[2].1, "vertex 2 keeps its color");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 3);

    // Esc mid-HUD restores the curve and journals nothing.
    hud_open_wheel(&mut h, away);
    h.app.set_active_rgb([200, 10, 10]);
    press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.app.brush_hud.is_none(), "Esc closes the HUD");
    hud_release_wheel(&mut h, away);
    assert_eq!(painted_vertex_tips(&h, id), colored, "Esc restores");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 3);

    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(painted_vertex_tips(&h, id), faded, "one Ctrl+Z per HUD");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert_eq!(painted_vertex_tips(&h, id), before);
}

/// Select-tool grip picks arm the same HUD for those vertices (user,
/// 27 September 2026: "for all curve types if a curve verticie is selected").
#[test]
fn select_tool_grip_picks_take_the_tip_hud() {
    let mut h = grip_board("hud_select_picked");
    let (id, pts) = hud_polyline(&mut h);
    let xf = h.app.board_xf();
    press_primary(&mut h, xf.w2s(pts[2]), egui::Modifiers::NONE);
    assert_eq!(
        h.app.picked_vertices(),
        Some((id, vec![2])),
        "the click picks vertex 2"
    );
    let before = painted_vertex_tips(&h, id);
    let depth = h.app.tab().journal.undo_depth();
    let away = xf.w2s(pts[1] + EVec2::new(0.0, 160.0));
    hud_scrub(&mut h, away, EVec2::new(40.0, 0.0), egui::Modifiers::ALT);
    let tips = painted_vertex_tips(&h, id);
    assert!(tips[2].0 > before[2].0 + 10.0, "{tips:?}");
    assert_eq!(tips[0], before[0]);
    assert_eq!(tips[1], before[1]);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert!(h.app.board_sel.contains(&id), "the curve stays selected");
}

/// Hovering a vertex arms the HUD for that vertex alone; away from the
/// vertices Direct Select edits the whole curve (user, 27 September 2026:
/// "hover to re enter editing mode to spot edti radious color etcetra").
#[test]
fn hovering_a_vertex_arms_the_hud_for_that_vertex() {
    let mut h = grip_board("hud_hover_vertex");
    let (id, pts) = hud_polyline(&mut h);
    let xf = h.app.board_xf();
    let before = painted_vertex_tips(&h, id);

    // Select tool, nothing picked: hover vertex 0.
    hud_scrub(
        &mut h,
        xf.w2s(pts[0]),
        EVec2::new(40.0, 0.0),
        egui::Modifiers::ALT,
    );
    let tips = painted_vertex_tips(&h, id);
    assert!(tips[0].0 > before[0].0 + 10.0, "{tips:?}");
    assert_eq!(tips[1], before[1]);
    assert_eq!(tips[2], before[2]);

    // Direct Select, nothing picked: hover vertex 2.
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.frame();
    hud_scrub(
        &mut h,
        xf.w2s(pts[2]),
        EVec2::new(40.0, 0.0),
        egui::Modifiers::ALT,
    );
    let hovered = painted_vertex_tips(&h, id);
    assert!(hovered[2].0 > tips[2].0 + 10.0, "{hovered:?}");
    assert_eq!(hovered[1], tips[1], "vertex 1 untouched");

    // Direct Select away from every vertex: the whole curve.
    let away = xf.w2s(pts[1] + EVec2::new(0.0, 160.0));
    hud_scrub(&mut h, away, EVec2::new(-20.0, 0.0), egui::Modifiers::ALT);
    let whole = painted_vertex_tips(&h, id);
    for k in 0..3 {
        assert!(whole[k].0 < hovered[k].0, "vertex {k} narrows: {whole:?}");
    }
}

/// A whole-curve color change reaches a curve that carries per-vertex
/// colors (they paint over the stroke color). User, 28 September 2026
/// (ed1): the change shifts every vertex from its own color, so the
/// gradient stays a gradient; the first vertex, the color the wheel opened
/// on, lands on the pick.
#[test]
fn a_whole_curve_hud_color_recolors_vertex_tips() {
    let mut h = grip_board("hud_whole_color");
    let (id, pts) = hud_polyline(&mut h);
    style_vertices(
        &mut h,
        id,
        &[4.0, 8.0, 4.0],
        &[0, 120, 240],
        &[None, None, None],
    );
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.frame();
    let before = painted_vertex_tips(&h, id);
    let away = h.app.board_xf().w2s(pts[1] + EVec2::new(0.0, 160.0));
    hud_open_wheel(&mut h, away);
    h.app.set_active_rgb([10, 200, 30]);
    hud_release_wheel(&mut h, away);
    let tips = painted_vertex_tips(&h, id);
    let rgb = |c: [u8; 4]| [c[0], c[1], c[2]];
    for (k, (w, c)) in tips.iter().enumerate() {
        let want = board_color::shift_hsv(rgb(before[k].1), rgb(before[0].1), [10, 200, 30]);
        assert_eq!(rgb(*c), want, "vertex {k} shifts from its own color");
        assert_eq!(*w, [4.0, 8.0, 4.0][k], "vertex {k} keeps its width");
    }
    assert_eq!(rgb(tips[0].1), [10, 200, 30]);
    assert_ne!(tips[1].1, tips[0].1, "still a gradient");
    assert_ne!(tips[2].1, tips[1].1, "still a gradient");
}

/// Per-vertex opacity is alpha in the vertex colors, which the HTML export
/// writes as gradient stop opacity (Art. IV: both interpreters).
#[test]
fn per_vertex_opacity_exports_as_gradient_stop_opacity() {
    let mut h = grip_board("hud_vertex_alpha_export");
    let (id, pts) = hud_polyline(&mut h);
    h.app.set_board_tool(board::BoardTool::DirectSelect);
    h.app.direct_set_target(Some(id));
    h.app.direct.anchors = [2].into_iter().collect();
    h.frame();
    let away = h.app.board_xf().w2s(pts[1] + EVec2::new(0.0, 160.0));
    hud_scrub(&mut h, away, EVec2::new(0.0, 60.0), egui::Modifiers::SHIFT);
    let alpha = painted_vertex_tips(&h, id)[2].1[3];
    assert!(alpha < 200, "vertex 2 fades: {alpha}");
    let html = slate_artifact::render_html(h.app.doc(), &slate_artifact::AssetMap::default());
    let want = format!("stop-opacity=\"{:.3}\"", alpha as f32 / 255.0);
    assert!(html.contains(&want), "the export fades vertex 2 ({want})");
}

/// User (28 September 2026, tl5: "should be able to spesify end condition
/// on a per vertex bass"): with an end grip picked, the style row sets that
/// end only. An arrow at the picked start leaves the far end plain; a round
/// cap at the picked end keeps the start's arrow; each is one undo step.
#[test]
fn a_picked_end_grip_takes_its_own_end_condition() {
    let mut h = grip_board("end_condition_per_end");
    let (id, pts) = end_condition_line(&mut h);
    let xf = h.app.board_xf();
    let away = xf.w2s(pts[0].lerp(pts[1], 0.5) + EVec2::new(0.0, 160.0));
    let half = 4.0;
    let plain = end_reach(&h, id, pts);
    assert!(plain.side[0] < half + 1.5 && plain.side[1] < half + 1.5);
    let depth = h.app.tab().journal.undo_depth();

    press_primary(&mut h, xf.w2s(pts[0]), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0])), "start picked");
    hud_row_release(&mut h, away, 2);
    let arrow = end_reach(&h, id, pts);
    assert!(
        arrow.side[0] > 2.0 * half,
        "a head at the start: {:?}",
        arrow.side
    );
    assert!(
        arrow.side[1] < half + 1.5,
        "the end stays plain: {:?}",
        arrow.side
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");

    press_primary(&mut h, xf.w2s(pts[1]), egui::Modifiers::NONE);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![1])), "end picked");
    hud_row_release(&mut h, away, 1);
    let round = end_reach(&h, id, pts);
    assert!(
        round.side[0] > 2.0 * half,
        "the start keeps its head: {:?}",
        round.side
    );
    assert!(
        round.past[1] > half * 0.8,
        "a round cap past the end: {:?}",
        round.past
    );
    assert!(round.side[1] < half + 1.5, "{:?}", round.side);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 2);

    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    let undone = end_reach(&h, id, pts);
    assert!(
        undone.past[1] < 1.5,
        "undo flattens the end: {:?}",
        undone.past
    );
    assert!(undone.side[0] > 2.0 * half, "and keeps the start's head");
    press_key_with(&mut h, egui::Key::Z, egui::Modifiers::CTRL);
    assert!(
        end_reach(&h, id, pts).side[0] < half + 1.5,
        "undo drops the head"
    );
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    press_key_with(&mut h, egui::Key::Y, egui::Modifiers::CTRL);
    let redone = end_reach(&h, id, pts);
    assert!(redone.side[0] > 2.0 * half && redone.past[1] > half * 0.8);
}

/// Esc before the release puts a picked end's condition back and journals
/// nothing.
#[test]
fn escape_restores_a_picked_end_condition() {
    use board_tip_hud::{palette_band_y, palette_slot};
    let mut h = grip_board("end_condition_escape");
    let (id, pts) = end_condition_line(&mut h);
    let xf = h.app.board_xf();
    let away = xf.w2s(pts[0].lerp(pts[1], 0.5) + EVec2::new(0.0, 160.0));
    press_primary(&mut h, xf.w2s(pts[0]), egui::Modifiers::NONE);
    let before = h.app.doc().scene.node(id).cloned();
    let depth = h.app.tab().journal.undo_depth();
    h.frame_with(alt_right(away, None));
    h.frame_with(alt_right(away, Some(true)));
    let r = (h.app.active_tip().0 * 0.5 * h.app.tab().cam.z).max(1.5);
    let n = h.app.tip_choices().len();
    let slot = palette_slot(away, r, 2, n);
    h.frame_with(alt_right(
        Pos2::new(away.x, palette_band_y(away, r) + 1.0),
        None,
    ));
    h.frame_with(alt_right(slot, None));
    assert!(
        end_reach(&h, id, pts).side[0] > 8.0,
        "the hovered choice previews"
    );
    h.frame_with(|i| {
        i.modifiers = egui::Modifiers::ALT;
        i.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::ALT,
        });
    });
    h.frame_with(alt_right(slot, Some(false)));
    h.frame();
    assert_eq!(h.app.doc().scene.node(id).cloned(), before, "Esc restores");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);
}

/// One end picked, the row reads per end ("Arrow here") and has no "Narrow
/// at both ends"; both ends picked, it sets both. The conditions survive
/// save and reload.
#[test]
fn both_picked_ends_take_the_condition_and_survive_save() {
    use board_tip_hud::choice_label;
    use slate_doc::scene::{StrokeCap, WidthProfile};
    let mut h = grip_board("end_condition_both");
    let (id, pts) = end_condition_line(&mut h);
    let xf = h.app.board_xf();
    let away = xf.w2s(pts[0].lerp(pts[1], 0.5) + EVec2::new(0.0, 160.0));

    press_primary(&mut h, xf.w2s(pts[0]), egui::Modifiers::NONE);
    let labels: Vec<&str> = h
        .app
        .tip_choices()
        .iter()
        .map(|c| choice_label(*c, false))
        .collect();
    assert!(labels.contains(&"Arrow here"), "{labels:?}");
    assert!(!labels.contains(&"Narrow at both ends"), "{labels:?}");

    press_primary(&mut h, xf.w2s(pts[1]), egui::Modifiers::SHIFT);
    assert_eq!(h.app.picked_vertices(), Some((id, vec![0, 1])));
    hud_row_release(&mut h, away, 3);
    let s = stroke_of(&h, id);
    assert!(
        matches!(s.profile, WidthProfile::Ends { .. }),
        "{:?}",
        s.profile
    );
    assert_eq!(s.end_caps(), [StrokeCap::Round; 2]);
    hud_row_release(&mut h, away, 2);
    let s = stroke_of(&h, id);
    assert_eq!(s.arrows(), [true, true]);
    assert_eq!(s.profile, WidthProfile::Uniform);

    press_primary(&mut h, xf.w2s(pts[1]), egui::Modifiers::NONE);
    hud_row_release(&mut h, away, 1);
    let s = stroke_of(&h, id);
    assert_eq!(s.arrows(), [true, false], "the start keeps its arrow");
    assert_eq!(s.end_caps(), [StrokeCap::Butt, StrokeCap::Round]);

    let path = h.base.join("end-conditions.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());
    let mut h2 = Harness::new("end_condition_both_reload");
    h2.app.open_doc_at(path);
    let reloaded = h2
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find_map(|n| match &n.kind {
            NodeKind::Shape(r) if n.id == id => Some(r.stroke),
            _ => None,
        })
        .expect("the line reloads");
    assert_eq!(reloaded, s);
}

/// A trimmed piece keeps the condition of the source end it still owns;
/// the new cut end is plain. Split does the same for both pieces.
#[test]
fn trimming_keeps_the_far_ends_arrow() {
    use slate_doc::scene::{StrokeCap, StrokeEnd};
    let mut h = trim_board("end_condition_trim");
    let horiz = add_seg(&mut h.app, Pos2::new(0.0, 50.0), Pos2::new(100.0, 50.0));
    let vert = add_seg(&mut h.app, Pos2::new(50.0, 0.0), Pos2::new(50.0, 100.0));
    if let Some(NodeKind::Shape(s)) = h.app.doc_mut().scene.node_mut(horiz).map(|n| &mut n.kind) {
        s.stroke.cap = StrokeCap::Round;
        s.stroke.set_end(
            0,
            StrokeEnd {
                cap: StrokeCap::Round,
                arrow: false,
                narrow: true,
            },
        );
        s.stroke.set_end(
            1,
            StrokeEnd {
                cap: StrokeCap::Butt,
                arrow: true,
                narrow: false,
            },
        );
    }
    select_trim(&mut h.app, &[horiz, vert]);
    h.app.set_board_tool(board::BoardTool::Trim);
    assert!(h.app.trim_click(Pos2::new(25.0, 50.0), false));
    let s = stroke_of(&h, horiz);
    assert_eq!(s.arrows(), [false, true], "the far end keeps its arrow");
    assert_eq!(s.end_caps(), [StrokeCap::Round, StrokeCap::Butt]);
    assert_eq!(
        s.profile.narrow_ends(),
        [false, false],
        "the cut end is plain"
    );
    h.app.board_undo();

    select_trim(&mut h.app, &[horiz, vert]);
    h.app.set_board_tool(board::BoardTool::Split);
    assert!(h.app.trim_click(Pos2::new(25.0, 50.0), false));
    let pieces: Vec<_> = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| n.id != vert)
        .map(|n| (n.rect.x, stroke_of(&h, n.id)))
        .collect();
    assert_eq!(pieces.len(), 2);
    for (x, s) in pieces {
        if x < 25.0 {
            assert_eq!(s.arrows(), [false, false]);
            assert_eq!(s.profile.narrow_ends(), [true, false], "left piece");
            assert_eq!(s.end_caps(), [StrokeCap::Round; 2]);
        } else {
            assert_eq!(s.arrows(), [false, true]);
            assert_eq!(s.profile.narrow_ends(), [false, false], "right piece");
            assert_eq!(s.end_caps(), [StrokeCap::Round, StrokeCap::Butt]);
        }
    }
}

/// Ctrl+J keeps the conditions of the free ends and drops them at the
/// joined ends; closing an open curve drops both.
#[test]
fn join_keeps_free_end_conditions_and_drops_joined_ones() {
    use slate_doc::scene::{StrokeCap, StrokeEnd};
    let mut h = trim_board("end_condition_join");
    let a = add_stroke(&mut h.app, 0.0, 0.0);
    let b = add_stroke(&mut h.app, 150.0, 0.0);
    let arrow = StrokeEnd {
        cap: StrokeCap::Round,
        arrow: true,
        narrow: false,
    };
    let flat = StrokeEnd {
        cap: StrokeCap::Butt,
        arrow: false,
        narrow: false,
    };
    for (id, ends) in [(a, [arrow, arrow]), (b, [arrow, flat])] {
        if let Some(NodeKind::Shape(s)) = h.app.doc_mut().scene.node_mut(id).map(|n| &mut n.kind) {
            s.stroke.set_end(0, ends[0]);
            s.stroke.set_end(1, ends[1]);
        }
    }
    h.app.board_sel = [a, b].into_iter().collect();
    assert!(h.app.cmd_join());
    let joined = h.app.doc().scene.nodes[0].id;
    let s = stroke_of(&h, joined);
    assert_eq!(s.arrows(), [true, false]);
    assert_eq!(s.end_caps(), [StrokeCap::Round, StrokeCap::Butt]);
    h.app.board_undo();

    h.app.board_sel = [a].into_iter().collect();
    assert!(h.app.cmd_join());
    let s = stroke_of(&h, a);
    assert_eq!(s.arrows(), [false, false], "a closed curve has no ends");
    assert_eq!((s.cap_start, s.cap_end), (None, None));
}
