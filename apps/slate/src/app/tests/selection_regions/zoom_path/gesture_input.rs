//! Zoom marquee, tool ownership, and Windows keyboard interruption tests.
use super::*;

fn navigation_button(pos: Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn z_marquee_owns_the_primary_drag_over_hand_and_select() {
    use egui::PointerButton::Primary;
    use std::time::{Duration, Instant};

    let settle_shortcut = |h: &mut Harness, key| {
        press_key(h, key);
        h.app
            .bare_letter_hold
            .as_mut()
            .expect("the keyboard shortcut waits for the command-entry window")
            .started_at = Instant::now() - Duration::from_secs(1);
        h.frame();
    };

    for switch_to_select in [false, true] {
        let mut h = grip_board("regions_zoom_tool_priority");
        let center = h.app.canvas_rect.center();
        settle_shortcut(&mut h, egui::Key::H);
        assert_eq!(h.app.board_tool, board::BoardTool::Pan);
        if switch_to_select {
            settle_shortcut(&mut h, egui::Key::V);
            assert_eq!(h.app.board_tool, board::BoardTool::Select);
        }
        let tool = h.app.board_tool;
        let camera = h.app.tab().cam;
        let start = center - EVec2::new(200.0, 135.0);
        let end = center + EVec2::new(150.0, 100.0);
        let world_center = h.app.screen_to_world(start.lerp(end, 0.5));

        settle_shortcut(&mut h, egui::Key::Z);
        assert!(h.app.zoom_tool_active(), "Zoom arms over {tool:?}");
        h.frame_with(|input| {
            input.events.extend([
                egui::Event::PointerMoved(start),
                navigation_button(start, Primary, true),
            ]);
        });
        let frame = h.frame_output(|input| input.events.push(egui::Event::PointerMoved(end)));
        assert_eq!(h.app.zoom_marquee.as_ref().map(|g| g.origin), Some(start));
        assert_eq!(
            frame.platform_output.cursor_icon,
            egui::CursorIcon::Crosshair
        );
        assert_eq!(h.app.tab().cam.offset, camera.offset);
        assert_eq!(h.app.tab().cam.z, camera.z);

        h.frame_with(|input| input.events.push(navigation_button(end, Primary, false)));
        assert!(h.app.zoom_marquee.is_none());
        assert!(h.app.tab().cam.z > camera.z);
        assert_eq!(h.app.tab().cam.offset, world_center.to_vec2());
        assert_eq!(h.app.board_tool, tool);
    }
}

#[test]
fn z_marquee_fits_the_ordered_release_when_the_drag_arrives_in_one_frame() {
    use egui::PointerButton::Primary;

    for tool in [board::BoardTool::Select, board::BoardTool::Pan] {
        let mut h = grip_board("regions_zoom_batched_drag");
        h.app.set_board_tool(tool);
        let center = h.app.canvas_rect.center();
        let start = center - EVec2::new(180.0, 120.0);
        let end = center + EVec2::new(140.0, 85.0);
        let world_center = h.app.screen_to_world(start.lerp(end, 0.5));
        let before = h.app.tab().cam;
        press_key(&mut h, egui::Key::Z);
        h.frame_with(|input| {
            input.events.extend([
                egui::Event::PointerMoved(start),
                navigation_button(start, Primary, true),
                egui::Event::PointerMoved(end),
                navigation_button(end, Primary, false),
                egui::Event::PointerMoved(end + EVec2::new(45.0, -25.0)),
            ]);
        });
        assert!(h.app.zoom_tool_active());
        assert!(h.app.zoom_marquee.is_none());
        assert!(h.app.tab().cam.z > before.z);
        assert_eq!(h.app.tab().cam.offset, world_center.to_vec2());
        assert_eq!(h.app.board_tool, tool);
    }
}

#[test]
fn z_click_uses_the_ordered_location_and_modifier() {
    use egui::PointerButton::Primary;

    let mut h = grip_board("regions_zoom_batched_click");
    let at = h.app.canvas_rect.center() + EVec2::new(-150.0, 130.0);
    let anchor = h.app.screen_to_world(at);
    let before = h.app.tab().cam.z;
    press_key(&mut h, egui::Key::Z);
    h.frame_with(|input| {
        input.events.extend([
            egui::Event::PointerMoved(at),
            navigation_button(at, Primary, true),
            egui::Event::PointerButton {
                pos: at,
                button: Primary,
                pressed: false,
                modifiers: egui::Modifiers::ALT,
            },
            egui::Event::PointerMoved(at + EVec2::new(40.0, 30.0)),
        ]);
    });
    assert!(h.app.zoom_tool_active());
    assert!((h.app.tab().cam.z - before / 1.5).abs() < 0.0001);
    assert!(h.app.screen_to_world(at).distance(anchor) < 0.0001);
}

#[test]
fn a_window_menu_space_press_at_home_without_a_release_does_not_disable_zoom() {
    let mut h = grip_board("regions_alt_space_window_menu");
    h.app.go_home();
    h.frame();
    h.frame_with(|input| {
        // Alt is already up in the frame snapshot. The OS still receives
        // the Space release, so Slate only gets this modified key-down.
        assert_eq!(input.modifiers, egui::Modifiers::NONE);
        input
            .events
            .push(key_event(egui::Key::Space, egui::Modifiers::ALT));
    });
    h.frame();
    assert!(!h.ctx.input(|input| input.key_down(egui::Key::Space)));

    h.app.leave_home();
    h.frame();
    let at = h.app.canvas_rect.center() + EVec2::new(-140.0, 105.0);
    let before = h.app.tab().cam.z;
    press_key(&mut h, egui::Key::Z);
    click_board(&mut h, at);
    assert!(h.app.zoom_tool_active());
    assert!((h.app.tab().cam.z - before * 1.5).abs() < 0.0001);
}

#[test]
fn a_window_menu_space_press_is_not_a_repeat_last_tap() {
    let mut h = grip_board("regions_alt_space_repeat");
    h.app.set_board_tool(board::BoardTool::Line);
    let release = || egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let repeats = |h: &Harness| {
        h.app
            .cmd_history
            .iter()
            .filter(|entry| entry.id.0 == "app.repeat_last")
            .count()
    };
    h.frame_with(|input| {
        input
            .events
            .extend([key_event(egui::Key::Space, egui::Modifiers::ALT), release()]);
    });
    assert_eq!(repeats(&h), 0);

    h.frame_with(|input| {
        input.events.extend([
            key_event(egui::Key::Space, egui::Modifiers::NONE),
            release(),
        ]);
    });
    assert_eq!(repeats(&h), 1);
}

#[test]
fn losing_window_focus_cancels_an_active_space_pan_without_key_up() {
    use egui::PointerButton::Primary;

    let mut h = grip_board("regions_space_pan_focus_loss");
    let start = h.app.canvas_rect.center() + EVec2::new(150.0, -60.0);
    let checkpoint = start + EVec2::new(20.0, -15.0);
    let end = checkpoint + EVec2::new(80.0, 90.0);
    let before = h.app.tab().cam;
    press_key(&mut h, egui::Key::Z);
    h.frame_with(|input| {
        input.events.extend([
            key_event(egui::Key::Space, egui::Modifiers::NONE),
            egui::Event::PointerMoved(start),
            navigation_button(start, Primary, true),
        ]);
    });
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(checkpoint)));
    assert!(h.app.zoom_tool_active());
    assert_eq!(
        h.app.tab().cam.offset,
        before.offset - (checkpoint - start) / before.z
    );
    let offset = h.app.tab().cam.offset;

    h.frame_with(|input| {
        input.focused = false;
        input.events.push(egui::Event::WindowFocused(false));
    });
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(end)));
    h.frame_with(|input| input.events.push(navigation_button(end, Primary, false)));
    assert!(!h.ctx.input(|input| input.key_down(egui::Key::Space)));
    assert!(h.app.zoom_marquee.is_none());
    assert_eq!(h.app.tab().cam.offset, offset);
    assert_eq!(h.app.tab().cam.z, before.z);
}

#[test]
fn changing_tools_cancels_a_hand_owned_primary_pan() {
    use egui::PointerButton::Primary;

    let mut h = grip_board("regions_abandoned_hand_capture");
    h.app.set_board_tool(board::BoardTool::Pan);
    let start = h.app.canvas_rect.center() + EVec2::new(90.0, -95.0);
    let end = start + EVec2::new(32.0, 20.0);
    let camera = h.app.tab().cam;
    h.frame_with(|input| {
        input.events.extend([
            egui::Event::PointerMoved(start),
            navigation_button(start, Primary, true),
        ]);
    });
    h.app.set_board_tool(board::BoardTool::Brush);
    h.frame_with(|input| input.events.push(egui::Event::PointerMoved(end)));
    h.frame_with(|input| input.events.push(navigation_button(end, Primary, false)));
    assert_eq!(h.app.tab().cam.offset, camera.offset);
    assert_eq!(h.app.tab().cam.z, camera.z);
}

#[test]
fn explicit_board_pans_remain_available_while_zoom_is_armed() {
    use egui::PointerButton::{Middle, Primary, Secondary};

    for button in [Primary, Middle, Secondary] {
        let mut h = grip_board("regions_zoom_explicit_pan");
        press_key(&mut h, egui::Key::Z);
        let camera = h.app.tab().cam;
        let start = h.app.canvas_rect.center() + EVec2::new(140.0, -65.0);
        let end = start + EVec2::new(32.0, 18.0);
        let space = |pressed| egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        h.frame_with(|input| {
            input.events.push(egui::Event::PointerMoved(start));
            if button == Primary {
                input.events.push(space(true));
            }
            input.events.push(navigation_button(start, button, true));
        });
        assert!(h.app.zoom_tool_active());
        h.frame_with(|input| input.events.push(egui::Event::PointerMoved(end)));
        assert!(h.app.zoom_marquee.is_none());
        h.frame_with(|input| {
            input.events.push(navigation_button(end, button, false));
            if button == Primary {
                input.events.push(space(false));
            }
        });
        assert!(h.app.zoom_tool_active());
        assert!(h.app.zoom_marquee.is_none());
        assert_eq!(h.app.tab().cam.z, camera.z);
        assert_eq!(
            h.app.tab().cam.offset,
            camera.offset - (end - start) / camera.z
        );
    }
}
