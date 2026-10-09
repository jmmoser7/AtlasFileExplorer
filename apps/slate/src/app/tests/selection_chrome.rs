//! Mirror flyout, eraser preview color, and the rotate cursor.

use super::*;

/// The Actions flyout offers both mirrors, and they run the commands.
#[test]
fn actions_flyout_offers_mirror() {
    let mut h = web_board("actions_mirror");
    let pic = add_picture(
        &mut h,
        slate_doc::scene::WorldRect::new(0.0, 0.0, 200.0, 100.0),
    );
    h.app.board_sel = std::iter::once(pic).collect();
    let items = ui::tools::palette_strip_items(&h.app, "tool.actions", &[]);
    let ids: Vec<_> = items.iter().map(|i| i.id).collect();
    assert!(ids.contains(&"action.mirror_h"), "{ids:?}");
    assert!(ids.contains(&"action.mirror_v"), "{ids:?}");
    for id in ["board.mirror.horizontal", "board.mirror.vertical"] {
        assert!(h
            .app
            .registry
            .by_id(atlas_commands::CommandId(id))
            .is_some());
    }
    ui::tools::activate_flyout_id(&mut h.app, &h.ctx, "action.mirror_v");
    assert_eq!(
        h.app.cmd_history.iter().last().unwrap().id.0,
        "board.mirror.vertical"
    );
    assert_eq!(picture_flips(&h, pic), (false, true));
}

/// Stated 2026-09-26: the eraser preview never takes the brush color. Its
/// tip, its size HUD, and a drag across ink all paint in the eraser's own
/// neutral, whatever the foreground is.
#[test]
fn the_eraser_preview_keeps_its_own_color() {
    let mut h = line_board("eraser_preview_color");
    h.app.board_colors.fg = Rgba([230, 20, 20, 255]);
    h.app.set_board_tool(board::BoardTool::Brush);
    h.frame();
    let c = h.app.canvas_rect.center();
    let brush = h.frame_output(pointer_to(c, false));
    assert!(
        painted_colors(&brush).into_iter().any(brush_red),
        "fixture: the brush tip paints in the foreground"
    );

    h.app.set_board_tool(board::BoardTool::Eraser);
    h.frame_with(pointer_to(c, false));
    let hover = h.frame_output(pointer_to(c + EVec2::new(3.0, 0.0), false));
    assert!(
        !painted_colors(&hover).into_iter().any(brush_red),
        "the eraser tip took the brush color"
    );
    let expected = h.app.eraser_preview_color();
    let (_, _, ink) = h.app.width_cursor_disc().expect("the eraser shows a disc");
    assert_eq!(ink, expected, "the eraser tip is the eraser preview color");

    h.frame_with(right_button(c, true, true));
    let hud = h.frame_output(|input| {
        input.modifiers.alt = true;
        input
            .events
            .push(egui::Event::PointerMoved(c + EVec2::new(30.0, -10.0)));
    });
    assert!(
        matches!(h.app.brush_hud, Some(board_color::BrushHud::Size { .. })),
        "Alt+right-drag opened the size HUD"
    );
    assert!(
        !painted_colors(&hud).into_iter().any(brush_red),
        "the eraser size HUD took the brush color"
    );
    h.frame_with(right_button(c + EVec2::new(30.0, -10.0), false, false));

    // A drag across blue ink, on the board and on an image's paint layer.
    let erase_across = |h: &mut Harness, what: &str| {
        let c = h.app.canvas_rect.center();
        h.frame_with(primary_button(c - EVec2::new(60.0, 0.0), true, false));
        for i in 0..12 {
            let at = c + EVec2::new(-60.0 + i as f32 * 10.0, (i % 3) as f32);
            let out = h.frame_output(pointer_to(at, false));
            assert!(
                !painted_colors(&out).into_iter().any(brush_red),
                "{what}: frame {i} of the erase drag painted the brush color"
            );
        }
        h.frame_with(primary_button(c + EVec2::new(60.0, 0.0), false, false));
    };
    let world = |h: &Harness, dx: f32| {
        h.app
            .board_xf()
            .s2w(h.app.canvas_rect.center() + EVec2::new(dx, 0.0))
    };
    h.app.board_colors.fg = Rgba([20, 40, 230, 255]);
    h.app.set_board_tool(board::BoardTool::Brush);
    let (a, b) = (world(&h, -80.0), world(&h, 80.0));
    h.app.finish_freehand_brush(vec![a, b]);
    h.app.board_colors.fg = Rgba([230, 20, 20, 255]);
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.frame();
    erase_across(&mut h, "board");

    let p = h.base.join("photo.png");
    std::fs::write(&p, b"png").unwrap();
    let item = h.app.add_paths(&[p])[0];
    let (a, b) = (world(&h, -120.0), world(&h, 120.0));
    let image = h.app.doc_mut().scene.build_node(
        slate_doc::scene::WorldRect::new(a.x, a.y - 60.0, b.x - a.x, 120.0),
        slate_doc::scene::NodeKind::Image(slate_doc::scene::ImageNode::new(item)),
    );
    let image_id = image.id;
    h.app.add_nodes(vec![image]);
    h.app.board_sel = std::iter::once(image_id).collect();
    h.app.board_colors.fg = Rgba([20, 40, 230, 255]);
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.sync_image_paint_for_tool();
    let (a, b) = (world(&h, -80.0), world(&h, 80.0));
    h.app.finish_freehand_brush(vec![a, b]);
    h.app.board_colors.fg = Rgba([230, 20, 20, 255]);
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.sync_image_paint_for_tool();
    assert!(
        h.app.image_paint_session().is_some(),
        "fixture: image paint"
    );
    h.frame();
    erase_across(&mut h, "image paint layer");

    // One neutral per theme, readable on that theme's board, for the fill and
    // the rim alike.
    fn luminance(c: egui::Color32) -> f32 {
        let lin = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }
    h.app.set_board_tool(board::BoardTool::Eraser);
    h.app.eraser_opacity = 1.0;
    for dark in [false, true] {
        h.app.dark_mode = dark;
        let ink = h.app.eraser_preview_color();
        let [r, g, b, a] = ink.to_srgba_unmultiplied();
        assert!(
            r == g && g == b && a == 255,
            "dark {dark}: the eraser preview is an opaque neutral gray, got {ink:?}"
        );
        let bg = atlas_shell::theme::Palette::for_mode(dark).bg;
        let (l1, l2) = (luminance(ink), luminance(bg));
        let contrast = (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05);
        assert!(
            contrast >= 3.0,
            "dark {dark}: eraser preview {ink:?} on board {bg:?} has contrast {contrast:.2}"
        );
        let out = h.frame_output(pointer_to(c + EVec2::new(dark as u8 as f32, 1.0), false));
        let mut rims = Vec::new();
        fn walk(shape: &egui::Shape, acc: &mut Vec<egui::Color32>) {
            match shape {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, acc)),
                egui::Shape::Circle(c) if c.stroke.width > 0.0 => acc.push(c.stroke.color),
                _ => {}
            }
        }
        for clipped in &out.shapes {
            walk(&clipped.shape, &mut rims);
        }
        assert!(
            rims.contains(&ink),
            "dark {dark}: the eraser tip rim is not the preview color: {rims:?}"
        );
    }
}

#[test]
fn rotate_hover_and_drag_show_a_circular_arrow_in_the_windows_cursor_scheme() {
    let mut h = Harness::new("rotate_pointer");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.app.set_board_tool(board::BoardTool::Select);
    add_rect(&mut h.app, 0.0, 0.0);
    let id = h.app.doc().scene.nodes[0].id;
    h.app.board_sel = std::iter::once(id).collect();
    // Far enough in that the corner rotate zone is clear of the side wire grips.
    h.app.tab_mut().cam.z = 3.0;
    h.frame();
    let xf = h.app.board_xf();
    let n = h.app.doc().scene.node(id).unwrap().clone();
    let p = board_handles::selection_geom(&xf, n.rect, n.rotation_deg).rotate_points[1];
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(p)));
    let hover = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(p)));
    assert_rotate_pointer(&hover, p, "hover");

    let press = |pressed: bool, at: Pos2| {
        move |i: &mut egui::RawInput| {
            i.events.push(egui::Event::PointerMoved(at));
            i.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
    };
    h.frame_with(press(true, p));
    let q = p + EVec2::new(0.0, 60.0);
    for step in 1..=4 {
        let at = p.lerp(q, step as f32 / 4.0);
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(at)));
    }
    assert!(
        matches!(h.app.board_drag, Some(board::BoardDrag::Rotate { .. })),
        "the press on the rotate zone rotates"
    );
    let drag = h.frame_output(|i| i.events.push(egui::Event::PointerMoved(q)));
    assert_rotate_pointer(&drag, q, "drag");
    h.frame_with(press(false, q));
}
