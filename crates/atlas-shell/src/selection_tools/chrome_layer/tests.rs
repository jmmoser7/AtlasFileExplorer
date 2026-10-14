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

fn tooltip_frame(ctx: &egui::Context, pressed: Option<bool>, covered: bool) -> bool {
    let rect = Rect::from_min_size(Pos2::new(723.5, 498.5), Vec2::splat(30.0));
    let point = rect.center();
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
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
            time: Some(ctx.input(|i| i.time) + 1.0 / 60.0),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut controls = object_ui(ui, Id::new("tooltip-strip"), rect);
                let original = ctx.style();
                clicked = super::super::button(
                    &mut controls,
                    rect,
                    Id::new("native-text-label"),
                    "Text",
                    Icon::Text,
                    false,
                    1.0,
                    Palette::dark(),
                    1.0,
                    true,
                )
                .clicked();
                assert!(
                    std::sync::Arc::ptr_eq(&original, &ctx.style()),
                    "restore exact style"
                );
            });
            if covered {
                popup_area(ctx, Id::new("cover-native-label"), selection_chrome_layer())
                    .fixed_pos(rect.min)
                    .constrain(false)
                    .fade_in(false)
                    .show(ctx, |ui| {
                        ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
                    });
            }
        },
    );
    clicked
}

#[test]
fn a_first_native_text_tooltip_preserves_the_immediate_real_click() {
    let ctx = egui::Context::default();
    ctx.style_mut(|style| style.interaction.tooltip_delay = 0.0);
    // Establish the widget rectangles, then exercise the first native tooltip.
    tooltip_frame(&ctx, None, false);
    tooltip_frame(&ctx, None, false);
    let tooltip =
        egui::containers::AreaState::load(&ctx, Id::new("native-text-label").with(0usize))
            .expect("the actual native label tooltip appeared");
    let button = Rect::from_min_size(Pos2::new(723.5, 498.5), Vec2::splat(30.0));
    assert!(
        !tooltip.rect().intersects(button),
        "native tooltip avoids its button"
    );
    tooltip_frame(&ctx, Some(true), false);
    assert!(
        tooltip_frame(&ctx, Some(false), false),
        "genuine response.clicked wins"
    );
}

#[test]
fn a_real_popup_still_owns_the_click_above_native_label_chrome() {
    let ctx = egui::Context::default();
    ctx.style_mut(|style| style.interaction.tooltip_delay = 0.0);
    for _ in 0..3 {
        tooltip_frame(&ctx, None, true);
    }
    tooltip_frame(&ctx, Some(true), true);
    assert!(
        !tooltip_frame(&ctx, Some(false), true),
        "covering popup owns press/release"
    );
}
