use super::tests::live_model;
use super::*;

#[test]
fn model_input_routes_orbit_pan_and_zoom_without_moving_the_board() {
    check_model_input(false, egui::PointerButton::Primary);
}

#[test]
fn compact_model_input_keeps_orbit_pan_and_zoom_in_the_viewport() {
    check_model_input(true, egui::PointerButton::Primary);
}

#[test]
fn rhino_right_drag_routes_orbit_shift_pan_and_zoom_without_board_motion() {
    check_model_input(false, egui::PointerButton::Secondary);
    check_model_input(true, egui::PointerButton::Secondary);
}

#[test]
fn compact_model_double_click_enters_the_live_viewport() {
    check_model_entry(true, false);
}

#[test]
fn model_double_click_enters_selected_and_unselected_viewports() {
    for selected in [false, true] {
        check_model_entry(false, selected);
    }
}

fn check_model_entry(compact: bool, selected: bool) {
    let (mut h, id) = live_model("model_entry");
    h.app.lock_model(id);
    h.app.model3d.headless_live = true;
    let (x, y) = h.app.doc().scene.node(id).unwrap().rect.center();
    let (width, height, zoom) = if compact {
        (48.0, 36.0, 1.0)
    } else {
        (654.0, 490.0, 0.5)
    };
    h.app.patch_nodes(&[id], |node| {
        node.rect =
            slate_doc::scene::WorldRect::new(x - width * 0.5, y - height * 0.5, width, height);
    });
    h.app.tab_mut().cam.z = zoom;
    h.app.board_sel.clear();
    if selected {
        h.app.board_sel.insert(id);
    }
    h.frame();
    let at = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect)
        .center();
    for _ in 0..3 {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(at)));
    }
    for pressed in [true, false, true, false] {
        let time = h.ctx.input(|input| input.time) + 0.025;
        h.frame_with(|input| {
            input.time = Some(time);
            input.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        });
    }
    assert!(
        h.app.model3d.live.contains_key(&id),
        "double-click enters 3D"
    );
}

fn check_model_input(compact: bool, button: egui::PointerButton) {
    let (mut h, id) = live_model("model_input");
    if compact {
        let (x, y) = h.app.doc().scene.node(id).unwrap().rect.center();
        h.app.patch_nodes(&[id], |node| {
            node.rect = slate_doc::scene::WorldRect::new(x - 24.0, y - 18.0, 48.0, 36.0);
        });
        h.app.tab_mut().cam.z = 1.0;
        h.frame();
    }
    let rect = h.app.doc().scene.node(id).unwrap().rect;
    let board_cam = h.app.tab().cam;
    let start = h.app.board_xf().rect_w2s(rect).center();
    let world = h.app.board_xf().s2w(start);
    assert_eq!(h.app.live_model_at(world.x, world.y), Some(id));
    assert_eq!(h.app.board_tool, super::super::board::BoardTool::Select);
    // egui Areas settle their size across initial passes; selection
    // chrome must have its final hit rect before the pointer presses.
    for _ in 0..3 {
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(start)));
    }
    for (pan, dolly) in [(false, false), (true, false), (false, true)] {
        let before = h.app.model3d.live[&id].cam;
        let modifiers = egui::Modifiers {
            shift: pan,
            ctrl: dolly,
            ..Default::default()
        };
        h.frame_with(|input| {
            input.modifiers = modifiers;
            input.events.push(egui::Event::PointerMoved(start));
            input.events.push(egui::Event::PointerButton {
                pos: start,
                button,
                pressed: true,
                modifiers,
            });
        });
        for dx in [20.0, 60.0] {
            h.frame_with(|input| {
                input.modifiers = modifiers;
                input
                    .events
                    .push(egui::Event::PointerMoved(start + egui::vec2(dx, 20.0)));
            });
            assert!(
                matches!(
                    h.app.board_drag,
                    Some(super::super::board::BoardDrag::ModelOrbit { .. })
                ),
                "drag routing: {:?}, align={}, pointer={:?}, canvas={:?}",
                h.app.board_drag.as_ref().map(std::mem::discriminant),
                h.app.board_align_eat_press,
                h.ctx.pointer_hover_pos(),
                h.app.canvas_rect
            );
        }
        h.frame_with(|input| {
            input.modifiers = modifiers;
            input.events.push(egui::Event::PointerButton {
                pos: start + egui::vec2(60.0, 20.0),
                button,
                pressed: false,
                modifiers,
            });
        });
        let after = h.app.model3d.live[&id].cam;
        if dolly {
            assert_ne!(
                after.distance, before.distance,
                "Ctrl+drag dollies the model"
            );
            assert_eq!(after.target, before.target);
            assert_eq!((after.yaw, after.pitch), (before.yaw, before.pitch));
        } else if pan {
            assert_ne!(after.target, before.target, "Shift+drag pans the model");
            assert_eq!((after.yaw, after.pitch), (before.yaw, before.pitch));
        } else {
            assert_ne!(after.yaw, before.yaw, "drag orbits the model");
            assert_eq!(after.target, before.target);
        }
    }
    let distance = h.app.model3d.live[&id].cam.distance;
    h.frame_with(|input| {
        input.events.push(egui::Event::PointerMoved(start));
        input.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 50.0),
            modifiers: Default::default(),
        });
    });
    assert_ne!(h.app.model3d.live[&id].cam.distance, distance);
    assert_eq!(h.app.doc().scene.node(id).unwrap().rect, rect);
    assert_eq!(h.app.tab().cam.offset, board_cam.offset);
    assert_eq!(h.app.tab().cam.z, board_cam.z);
}
