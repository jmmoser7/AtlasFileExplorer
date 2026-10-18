//! Input ownership and occlusion regressions.

use super::*;

#[test]
fn subselect_respects_space_pan_and_off_canvas_chrome() {
    for outside in [false, true] {
        let mut h = grip_board("subselect_input_guards");
        let (_, pts) = edge_polyline(&mut h);
        let at = if outside {
            let p = Pos2::new(
                h.app.canvas_rect.left() - 20.0,
                h.app.canvas_rect.center().y,
            );
            let xf = h.app.board_xf();
            add_seg(
                &mut h.app,
                xf.s2w(p - EVec2::new(0.0, 40.0)),
                xf.s2w(p + EVec2::new(0.0, 40.0)),
            );
            h.app.board_sel.clear();
            h.frame();
            p
        } else {
            let p = h.app.board_xf().w2s(mid(pts[1], pts[2]));
            h.frame_with(|i| i.events.push(key_event(egui::Key::Space, SUB_OBJECT)));
            p
        };
        h.frame_with(|i| {
            i.modifiers = SUB_OBJECT;
            i.events.push(egui::Event::PointerMoved(at));
        });
        h.frame_with(|i| {
            i.modifiers = SUB_OBJECT;
            i.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: SUB_OBJECT,
            });
        });
        assert!(
            h.app.direct.grip_points.edges.is_empty(),
            "outside={outside}"
        );
        assert!(
            h.app.board_drag.is_none(),
            "blocked chord starts no edge marquee"
        );
        let release = at + EVec2::new(20.0, 10.0);
        h.frame_with(|i| {
            i.modifiers = SUB_OBJECT;
            i.events.push(egui::Event::PointerMoved(release));
        });
        h.frame_with(|i| {
            i.modifiers = SUB_OBJECT;
            i.events.push(egui::Event::PointerButton {
                pos: release,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: SUB_OBJECT,
            });
        });
        assert!(h.app.direct.grip_points.edges.is_empty());
        assert!(h.app.board_drag.is_none());
    }
}

#[test]
fn opaque_body_blocks_subselect_of_a_curve_behind_it() {
    for locked in [false, true] {
        let mut h = grip_board("edge_occluded");
        let (_, pts) = edge_polyline(&mut h);
        let at = mid(pts[1], pts[2]);
        let cover = add_rect(&mut h.app, at.x - 30.0, at.y - 30.0);
        h.app.patch_nodes(&[cover], |n| {
            n.rect = WorldRect::new(at.x - 30.0, at.y - 30.0, 60.0, 60.0);
            n.locked = locked;
        });
        h.app.board_sel.clear();
        h.frame();
        let screen = h.app.board_xf().w2s(at);
        press_primary(&mut h, screen, SUB_OBJECT);
        assert!(h.app.direct.grip_points.edges.is_empty(), "locked={locked}");
        assert_eq!(h.app.board_sel, [cover].into_iter().collect());
    }
}
