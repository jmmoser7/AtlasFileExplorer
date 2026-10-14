use super::*;

fn frame(ctx: &egui::Context, point: Pos2, pressed: Option<bool>) -> bool {
    let mut events = vec![egui::Event::PointerMoved(point)];
    if let Some(pressed) = pressed {
        events.push(egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    let mut clicked = false;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            time: Some(ctx.input(|i| i.time) + 1.0 / 60.0),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.set_clip_rect(Rect::from_min_size(
                    Pos2::new(100.0, 100.0),
                    Vec2::splat(100.0),
                ));
                let rect = Rect::from_min_size(Pos2::new(80.0, 80.0), Vec2::splat(160.0));
                let controls = object_ui(ui, Id::new("clipped-strip"), rect);
                clicked = controls
                    .interact(rect, Id::new("clipped-strip-button"), Sense::click())
                    .clicked();
            });
        },
    );
    clicked
}

#[test]
fn selection_chrome_interacts_inside_its_parent_clip_and_does_not_occlude_outside() {
    let ctx = egui::Context::default();
    let inside = Pos2::new(150.0, 150.0);
    let outside = Pos2::new(90.0, 90.0);
    for _ in 0..3 {
        frame(&ctx, inside, None);
    }
    assert_eq!(ctx.layer_id_at(inside), Some(selection_chrome_layer()));
    assert_eq!(ctx.layer_id_at(outside), Some(egui::LayerId::background()));
    frame(&ctx, inside, Some(true));
    assert!(
        frame(&ctx, inside, Some(false)),
        "the visible control works"
    );
    frame(&ctx, outside, None);
    frame(&ctx, outside, Some(true));
    assert!(
        !frame(&ctx, outside, Some(false)),
        "the clipped part cannot activate"
    );
    assert_eq!(ctx.layer_id_at(outside), Some(egui::LayerId::background()));
}
