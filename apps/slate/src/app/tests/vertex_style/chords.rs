//! Alt-right scale and smooth tip chords.

use super::*;

#[test]
fn alt_right_drag_scales_about_the_press_point_through_real_frames() {
    let mut h = Harness::new("hud_center");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.frame();
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.brush_width = 20.0;
    let press = h.app.canvas_rect.center();
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    let cam0 = h.app.tab().cam.offset;
    h.frame_with(|i| {
        i.modifiers = alt;
        i.events.push(egui::Event::PointerMoved(press));
    });
    h.frame_with(|i| {
        i.modifiers = alt;
        i.events.push(egui::Event::PointerButton {
            pos: press,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: alt,
        });
    });
    for k in 1..=6 {
        let p = press + egui::vec2(10.0 * k as f32, -4.0 * k as f32);
        h.frame_with(|i| {
            i.modifiers = alt;
            i.events.push(egui::Event::PointerMoved(p));
        });
    }
    let origin = match h.app.brush_hud {
        Some(board_color::BrushHud::Size { origin, .. }) => origin,
        other => panic!("size hud open, got {other:?}"),
    };
    assert_eq!(
        h.app.board_xf().w2s(origin),
        press,
        "HUD center is the press point, pinned to the canvas"
    );
    assert_eq!(
        h.app.tab().cam.offset,
        cam0,
        "the canvas does not move under the HUD"
    );
    assert!(h.app.brush_width > 20.0);
}

/// ts3 (user pass, 28 September 2026): with Smooth armed, Alt+right-drag
/// scrubs its size and softness, Shift+right-drag its strength, and
/// Ctrl+right-drag shows no color wheel.
#[test]
fn smooth_tip_chords_scrub_size_softness_and_strength_with_no_wheel() {
    let mut h = Harness::new("smooth_chords");
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = ViewKind::Board;
    h.frame();
    h.app.set_board_tool(board::BoardTool::Smooth);
    h.app.tab_mut().cam.z = 1.0;
    h.app.smooth_width = 40.0;
    h.app.smooth_softness = 0.5;
    h.app.smooth_strength = 0.5;
    h.frame();
    let press = h.app.canvas_rect.center();
    let alt = egui::Modifiers {
        alt: true,
        ..Default::default()
    };
    right_chord(&mut h, press, EVec2::new(30.0, -30.0), alt, |h| {
        assert!(
            matches!(h.app.brush_hud, Some(board_color::BrushHud::Size { .. })),
            "Alt+right-drag opens the size HUD: {:?}",
            h.app.brush_hud
        );
    });
    assert!(
        h.app.smooth_width > 40.0 + 10.0,
        "size grew: {}",
        h.app.smooth_width
    );
    assert!(
        h.app.smooth_softness > 0.5,
        "softness rose: {}",
        h.app.smooth_softness
    );
    assert!(
        (h.app.smooth_strength - 0.5).abs() < 1e-6,
        "strength untouched"
    );

    let (width, softness) = (h.app.smooth_width, h.app.smooth_softness);
    right_chord(
        &mut h,
        press,
        EVec2::new(0.0, -30.0),
        egui::Modifiers::SHIFT,
        |h| {
            assert!(
                matches!(h.app.brush_hud, Some(board_color::BrushHud::Opacity { .. })),
                "Shift+right-drag opens the strength HUD: {:?}",
                h.app.brush_hud
            );
        },
    );
    assert!(
        h.app.smooth_strength > 0.6,
        "strength rose: {}",
        h.app.smooth_strength
    );
    assert_eq!(
        (h.app.smooth_width, h.app.smooth_softness),
        (width, softness)
    );

    let fg = h.app.board_colors.fg;
    let strength = h.app.smooth_strength;
    right_chord(
        &mut h,
        press,
        EVec2::new(40.0, 20.0),
        egui::Modifiers::CTRL,
        |h| {
            assert!(
                !matches!(h.app.brush_hud, Some(board_color::BrushHud::Wheel { .. })),
                "Ctrl+right-drag shows no color wheel for Smooth"
            );
        },
    );
    assert_eq!(h.app.board_colors.fg, fg, "no color was picked");
    assert_eq!(
        (
            h.app.smooth_width,
            h.app.smooth_softness,
            h.app.smooth_strength
        ),
        (width, softness, strength)
    );
}
