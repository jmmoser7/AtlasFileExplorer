use super::*;
use crate::app::tests::{edge_polyline, grip_board, mid, SUB_OBJECT};

#[test]
fn grip_drag_handoff_respects_covering_area_inline_editor_and_space() {
    use crate::app::tests::{closed_form_board, key_event};
    use slate_doc::scene::ShapeKind;

    for blocker in ["none", "area", "inline", "space"] {
        let (mut h, id, _) = closed_form_board("grip_drag_handoff", ShapeKind::Rect);
        let center = h.app.board_xf().s2w(h.app.canvas_rect.center());
        h.app.patch_nodes(&[id], |n| {
            n.rect = WorldRect::new(center.x - 2.0, center.y - 1.5, 4.0, 3.0);
        });
        h.frame();
        let at = h.app.board_xf().w2s(center);
        assert_eq!(h.app.tiny_body_owns_pointer(at), Some(id));
        if blocker == "inline" {
            h.app.open_corner_entry(id, None);
            assert!(h.app.property_inline_editor_open());
        }
        let chrome_rect = Rect::from_center_size(at, Vec2::splat(40.0));
        h.app.shape_properties.chrome_hits.push(chrome_rect);
        let ctx = egui::Context::default();
        for step in 0..4 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                ..Default::default()
            };
            let pointer = if step == 3 {
                at + Vec2::new(30.0, 10.0)
            } else {
                at
            };
            input.events.push(egui::Event::PointerMoved(pointer));
            if step == 2 {
                input.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
                if blocker == "space" {
                    input
                        .events
                        .push(key_event(egui::Key::Space, egui::Modifiers::NONE));
                }
            }
            let _ = ctx.run(input, |ctx| {
                let layer = atlas_shell::selection_tools::selection_chrome_layer();
                egui::Area::new(layer.id)
                    .order(layer.order)
                    .fixed_pos(chrome_rect.min)
                    .sense(egui::Sense::hover())
                    .movable(false)
                    .fade_in(false)
                    .show(ctx, |ui| {
                        ui.allocate_space(chrome_rect.size());
                    });
                if blocker == "area" {
                    egui::Area::new(egui::Id::new("grip_covering_area"))
                        .order(egui::Order::Tooltip)
                        .fixed_pos(chrome_rect.min)
                        .fade_in(false)
                        .show(ctx, |ui| {
                            ui.allocate_space(chrome_rect.size());
                        });
                }
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.allocate_rect(h.app.canvas_rect, Sense::click_and_drag());
                    let allowed = !ui.input(|i| i.key_down(egui::Key::Space));
                    let claimed = h.app.canvas_grip_drag_response(ui, response, allowed);
                    if step == 3 {
                        assert_eq!(
                            claimed.drag_started_by(egui::PointerButton::Primary),
                            blocker == "none",
                            "blocker={blocker}"
                        );
                        assert_eq!(ctx.dragged_id() == Some(claimed.id), blocker == "none");
                    }
                });
            });
        }
        assert!(
            h.app.board_drag.is_none(),
            "handoff only assigns the egui owner"
        );
    }
}

#[test]
fn covering_area_and_open_editor_block_raw_edge_input() {
    // Exercise the ordered input owner with real press events and an actual
    // overlapping Area. No popup memory flag is set by these Areas.
    for editor in [false, true] {
        let mut h = grip_board("edge_layer_guards");
        let (_, pts) = edge_polyline(&mut h);
        let xf = h.app.board_xf();
        let at = xf.w2s(mid(pts[1], pts[2]));
        h.app.ctrl_down = true;
        h.app.shift_down = true;
        let layer = if editor {
            h.app.shape_properties.panel = Some(crate::app::board_properties::Panel::Stroke);
            atlas_shell::selection_tools::selection_chrome_layer()
        } else {
            egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("unrelated_covering_area"),
            )
        };
        let ctx = egui::Context::default();
        for pressed in [None, None, Some(true)] {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                modifiers: SUB_OBJECT,
                ..Default::default()
            };
            input.events.push(egui::Event::PointerMoved(at));
            if let Some(pressed) = pressed {
                input.events.push(egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: SUB_OBJECT,
                });
            }
            let _ = ctx.run(input, |ctx| {
                egui::Area::new(layer.id)
                    .order(layer.order)
                    .fixed_pos(at - Vec2::splat(20.0))
                    .fade_in(false)
                    .show(ctx, |ui| {
                        ui.allocate_space(Vec2::splat(40.0));
                    });
                egui::CentralPanel::default().show(ctx, |ui| {
                    if pressed.is_some() {
                        assert_eq!(ctx.layer_id_at(at), Some(layer));
                        assert!(!ctx.memory(|m| m.any_popup_open()));
                        h.app.curve_subselect_input(ui, &xf, Some(at), true);
                    }
                });
            });
        }
        assert!(h.app.board_drag.is_none(), "editor={editor}");
        assert!(h.app.direct.grip_points.edges.is_empty());
    }
}
