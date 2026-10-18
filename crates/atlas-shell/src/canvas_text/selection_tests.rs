//! Native selection and clipped canvas-text input ownership.
use super::*;

#[test]
fn scaled_selection_copies_text_at_multiple_zooms() {
    for (scale, clipped) in [0.5, 1.0, 2.0]
        .into_iter()
        .flat_map(|scale| [false, true].map(|clipped| (scale, clipped)))
    {
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut frame = |events| {
            time += 0.1;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let galley = ui.painter().layout_no_wrap(
                            if clipped {
                                "Hidden first line\nSelectable reply"
                            } else {
                                "Selectable reply"
                            }
                            .into(),
                            FontId::proportional(20.0),
                            Color32::WHITE,
                        );
                        let offset = if clipped {
                            galley.rows[1].rect.min.y
                        } else {
                            0.0
                        };
                        if clipped {
                            ui.set_clip_rect(Rect::from_min_size(
                                Pos2::new(50.0, 50.0),
                                egui::vec2(400.0, 30.0) * scale,
                            ));
                        }
                        Scaled::from_galley(galley, scale).selectable(
                            ui,
                            egui::Id::new("reply"),
                            Pos2::new(50.0, 50.0 - offset * scale),
                            Color32::WHITE,
                        );
                    });
                },
            )
        };
        frame(vec![]);
        frame(vec![]);
        if clipped {
            assert_ne!(
                ctx.layer_id_at(Pos2::new(60.0, 45.0)),
                Some(egui::LayerId::new(
                    egui::Order::Background,
                    egui::Id::new("reply")
                )),
                "clipped-out text cannot own a header click at scale {scale}"
            );
        }
        let first = Pos2::new(50.0, 50.0 + 10.0 * scale);
        let last = Pos2::new(50.0 + 160.0 * scale, first.y);
        frame(vec![
            egui::Event::PointerMoved(first),
            egui::Event::PointerButton {
                pos: first,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        frame(vec![egui::Event::PointerMoved(last)]);
        frame(vec![egui::Event::PointerButton {
            pos: last,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        let output = frame(vec![egui::Event::Copy]);
        assert!(
            output
                .platform_output
                .commands
                .iter()
                .any(|c| matches!(c,egui::OutputCommand::CopyText(s) if s=="Selectable reply")),
            "copy failed at scale {scale}, clipped={clipped}: {:?}",
            output.platform_output.commands
        );
    }
}

#[test]
fn selected_text_remains_copyable_after_scrolling_fully_out_of_view() {
    for scale in [0.5, 1.0, 2.0] {
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut frame = |hidden, events| {
            time += 0.1;
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let galley = ui.painter().layout_no_wrap(
                            "Selectable reply".into(),
                            FontId::proportional(20.0),
                            Color32::WHITE,
                        );
                        let top = if hidden {
                            49.0 - galley.size().y * scale
                        } else {
                            50.0
                        };
                        ui.set_clip_rect(Rect::from_min_size(
                            Pos2::new(50.0, 50.0),
                            Vec2::new(400.0, 30.0) * scale,
                        ));
                        Scaled::from_galley(galley, scale).selectable(
                            ui,
                            egui::Id::new("reply"),
                            Pos2::new(50.0, top),
                            Color32::WHITE,
                        );
                    });
                },
            )
        };
        frame(false, vec![]);
        frame(false, vec![]);
        let first = Pos2::new(50.0, 50.0 + 10.0 * scale);
        let selected_width = ctx.fonts(|fonts| {
            fonts
                .layout_no_wrap(
                    "Selectable reply".into(),
                    FontId::proportional(20.0),
                    Color32::WHITE,
                )
                .pos_from_ccursor(egui::text::CCursor::new("Selectable".len()))
                .left()
        });
        let last = Pos2::new(50.0 + selected_width * scale, first.y);
        frame(
            false,
            vec![
                egui::Event::PointerMoved(first),
                egui::Event::PointerButton {
                    pos: first,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        );
        frame(false, vec![egui::Event::PointerMoved(last)]);
        frame(
            false,
            vec![egui::Event::PointerButton {
                pos: last,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        );
        assert!(egui::text_selection::LabelSelectionState::load(&ctx).has_selection());
        frame(true, vec![]);
        frame(true, vec![]);
        assert!(
            egui::text_selection::LabelSelectionState::load(&ctx).has_selection(),
            "hidden selection was cleared at {scale}"
        );
        assert_ne!(
            ctx.layer_id_at(Pos2::new(60.0, 45.0)),
            Some(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("reply")
            )),
            "fully hidden text cannot own the adjacent header"
        );
        let output = frame(true, vec![egui::Event::Copy]);
        assert!(output.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(text) if text == "Selectable")), "copy after scrolling failed at {scale}: {:?}", output.platform_output.commands);
    }
}
