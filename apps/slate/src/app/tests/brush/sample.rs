//! Alt-click sampling of the foreground color.

use super::*;

/// Alt+left-click with the Brush samples instead of painting; releasing Alt
/// paints again.
#[test]
fn brush_alt_click_samples_and_releasing_alt_paints_again() {
    let mut h = web_board("brush_alt_sample");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(pointer_to(c, true));
    assert!(h.app.eyedropper_active(), "Alt spring-loads the eyedropper");
    h.frame_with(primary_button(c, true, true));
    h.frame_with(primary_button(c, false, true));
    assert!(
        h.app.doc().scene.nodes.is_empty(),
        "Alt+click painted a dab"
    );
    assert_eq!(h.app.board_tool, board::BoardTool::Brush);

    h.frame_with(pointer_to(c, false));
    assert!(!h.app.eyedropper_active());
    h.frame_with(primary_button(c, true, false));
    for i in 1..=6 {
        h.frame_with(pointer_to(c + EVec2::new(i as f32 * 12.0, 0.0), false));
    }
    h.frame_with(primary_button(c + EVec2::new(80.0, 0.0), false, false));
    assert_eq!(h.app.doc().scene.nodes.len(), 1, "plain drag paints again");
}

/// The Alt sample reads the screen pixel under the hotspot, so the sampling
/// cursor must leave that pixel showing the canvas, not the current color.
#[test]
fn brush_alt_sampling_cursor_leaves_the_hotspot_clear() {
    use eframe::epaint::{Shape, Vertex};
    fn covers(shape: &Shape, p: Pos2, fg: egui::Color32) -> bool {
        match shape {
            Shape::Vec(list) => list.iter().any(|s| covers(s, p, fg)),
            Shape::Circle(c) => {
                let d = c.center.distance(p);
                (c.fill == fg && d <= c.radius)
                    || (c.stroke.color == fg && (d - c.radius).abs() <= c.stroke.width * 0.5)
            }
            Shape::Mesh(m) => m.indices.chunks(3).any(|t| {
                let v: Vec<&Vertex> = t.iter().map(|i| &m.vertices[*i as usize]).collect();
                v.iter().all(|v| v.color == fg) && {
                    let s =
                        |a: Pos2, b: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                    let (d0, d1, d2) = (
                        s(v[0].pos, v[1].pos),
                        s(v[1].pos, v[2].pos),
                        s(v[2].pos, v[0].pos),
                    );
                    (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0) || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
                }
            }),
            _ => false,
        }
    }
    let mut h = web_board("brush_alt_cursor");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg = slate_doc::scene::Rgba([12, 200, 34, 255]);
    let fg = board::rgba32(h.app.board_colors.fg);
    h.frame();
    let c = h.app.canvas_rect.center();
    h.frame_with(pointer_to(c, true));
    let mut input = egui::RawInput {
        screen_rect: Some(ERect::from_min_size(Pos2::ZERO, EVec2::new(1440.0, 900.0))),
        ..Default::default()
    };
    pointer_to(c, true)(&mut input);
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let out = ctx.run(input, |c| app.update_app(c));
    assert!(h.app.eyedropper_active());
    let hits = out
        .shapes
        .iter()
        .filter(|s| covers(&s.shape, c, fg))
        .count();
    assert_eq!(
        hits, 0,
        "the sampling cursor paints the foreground on the hotspot"
    );
}

#[test]
fn a_brush_sample_sets_the_foreground_and_recent_colors() {
    let mut h = web_board("brush_sample_apply");
    h.app.set_board_tool(board::BoardTool::Brush);
    h.app.board_colors.fg = slate_doc::scene::Rgba([1, 2, 3, 200]);
    h.app.adopt_sampled_brush_color([90, 140, 210]);
    assert_eq!(h.app.board_colors.fg.0, [90, 140, 210, 200]);
    let recent = h.app.doc().view.recent_colors.clone().unwrap_or_default();
    assert_eq!(recent.first().copied(), Some([90, 140, 210]));
}
