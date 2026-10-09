//! Tip HUD scrubs, the color wheel, and end conditions.

use super::*;

/// Open the color wheel at `at` through frames; returns its center.
pub(crate) fn open_wheel_frames(h: &mut Harness, at: Pos2) -> Pos2 {
    h.frame_with(ctrl_right(at, None));
    h.frame_with(ctrl_right(at, Some(true)));
    match h.app.brush_hud {
        Some(board_color::BrushHud::Wheel { center, .. }) => center,
        ref other => panic!("Ctrl+right opens the wheel, got {other:?}"),
    }
}

/// A point on the wheel's disk well away from its white, black and gray
/// edges: a saturated, bright color.
pub(crate) fn bright_on_disk(center: Pos2) -> Pos2 {
    center + EVec2::new(50.0, -50.0)
}

// ---------- whole-curve tip HUD under the Select tool (ed1–ed3, eb1) ----------

/// One tip per vertex of curve `id`, with these widths and straight colors.
pub(crate) fn tip_vertices(h: &mut Harness, id: NodeId, widths: &[f32], colors: &[[u8; 4]]) {
    let n = h.app.doc_mut().scene.node_mut(id).unwrap();
    let NodeKind::Shape(s) = &mut n.kind else {
        panic!("a shape")
    };
    let mut path = s.path.as_deref().cloned().unwrap();
    assert_eq!(widths.len(), path.segs.len() + 1, "one tip per vertex");
    path.tips = widths
        .iter()
        .zip(colors)
        .map(|(&width, &c)| slate_doc::scene::StrokeSpan {
            width,
            softness: 0.0,
            color: Rgba(c),
            texture: Default::default(),
        })
        .collect();
    s.stroke.width = widths.iter().copied().fold(0.0, f32::max);
    s.path = Some(path.into());
    h.frame();
}

/// A selected polyline with a taper, under the Select tool, nothing picked,
/// and a screen point well away from its grips.
pub(crate) fn whole_curve_board(tag: &str) -> (Harness, NodeId, [Pos2; 3], Pos2) {
    let mut h = grip_board(tag);
    let (id, pts) = hud_polyline(&mut h);
    tip_vertices(
        &mut h,
        id,
        &[4.0, 8.0, 4.0],
        &[[200, 40, 40, 255], [40, 200, 40, 255], [40, 40, 200, 255]],
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Select);
    assert_eq!(h.app.board_sel.len(), 1);
    assert_eq!(h.app.picked_vertices(), None, "nothing picked");
    let away = h.app.board_xf().w2s(pts[1] + EVec2::new(0.0, 160.0));
    (h, id, pts, away)
}

// ---------- tip HUD on picked curve vertices (direct-selection.md) ----------

/// A right-button HUD chord through real frames: hover `at`, press there,
/// move by `delta`, release.
pub(crate) fn hud_scrub(h: &mut Harness, at: Pos2, delta: EVec2, mods: egui::Modifiers) {
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: mods,
    };
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerMoved(at));
    });
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerMoved(at));
        i.events.push(button(at, true));
    });
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerMoved(at + delta));
    });
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(button(at + delta, false));
    });
    h.frame();
}

/// Open the Ctrl+right color wheel at `at` through real frames and leave it
/// open (the right button stays down).
pub(crate) fn hud_open_wheel(h: &mut Harness, at: Pos2) {
    h.frame_with(|i| {
        i.modifiers = egui::Modifiers::CTRL;
        i.events.push(egui::Event::PointerMoved(at));
    });
    h.frame_with(|i| {
        i.modifiers = egui::Modifiers::CTRL;
        i.events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::CTRL,
        });
    });
    assert!(
        matches!(h.app.brush_hud, Some(board_color::BrushHud::Wheel { .. })),
        "Ctrl+right opens the wheel"
    );
}

/// Releases after a hold past the quick-click window: the callers set the
/// color directly instead of travelling, and a travel-free quick release
/// opens numeric entry instead (user, 28 September 2026).
pub(crate) fn hud_release_wheel(h: &mut Harness, at: Pos2) {
    let t = h.ctx.input(|i| i.time) + board_tip_hud::QUICK_CLICK_SECONDS + 0.2;
    h.frame_with(|i| {
        i.time = Some(t);
        i.modifiers = egui::Modifiers::CTRL;
        i.events.push(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::CTRL,
        });
    });
    h.frame();
}

/// A three-vertex polyline across the middle of the canvas, selected.
pub(crate) fn hud_polyline(h: &mut Harness) -> (NodeId, [Pos2; 3]) {
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let pts = [c + EVec2::new(-150.0, 0.0), c, c + EVec2::new(150.0, 0.0)];
    let id = commit_polyline(h, &pts, false);
    (id, pts)
}

pub(crate) fn press_primary(h: &mut Harness, screen: Pos2, mods: egui::Modifiers) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(screen)));
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerButton {
            pos: screen,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: mods,
        });
    });
    h.frame_with(|i| {
        i.modifiers = mods;
        i.events.push(egui::Event::PointerButton {
            pos: screen,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: mods,
        });
    });
    h.frame();
}

/// A selected horizontal two-point Line across the canvas middle, 8 wide
/// with flat ends, and its world end points.
pub(crate) fn end_condition_line(h: &mut Harness) -> (NodeId, [Pos2; 2]) {
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.frame();
    let c = h.app.board_xf().s2w(h.app.canvas_rect.center());
    let pts = [c + EVec2::new(-150.0, 0.0), c + EVec2::new(150.0, 0.0)];
    let (r, d) = board_path::points_to_path_data(&pts, false);
    h.app
        .commit_path_node(slate_doc::StrokeTool::Line, r, d, false);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    if let Some(NodeKind::Shape(s)) = h.app.doc_mut().scene.node_mut(id).map(|n| &mut n.kind) {
        s.stroke.width = 8.0;
        s.stroke.cap = slate_doc::scene::StrokeCap::Butt;
    }
    h.frame();
    (id, pts)
}

/// Alt+right-drag at `at` through frames, down into the style row, and
/// release on slot `slot`.
pub(crate) fn hud_row_release(h: &mut Harness, at: Pos2, slot: usize) {
    use board_tip_hud::{palette_band_y, palette_slot};
    h.frame_with(alt_right(at, None));
    h.frame_with(alt_right(at, Some(true)));
    assert!(
        matches!(h.app.brush_hud, Some(board_color::BrushHud::Size { .. })),
        "Alt+right opens the size HUD"
    );
    let r = (h.app.active_tip().0 * 0.5 * h.app.tab().cam.z).max(1.5);
    let n = h.app.tip_choices().len();
    let target = palette_slot(at, r, slot, n);
    h.frame_with(alt_right(
        Pos2::new(at.x, palette_band_y(at, r) + 1.0),
        None,
    ));
    h.frame_with(alt_right(target, None));
    h.frame_with(alt_right(target, Some(false)));
    h.frame();
}
