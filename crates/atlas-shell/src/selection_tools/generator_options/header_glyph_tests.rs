use super::*;

#[test]
fn unsupported_custom_row_dims_actual_glyph_and_remains_disabled() {
    let ctx = egui::Context::default();
    let mut open = true;
    let mut popup = None;
    let mut output = None;
    for _ in 0..3 {
        output = Some(ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    popup = paint(
                        ui,
                        Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(14.0, 24.0)),
                        Id::new("disabled-custom"),
                        &mut open,
                        OptionsPage::Aspect,
                        OptionsData {
                            aspects: &["Source", "1:1", "3:2", "2:3", "16:9"],
                            preserve: Some(false),
                            ..Default::default()
                        },
                        1.0,
                        Palette::light(),
                    )
                    .menu
                    .and_then(|menu| menu.popup);
                });
            },
        ));
    }
    let output = output.unwrap();
    let popup = popup.unwrap();
    let text = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Custom…" => {
                Some(text.visual_bounding_rect())
            }
            _ => None,
        })
        .expect("actual disabled Custom row");
    let row = Rect::from_min_max(
        Pos2::new(popup.left(), text.center().y - 15.0),
        Pos2::new(popup.right(), text.center().y + 15.0),
    );
    let alpha = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh)
                if !mesh.vertices.is_empty()
                    && mesh.vertices.iter().all(|vertex| row.contains(vertex.pos)) =>
            {
                Some(
                    mesh.vertices
                        .iter()
                        .map(|vertex| vertex.color.a())
                        .max()
                        .unwrap(),
                )
            }
            _ => None,
        })
        .max()
        .expect("actual registered MenuImage glyph");
    assert!(
        alpha > 0 && alpha <= 102,
        "whole disabled glyph must obey CSS opacity .4, got {alpha}"
    );
}

#[test]
fn compact_options_paints_shared_ellipsis_and_clicks_without_changing_model_caret() {
    for zoom in [0.5, 1.0, 2.0] {
        for theme in [Palette::light(), Palette::dark()] {
            let ctx = egui::Context::default();
            let mut options_open = false;
            let mut model_open = false;
            let mut popup = None;
            let model = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(140.0, 24.0) * zoom);
            let options = Rect::from_min_size(
                Pos2::new(model.right() + zoom, 100.0),
                Vec2::new(14.0, 24.0) * zoom,
            );
            let id = Id::new("glyph-options");
            let mut frame = |events: Vec<egui::Event>| {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
                        events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            crate::selection_tools::generator::model_picker(
                                ui,
                                model,
                                &["Auto"],
                                0,
                                &mut model_open,
                                Id::new("glyph-model"),
                                PickerPage::Root,
                                &[] as &[ModelParameter<'static>; 0],
                                None,
                                false,
                                zoom,
                                theme,
                            );
                            popup = paint(
                                ui,
                                options,
                                id,
                                &mut options_open,
                                OptionsPage::Root,
                                OptionsData {
                                    preserve: Some(false),
                                    ..Default::default()
                                },
                                zoom,
                                theme,
                            )
                            .menu
                            .and_then(|menu| menu.popup);
                        });
                    },
                )
            };
            let output = frame(vec![]);
            let segments = |rect: Rect| {
                output.shapes.iter().filter(|shape| matches!(&shape.shape,
                egui::Shape::LineSegment {points,..} if points.iter().all(|point|rect.contains(*point)))).count()
            };
            assert_eq!(segments(model), 2, "model keeps its own caret");
            assert_eq!(segments(options), 0, "options has no extra caret");
            let dots = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Circle(circle) if options.contains(circle.center) => Some(circle),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                dots.len(),
                3,
                "native More has three painted circles, independent of fonts"
            );
            let size = 14.0 * zoom;
            for (dot, offset) in dots.iter().zip([-0.3, 0.0, 0.3]) {
                assert!((dot.center.x - (options.center().x + size * offset)).abs() < 0.001);
                assert!((dot.center.y - options.center().y).abs() < 0.001);
                assert!((dot.radius - size * 0.07).abs() < 0.001);
                assert_eq!(dot.fill, theme.sub);
                assert!(options.contains_rect(Rect::from_center_size(
                    dot.center,
                    Vec2::splat(2.0 * dot.radius)
                )));
            }
            assert!(
                !output.shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if !text.galley.text().is_empty()
                    && options.intersects(text.visual_bounding_rect()))),
                "no font glyph can become a tofu square"
            );
            assert_eq!(
                ctx.read_response(id).unwrap().rect,
                options,
                "same14-unit hitbox"
            );
            let mut named_click = false;
            for pressed in [true, false] {
                let out = frame(vec![
                    egui::Event::PointerMoved(options.center()),
                    egui::Event::PointerButton {
                        pos: options.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]);
                named_click |= out.platform_output.events.iter().any(|event| matches!(event,
                    egui::output::OutputEvent::Clicked(info) if info.label.as_deref() == Some("Generator options")));
            }
            frame(vec![]);
            drop(frame);
            assert!(options_open);
            assert!(!model_open);
            assert!(
                named_click,
                "icon-only control retains an accessible Options label"
            );
            let popup = popup.expect("same existing options popup opens");
            assert_eq!(
                ctx.layer_id_at(popup.center()),
                Some(egui::LayerId::new(egui::Order::Tooltip, id.with("pop")))
            );
        }
    }
}
