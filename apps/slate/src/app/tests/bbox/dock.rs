//! Dock-strip arming, resize, and drag.

use super::*;

/// A click on a dropped-toolbar icon arms the command and does not place.
#[test]
fn dock_strip_click_arms_without_placing() {
    let mut h = web_board("dock_arm");
    let id = add_dock_strip(&mut h.app, "tool.text", &["text.block"]);
    h.frame();
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let world = h
        .app
        .dock_embed_first_tool_world(&h.ctx, &xf, &n)
        .expect("dropped toolbar must expose a tool slot");
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.try_dock_embed_click(&h.ctx, world));
    assert_eq!(h.app.board_tool, board::BoardTool::Text);
    assert_eq!(
        h.app.doc().scene.nodes.len(),
        before,
        "arming from a dropped toolbar must not place"
    );
}

/// Widening a dropped toolbar must not shrink icons; uniform grow enlarges them.
#[test]
fn dock_strip_resize_contains_without_shrinking_icons() {
    let mut h = web_board("dock_scale");
    let id = add_dock_strip(&mut h.app, "tool.shapes", &["shape.rect", "shape.ellipse"]);
    h.frame();
    h.app.tab_mut().cam.z = 1.0;
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let slate_doc::scene::NodeKind::DockStrip(strip) = &n.kind else {
        panic!("expected a dock strip");
    };
    let items = super::super::super::ui::tools::palette_strip_items(
        &h.app,
        &strip.palette_id,
        &strip.visible,
    );
    let mut tokens = atlas_shell::tokens::current().dock.clone();
    tokens.normalize();
    let layout = atlas_shell::dock::measure_icon_strip(
        &h.ctx,
        &items,
        &tokens,
        atlas_shell::dock::DockSide::BottomCenter,
        16_384.0,
    );
    let slot = layout.first_slot_id().expect("strip has a slot");
    let dest = xf.rect_w2s(n.rect);
    let a = atlas_shell::dock::icon_strip_slot_rect(dest, &layout, &tokens, slot)
        .expect("slot at dest");
    let wide =
        ERect::from_center_size(dest.center(), EVec2::new(dest.width() * 2.5, dest.height()));
    let b = atlas_shell::dock::icon_strip_slot_rect(wide, &layout, &tokens, slot)
        .expect("slot at wide dest");
    assert!(
        (b.height() - a.height()).abs() < 0.5,
        "widening the node must not shrink icons ({} vs {})",
        b.height(),
        a.height()
    );
    let both = ERect::from_center_size(dest.center(), dest.size() * 2.0);
    let c = atlas_shell::dock::icon_strip_slot_rect(both, &layout, &tokens, slot)
        .expect("slot at uniform dest");
    assert!(
        c.height() > a.height() * 1.8,
        "uniform grow must enlarge icons ({} vs {})",
        c.height(),
        a.height()
    );
}

/// Press-drag on a dropped toolbar moves it even when a create tool is armed.
#[test]
fn dock_strip_drag_moves_regardless_of_armed_tool() {
    let mut h = web_board("dock_move");
    let id = add_dock_strip(&mut h.app, "tool.shapes", &["shape.rect"]);
    h.app.set_board_tool(board::BoardTool::RectShape);
    h.frame();
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap();
    let world = Pos2::new(n.rect.x + n.rect.w * 0.5, n.rect.y + n.rect.h * 0.5);
    let screen = xf.w2s(world);
    let drag = h
        .app
        .begin_gesture_for_test(screen, world, egui::Modifiers::NONE);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "press-drag on a dropped toolbar must move it, not draw"
    );
}
