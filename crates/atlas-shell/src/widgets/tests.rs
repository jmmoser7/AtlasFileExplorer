use super::{
    format_slider_number, parse_slider_number, rail_gesture, typed_number_event, RailGesture,
    TypedNumber, HANDLE_CLICK_PX,
};

#[test]
fn a_click_on_the_handle_is_not_a_scrub() {
    assert_eq!(rail_gesture(true, 0.0), RailGesture::HandleClick);
    assert_eq!(
        rail_gesture(true, HANDLE_CLICK_PX),
        RailGesture::HandleClick
    );
    assert_eq!(
        rail_gesture(true, HANDLE_CLICK_PX + 0.1),
        RailGesture::Scrub
    );
    assert_eq!(rail_gesture(false, 0.0), RailGesture::Scrub);
}

#[test]
fn typed_slider_values_clamp_and_escape_cancels() {
    let range = 0.0..=100.0;
    assert_eq!(
        typed_number_event("12.5", range.clone(), true, false, false, true),
        TypedNumber::Commit(12.5)
    );
    assert_eq!(
        typed_number_event("150", range.clone(), false, false, true, true),
        TypedNumber::Commit(100.0)
    );
    assert_eq!(
        typed_number_event("-4", range.clone(), true, false, false, true),
        TypedNumber::Commit(0.0)
    );
    assert_eq!(
        typed_number_event("nope", range.clone(), true, false, false, true),
        TypedNumber::Invalid
    );
    assert_eq!(
        typed_number_event("40", range.clone(), false, true, false, true),
        TypedNumber::Cancel
    );
    assert_eq!(
        typed_number_event("40", range.clone(), false, false, false, true),
        TypedNumber::Idle
    );
    assert_eq!(
        typed_number_event("150", 0.0..=100.0, true, false, false, false),
        TypedNumber::Invalid
    );
    assert_eq!(parse_slider_number("33", 0.0..=100.0), Some(33.0));
    assert_eq!(format_slider_number(18.0), "18");
    assert_eq!(format_slider_number(0.5), "0.5");
}

#[test]
fn typed_double_slider_values_keep_the_entered_precision() {
    let entered = 0.123_456_789_123_f64;
    let range = 0.0..=1.0;
    assert_eq!(
        super::parse_slider_number_f64(&entered.to_string(), range.clone()),
        Some(entered)
    );
    assert_eq!(super::parse_slider_number_f64("1.1", range), Some(1.0));
}

use super::{rail_interaction, slider, slider_end_overhang, slider_press_zone};
use eframe::egui::{self, Id, Pos2, Rect, Vec2};

#[test]
fn the_press_zone_overhangs_both_ends_and_keeps_its_height() {
    let hit = Rect::from_min_max(Pos2::new(100.0, 90.0), Pos2::new(300.0, 104.0));
    let zone = slider_press_zone(hit, (102.0..=298.0).into(), 6.0);
    assert_eq!(zone.left(), 96.0);
    assert_eq!(zone.right(), 304.0);
    assert_eq!(zone.y_range(), hit.y_range());
    let wide = hit.expand2(Vec2::new(10.0, 0.0));
    assert_eq!(slider_press_zone(wide, hit.x_range(), 6.0), wide);
    assert_eq!(slider_press_zone(hit, hit.x_range(), -3.0), hit);
    let chrome = slider_end_overhang(1.0);
    assert!(chrome > 0.0);
    assert!((slider_end_overhang(2.0) - chrome * 2.0).abs() < 1e-4);
}

fn frame(ctx: &egui::Context, events: Vec<egui::Event>, mut add: impl FnMut(&mut egui::Ui)) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(480.0, 240.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.interact(
                    ui.max_rect(),
                    Id::new("canvas"),
                    egui::Sense::click_and_drag(),
                );
                add(&mut *ui);
            });
        },
    );
}

fn press(pos: Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        },
    ]
}

fn release(pos: Pos2) -> Vec<egui::Event> {
    vec![egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Default::default(),
    }]
}

#[test]
fn a_press_just_past_either_end_of_a_rail_scrubs_from_that_end() {
    let rail = Rect::from_min_max(Pos2::new(100.0, 90.0), Pos2::new(300.0, 104.0));
    let overhang = slider_end_overhang(1.0);
    let scrub = |at: Pos2| {
        let ctx = egui::Context::default();
        let mut fraction = 0.5_f32;
        let mut run = |events| {
            frame(&ctx, events, |ui| {
                let x = egui::lerp(rail.x_range(), fraction);
                let handle = Rect::from_center_size(Pos2::new(x, 97.0), Vec2::splat(8.0));
                rail_interaction(
                    ui,
                    Id::new("rail"),
                    rail,
                    rail.x_range(),
                    &mut fraction,
                    handle,
                    false,
                    1.0,
                );
            });
        };
        run(Vec::new());
        run(press(at));
        run(release(at));
        fraction
    };
    let y = rail.center().y;
    assert_eq!(scrub(Pos2::new(rail.left() - overhang * 0.5, y)), 0.0);
    assert_eq!(scrub(Pos2::new(rail.right() + overhang * 0.5, y)), 1.0);
    assert_eq!(scrub(Pos2::new(rail.left() - overhang - 2.0, y)), 0.5);
    assert_eq!(scrub(Pos2::new(rail.right() + overhang + 2.0, y)), 0.5);
}

#[test]
fn a_builtin_slider_grabs_a_press_just_past_its_end_over_a_canvas() {
    let drag_from = |dx: f32| {
        let ctx = egui::Context::default();
        let mut value = 50.0_f32;
        let run = |events, value: &mut f32| {
            let mut rail = Rect::NOTHING;
            frame(&ctx, events, |ui| {
                let slider_widget = egui::Slider::new(&mut *value, 0.0..=100.0).show_value(false);
                let id = slider(ui, slider_widget).id;
                rail = ui.ctx().read_response(id).map_or(Rect::NOTHING, |r| r.rect);
            });
            rail
        };
        let rail = run(Vec::new(), &mut value);
        let start = Pos2::new(rail.right() + dx, rail.center().y);
        run(press(start), &mut value);
        let moved = start + Vec2::new(12.0, 0.0);
        run(vec![egui::Event::PointerMoved(moved)], &mut value);
        run(release(moved), &mut value);
        value
    };
    let spacing = egui::Style::default().spacing.item_spacing.x;
    let overhang = slider_end_overhang(1.0).min(spacing - 1.0);
    assert_eq!(drag_from(overhang - 0.5), 100.0);
    assert_eq!(drag_from(overhang + 3.0), 50.0);
}
