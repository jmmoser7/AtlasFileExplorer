//! Shift steps and tab-lock on vector drafts.

use super::*;

#[test]
fn a_shift_line_tweens_the_tip_from_the_start_click() {
    let mut h = Harness::new("brush_tween");
    h.app.brush_width = 4.0;
    let start = h.app.tip_now();
    h.app.brush_width = 20.0;
    h.app
        .commit_tween_line(Pos2::new(0.0, 0.0), Pos2::new(80.0, 0.0), start, None);
    let node = h.app.doc().scene.nodes.last().unwrap();
    let slate_doc::scene::NodeKind::Shape(shape) = &node.kind else {
        panic!("line");
    };
    assert!((shape.stroke.width - 20.0).abs() < 1e-3);
    let tips = &shape.path.as_ref().unwrap().tips;
    assert_eq!(tips.len(), 2);
    assert!((tips[0].width - 4.0).abs() < 1e-3);
    assert!((tips[1].width - 20.0).abs() < 1e-3);
}

/// Shift places each next vertex on a 45° step from the last one.
#[test]
fn shift_places_the_next_vertex_on_a_45_degree_step() {
    for tool in VERTEX_TOOLS {
        let mut h = draft_board("shift_45_vertex", tool);
        let a = Pos2::new(0.0, 0.0);
        click_at(&mut h, a, egui::Modifiers::NONE);
        let raw = Pos2::new(100.0, 37.0);
        hover_at(&mut h, raw, egui::Modifiers::SHIFT);
        click_at(&mut h, raw, egui::Modifiers::SHIFT);
        let v = last_vertex(&h).expect("a draft");
        assert!(on_45(a, v), "{tool:?}: {v:?}");
        assert!(
            !near_px(v, raw),
            "{tool:?}: the raw pointer was constrained"
        );
    }
    // Line: its second point, the committed end.
    let mut h = draft_board("shift_45_line", board::BoardTool::Line);
    click_at(&mut h, Pos2::ZERO, egui::Modifiers::NONE);
    hover_at(&mut h, Pos2::new(100.0, 37.0), egui::Modifiers::SHIFT);
    click_at(&mut h, Pos2::new(100.0, 37.0), egui::Modifiers::SHIFT);
    let (a, b) = board_line::line_endpoints(&h.app.doc().scene.nodes[0]).unwrap();
    assert!(on_45(a, b), "{b:?}");
}

/// Tab locks the direction toward the pointer; an off-axis move only
/// changes length; placing the point ends the lock (P2.RhinoDraft.tab).
#[test]
fn tab_locks_the_segment_direction_on_every_vector_draft_tool() {
    let dir = EVec2::new(0.6, 0.8);
    for tool in VERTEX_TOOLS.into_iter().chain([board::BoardTool::Line]) {
        let mut h = draft_board("tab_lock_vertex", tool);
        click_at(&mut h, Pos2::ZERO, egui::Modifiers::NONE);
        hover_at(&mut h, Pos2::new(30.0, 40.0), egui::Modifiers::NONE);
        press_key_with(&mut h, egui::Key::Tab, egui::Modifiers::NONE);
        let lock = h
            .app
            .draft_lock
            .unwrap_or_else(|| panic!("{tool:?}: Tab locks"));
        assert!((lock - dir).length() < 1.0e-3, "{tool:?}: {lock:?}");
        let off = Pos2::new(500.0, -20.0);
        hover_at(&mut h, off, egui::Modifiers::NONE);
        assert!(
            h.app.draft_lock.is_some(),
            "{tool:?}: the lock holds while hovering"
        );
        click_at(&mut h, off, egui::Modifiers::NONE);
        let v = if tool == board::BoardTool::Line {
            board_line::line_endpoints(&h.app.doc().scene.nodes[0])
                .unwrap()
                .1
        } else {
            last_vertex(&h).expect("a draft")
        };
        assert!(
            on_ray(Pos2::ZERO, dir, v),
            "{tool:?}: {v:?} stays on the locked ray"
        );
        assert!(
            h.app.draft_lock.is_none(),
            "{tool:?}: placing the point ends the lock"
        );
    }
}

/// Tab again releases the lock; Esc clears it with the draft.
#[test]
fn tab_again_or_escape_releases_the_segment_lock() {
    for tool in VERTEX_TOOLS.into_iter().chain([board::BoardTool::Line]) {
        let mut h = draft_board("tab_lock_release", tool);
        click_at(&mut h, Pos2::ZERO, egui::Modifiers::NONE);
        hover_at(&mut h, Pos2::new(30.0, 40.0), egui::Modifiers::NONE);
        press_key_with(&mut h, egui::Key::Tab, egui::Modifiers::NONE);
        assert!(h.app.draft_lock.is_some(), "{tool:?}");
        assert!(
            !h.ctx.wants_keyboard_input(),
            "{tool:?}: the board kept the Tab"
        );
        press_key_with(&mut h, egui::Key::Tab, egui::Modifiers::NONE);
        assert!(h.app.draft_lock.is_none(), "{tool:?}: Tab again releases");
        let off = Pos2::new(500.0, -20.0);
        hover_at(&mut h, off, egui::Modifiers::NONE);
        if tool != board::BoardTool::Line {
            click_at(&mut h, off, egui::Modifiers::NONE);
            assert!(
                near_px(last_vertex(&h).unwrap(), off),
                "{tool:?}: free again"
            );
        }
        hover_at(&mut h, Pos2::new(600.0, 100.0), egui::Modifiers::NONE);
        press_key_with(&mut h, egui::Key::Tab, egui::Modifiers::NONE);
        assert!(h.app.draft_lock.is_some(), "{tool:?}");
        press_key_with(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
        assert!(h.app.draft_lock.is_none(), "{tool:?}: Esc clears the lock");
    }
}

/// r7-8 (user, 28 September 2026): "A Shift segment uses the currently
/// armed texture, stored per tip." A Graphite stroke, then Watercolor
/// picked in the style row and a Shift drag: the segment paints exactly as
/// an all-Watercolor stroke would there, and the freehand body exactly as
/// an all-Graphite one.
#[test]
fn a_shift_segment_paints_the_texture_armed_for_it() {
    use slate_doc::scene::BrushTexture;
    let mut h = brush_board("r7_8_segment_texture");
    h.app.board_colors.fg.0 = [40, 60, 200, 255];
    h.app.brush_opacity = 0.8;
    h.app.brush_softness = 0.2;
    h.app.brush_width = 24.0;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let p = |x: f32, y: f32| c + EVec2::new(x, y);
    let hud = h.app.board_xf().w2s(p(0.0, 150.0));
    pick_style_row(
        &mut h,
        hud,
        board_tip_hud::TipChoice::Texture(BrushTexture::Graphite),
    );
    press_drag_release_frames(
        &mut h,
        &dense(&[p(-300.0, -100.0), p(-100.0, -100.0)]),
        egui::Modifiers::NONE,
        |_| {},
    );
    let id = h.app.doc().scene.nodes.last().expect("the stroke").id;
    pick_style_row(
        &mut h,
        hud,
        board_tip_hud::TipChoice::Texture(BrushTexture::Watercolor),
    );
    press_drag_release_frames(
        &mut h,
        &[
            p(-60.0, 60.0),
            p(0.0, 20.0),
            p(60.0, -20.0),
            p(100.0, -98.0),
        ],
        egui::Modifiers::SHIFT,
        |_| {},
    );
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        1,
        "the segment extends the stroke"
    );
    let node = h.app.doc().scene.node(id).unwrap().clone();
    let NodeKind::Shape(s) = &node.kind else {
        panic!("a shape")
    };
    let tips = &s.path.as_ref().unwrap().tips;
    assert_eq!(
        tips.last().map(|t| t.texture),
        Some(BrushTexture::Watercolor)
    );
    assert_eq!(tips[tips.len() - 2].texture, BrushTexture::Graphite);
    let mixed = stamp_at_one(&node);
    let wet = stamp_at_one(&with_texture(&node, BrushTexture::Watercolor));
    let dry = stamp_at_one(&with_texture(&node, BrushTexture::Graphite));
    for w in around(p(20.0, -100.0), 6) {
        assert_eq!(
            stamp_px(&mixed, w),
            stamp_px(&wet, w),
            "the segment at {w:?}"
        );
    }
    for w in around(p(-220.0, -100.0), 6) {
        assert_eq!(stamp_px(&mixed, w), stamp_px(&dry, w), "the body at {w:?}");
    }
    assert!(
        around(p(20.0, -100.0), 6)
            .iter()
            .any(|w| stamp_px(&mixed, *w) != stamp_px(&dry, *w)),
        "the segment paints Graphite"
    );
}
