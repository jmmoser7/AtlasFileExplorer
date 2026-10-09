//! Focused web-page input ownership tests.

use super::*;

/// The headline of this contract: with the pointer inside a focused page, the
/// wheel scrolls the page and the board does not zoom (D22).
#[test]
fn the_wheel_inside_a_focused_page_scrolls_it_instead_of_zooming_the_board() {
    let (mut h, _id, host) = focused_page("web_wheel");
    let zoom_before = h.app.tab().cam.z;
    wheel_over_page(&mut h, -50.0);

    let wheels: Vec<f32> = host.sent(|i| match i {
        board_web::WebInput::Wheel { delta, .. } => Some(*delta),
        _ => None,
    });
    assert!(!wheels.is_empty(), "the page never saw the wheel");
    assert_eq!(
        h.app.tab().cam.z,
        zoom_before,
        "the board must not zoom under the pointer"
    );
}

/// With focus released, the same notch is the camera's again.
#[test]
fn the_wheel_zooms_the_board_again_once_focus_is_released() {
    let (mut h, _id, host) = focused_page("web_wheel_release");
    h.app.web_blur();
    let zoom_before = h.app.tab().cam.z;
    wheel_over_page(&mut h, -50.0);

    assert_ne!(
        h.app.tab().cam.z,
        zoom_before,
        "the board zooms when no page holds the pointer"
    );
    assert!(
        host.sent(|i| matches!(i, board_web::WebInput::Wheel { .. }).then_some(()))
            .is_empty(),
        "an unfocused page hears nothing"
    );
}

/// `canvas.pan_scroll`: Shift + wheel pans the board sideways instead of
/// zooming. egui delivers a Shift wheel as a horizontal delta. A plain notch
/// still zooms.
#[test]
fn shift_wheel_pans_the_board_sideways() {
    let mut h = line_board("shift_wheel_pan");
    h.app.set_board_tool(board::BoardTool::Select);
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(pointer_to(c, false));
    let (z, offset) = (h.app.tab().cam.z, h.app.tab().cam.offset);
    let notch = |h: &mut Harness, mods: egui::Modifiers| {
        h.frame_with(|i| {
            i.modifiers = mods;
            i.events.push(egui::Event::PointerMoved(c));
            i.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: EVec2::new(0.0, -60.0),
                modifiers: mods,
            });
        });
    };
    notch(&mut h, egui::Modifiers::SHIFT);
    let cam = h.app.tab().cam;
    assert_eq!(cam.z, z, "Shift + wheel zoomed");
    assert_eq!(
        cam.offset.y, offset.y,
        "Shift + wheel moved the board up or down"
    );
    assert!(
        cam.offset.x - offset.x > 10.0 / z,
        "Shift + wheel did not pan: offset {:?} -> {:?}",
        offset,
        cam.offset
    );
    notch(&mut h, egui::Modifiers::NONE);
    assert_ne!(h.app.tab().cam.z, z, "a plain notch no longer zooms");
}

/// Typing into a form must not run board commands: bare letters are the page's
/// while it holds focus, and they arrive as text.
#[test]
fn typing_into_a_page_does_not_reach_the_board_tools() {
    let (mut h, _id, host) = focused_page("web_typing");
    let tool_before = h.app.board_tool;
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        // "r" is the rectangle tool's bare-letter shortcut.
        input.events.push(egui::Event::Key {
            key: egui::Key::R,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
        input.events.push(egui::Event::Text("r".into()));
    });

    assert_eq!(h.app.board_tool, tool_before, "no tool switch while typing");
    let text: Vec<char> = host.sent(|i| match i {
        board_web::WebInput::Text(c) => Some(*c),
        _ => None,
    });
    assert_eq!(text, vec!['r'], "the character reached the page");
}

/// Esc is the one key the page never gets, because it is how the human gets
/// back out (D22).
#[test]
fn escape_peels_focus_and_never_reaches_the_page() {
    let (mut h, id, host) = focused_page("web_escape");
    assert_eq!(h.app.web.focused, Some(id));
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });
    });

    assert_eq!(h.app.web.focused, None, "Esc released the page");
    assert!(
        host.sent(|i| match i {
            board_web::WebInput::Key { key, .. } => Some(*key),
            _ => None,
        })
        .iter()
        .all(|k| *k != egui::Key::Escape),
        "the page never sees Escape"
    );
}

/// Clicking outside a focused page makes it a preview card again; the click is
/// Slate's, not another page input event.
#[test]
fn primary_click_outside_a_focused_page_releases_focus() {
    let (mut h, id, host) = focused_page("web_click_out");
    let xf = h.app.board_xf();
    let node = h.app.doc().scene.node(id).unwrap();
    let frame = xf.rect_w2s(node.rect);
    let outside = if frame.left() > 24.0 {
        Pos2::new(frame.left() - 12.0, frame.center().y)
    } else {
        Pos2::new(frame.right() + 12.0, frame.center().y)
    };
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(outside));
        input.events.push(egui::Event::PointerButton {
            pos: outside,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });

    assert_eq!(h.app.web.focused, None, "outside click released the page");
    assert!(
        host.sent(|i| match i {
            board_web::WebInput::Down { .. } => Some(()),
            _ => None,
        })
        .is_empty(),
        "the outside click must not be forwarded to the page"
    );
}

/// Right-click inside a live page is the page's: no board menu, tip HUD, or
/// pan. The frame band around it still opens the Slate portal menu (D17/D22).
#[test]
fn right_click_inside_a_live_page_belongs_to_the_page() {
    let (mut h, id, host) = focused_page("web_rmb_page");
    let cam_before = h.app.tab().cam.offset;
    let right = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        input.events.push(right(center(), true));
    });
    assert!(!h.app.hud_right_held, "no tip HUD on a page right press");
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(center() + EVec2::new(40.0, 10.0)));
    });
    h.frame_with(|input| {
        input
            .events
            .push(right(center() + EVec2::new(40.0, 10.0), false));
    });

    assert!(h.app.board_menu.is_none(), "the board menu stays shut");
    assert!(h.app.brush_hud.is_none());
    assert_eq!(h.app.tab().cam.offset, cam_before, "no right-drag pan");
    let downs: Vec<u8> = host.sent(|i| match i {
        board_web::WebInput::Down { button, .. } => Some(*button),
        _ => None,
    });
    assert!(
        downs.contains(&1),
        "the page hears the right button: {downs:?}"
    );

    let xf = h.app.board_xf();
    let frame = xf.rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let band = Pos2::new(frame.left() + 1.0, frame.center().y);
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(band));
        input.events.push(right(band, true));
    });
    h.frame_with(|input| input.events.push(right(band, false)));
    assert_eq!(
        h.app.board_menu.map(|(n, _)| n),
        Some(id),
        "the frame band keeps the Slate portal menu"
    );
}

/// A drag inside the page selects text there rather than moving the node or
/// panning the board.
#[test]
fn a_drag_inside_a_focused_page_moves_nothing_on_the_board() {
    let (mut h, id, host) = focused_page("web_drag");
    let rect_before = h.app.doc().scene.node(id).unwrap().rect;
    let cam_before = h.app.tab().cam.offset;
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(center()));
        input.events.push(egui::Event::PointerButton {
            pos: center(),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
    });
    h.frame_with(|input| {
        input
            .events
            .push(egui::Event::PointerMoved(center() + EVec2::new(60.0, 20.0)));
    });

    let held: Vec<u8> = host.sent(|i| match i {
        board_web::WebInput::Move { buttons, .. } => Some(*buttons),
        _ => None,
    });
    assert!(
        held.contains(&1),
        "the page must know the button is held, or it cannot select text"
    );
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect_before);
    assert_eq!(h.app.tab().cam.offset, cam_before);
}

/// Dangerous locators are refused by name rather than quietly not loading
/// (D19, D30).
#[test]
fn a_javascript_locator_is_refused_and_says_why() {
    let mut h = web_board("web_refuse");
    with_fake_host(&mut h);
    h.app.place_web_portal_at(Pos2::ZERO);
    let (id, _) = only_portal(&h);
    h.app.bind_web_source(id, "javascript:alert(1)".into());
    web_settle(&mut h, &[(id, 600.0)], 2);
    match h.app.web.state(id) {
        board_web::WebState::Refused { reason } => {
            assert!(reason.contains("javascript"), "the reason names the scheme");
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(h.app.web.live_count(), 0);
}

/// One completed draw is one undo step, and undo removes the node.
#[test]
fn a_recipe_driven_draw_is_a_single_undo_step() {
    let mut h = kit_board("kit_undo", board::BoardTool::Ellipse);
    drag(
        &mut h,
        board::BoardTool::Ellipse,
        Pos2::new(0.0, 0.0),
        Pos2::new(80.0, 80.0),
    );
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.app.board_undo();
    assert!(h.app.doc().scene.nodes.is_empty(), "one gesture, one undo");
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.nodes.len(), 1);
    h.frame();
}

#[test]
fn workbook_camera_round_trips_through_save() {
    let mut h = Harness::new("cam_rt");
    h.seed();
    h.app.tab_mut().cam.offset = EVec2::new(120.0, -40.0);
    h.app.tab_mut().cam.z = 1.6;
    let path = h.base.join("cam.slate");
    let tab_id = h.app.tab().id;
    h.app.save_doc_to(tab_id, path.clone());

    let mut h2 = Harness::new("cam_rt_load");
    h2.app.open_doc_at(path);
    assert!((h2.app.tab().cam.offset.x - 120.0).abs() < 1e-3);
    assert!((h2.app.tab().cam.offset.y + 40.0).abs() < 1e-3);
    assert!((h2.app.tab().cam.z - 1.6).abs() < 1e-3);
}

#[test]
fn board_paint_culls_offscreen_nodes() {
    let mut h = Harness::new("board_cull");
    h.seed();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    for i in 0..200 {
        add_rect(&mut h.app, (i as f32) * 400.0, 0.0);
    }
    h.app.canvas_rect = ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0));
    h.app.tab_mut().cam.offset = EVec2::ZERO;
    h.app.tab_mut().cam.z = 1.0;
    let painted = h.app.board_paint_nodes(h.app.canvas_rect);
    assert!(
        painted.len() < 200,
        "viewport cull must drop off-screen nodes, got {}",
        painted.len()
    );
    assert!(
        !painted.is_empty(),
        "the camera must still see nearby nodes"
    );
}

#[test]
fn slate_thumb_lru_caps_resident_textures() {
    let mut h = Harness::new("thumb_lru");
    let cap = atlas_core::display::SLATE_TEXTURES.resident_cap;
    for i in 0..(cap + 100) {
        let k = format!("k{i}");
        h.app.textures.insert(k.clone(), ThumbState::Failed);
        h.app
            .thumb_pixels
            .insert(k.clone(), egui::ColorImage::example());
        h.app.thumb_used.insert(k, i as u64);
    }
    h.app.evict_thumbs();
    assert!(h.app.textures.len() <= cap);
    assert_eq!(h.app.textures.len(), h.app.thumb_pixels.len());
}

/// The gap inside a multi-selection's box moves the selection.
#[test]
fn a_press_in_the_selection_gap_moves_the_group() {
    let mut h = web_board("wire_sel_gap");
    let a = add_rect(&mut h.app, 0.0, 0.0);
    let b = add_rect(&mut h.app, 100.0, 0.0);
    h.app.board_sel = [a, b].into_iter().collect();
    h.app.set_board_tool(board::BoardTool::Select);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();

    let xf = h.app.board_xf();
    let world = Pos2::new(90.0, 30.0);
    let screen = xf.w2s(world);
    assert!(h.app.wire_grip_at(screen, &xf).is_none());
    let drag = h
        .app
        .begin_gesture_for_test(screen, world, egui::Modifiers::NONE);
    assert!(
        matches!(drag, Some(board::BoardDrag::Move { .. })),
        "the gap moves the selection"
    );
}
