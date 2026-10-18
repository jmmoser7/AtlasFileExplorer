//! Transient right-button readouts follow the active light/dark theme.

use super::*;

#[test]
fn tip_hud_and_shift_cursor_readouts_switch_theme_ink_live() {
    let mut h = grip_board("tip_readout_theme");
    h.app.set_board_tool(board::BoardTool::Brush);
    let pointer = h.app.canvas_rect.center();
    let origin = h.app.board_xf().s2w(pointer);
    let (width0, softness0, opacity0) = h.app.active_tip();
    for dark in [false, true, false] {
        h.app.dark_mode = dark;
        let ink = h.app.palette().ink;
        for hud in [
            Some(board_color::BrushHud::Size {
                origin,
                width0,
                softness0,
            }),
            Some(board_color::BrushHud::Opacity { origin, opacity0 }),
            None,
        ] {
            h.app.brush_hud = hud;
            h.app.shift_down = true;
            let label = h.app.size_hud_label().unwrap();
            let out = h.ctx.run(egui::RawInput::default(), |ctx| {
                let painter = ctx.layer_painter(egui::LayerId::background());
                if hud.is_some() {
                    h.app.paint_brush_hud(&painter, pointer, ink);
                } else {
                    h.app.paint_width_cursor(&painter, pointer);
                }
            });
            let text = out
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                    _ => None,
                })
                .expect("the live readout is painted");
            assert!(
                text.galley
                    .job
                    .sections
                    .iter()
                    .all(|section| section.format.color == ink),
                "light/dark changes apply to the live readout: dark={dark}, hud={hud:?}"
            );
        }
    }
}
