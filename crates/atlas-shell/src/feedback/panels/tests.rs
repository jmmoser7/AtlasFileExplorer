use super::*;

fn frame(ctx: &egui::Context, hub: &mut FeedbackHub, events: Vec<egui::Event>) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1100.0, 850.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            show(ctx, &Palette::light(), hub, "window-test", "0");
        },
    );
}

fn window_rect(ctx: &egui::Context, hub: &FeedbackHub) -> Rect {
    ctx.memory(|m| {
        m.area_rect(Id::new((
            "feedback-dialog",
            "window-test",
            hub.phase == FeedbackPhase::KindPicker,
            hub.kind == Some(FeedbackKind::Bug),
        )))
    })
    .expect("visible feedback window")
}

#[test]
fn feedback_forms_keep_their_body_after_reopening_and_canvas_resize() {
    let ctx = egui::Context::default();
    let mut hub = FeedbackHub::default();
    for (phase, kind) in [
        (FeedbackPhase::KindPicker, None),
        (FeedbackPhase::BugForm, Some(FeedbackKind::Bug)),
        (FeedbackPhase::FeatureForm, Some(FeedbackKind::Feature)),
    ] {
        for canvas in [
            Rect::from_min_max(egui::pos2(90.0, 70.0), egui::pos2(970.0, 770.0)),
            Rect::from_min_max(egui::pos2(220.0, 110.0), egui::pos2(890.0, 690.0)),
        ] {
            hub.phase = phase;
            hub.kind = kind;
            hub.canvas_rect = Some(canvas);
            for _ in 0..8 {
                frame(&ctx, &mut hub, vec![]);
            }
            let actual = window_rect(&ctx, &hub);
            assert!(
                actual.height() >= 240.0,
                "{phase:?} collapsed to its header: {actual:?}"
            );
            assert!(
                canvas.expand(2.0).contains_rect(actual),
                "{actual:?} escapes {canvas:?}"
            );
            hub.phase = FeedbackPhase::Closed;
            frame(&ctx, &mut hub, vec![]);
        }
    }
}

#[test]
fn feedback_window_can_be_dragged_and_remembers_the_position() {
    let ctx = egui::Context::default();
    let mut hub = FeedbackHub::default();
    hub.begin_form(FeedbackKind::Bug);
    for _ in 0..8 {
        frame(&ctx, &mut hub, vec![]);
    }
    let before = window_rect(&ctx, &hub);
    let origin = before.min + Vec2::new(before.width() * 0.65, 12.0);
    let dest = origin + Vec2::new(80.0, 40.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&ctx, &mut hub, vec![egui::Event::PointerMoved(origin)]);
    frame(&ctx, &mut hub, vec![button(origin, true)]);
    frame(&ctx, &mut hub, vec![egui::Event::PointerMoved(dest)]);
    frame(&ctx, &mut hub, vec![button(dest, false)]);
    for _ in 0..3 {
        frame(&ctx, &mut hub, vec![]);
    }
    let moved = window_rect(&ctx, &hub);
    assert!(
        moved.min.distance(before.min) > 20.0,
        "header drag did not move {before:?}"
    );
    hub.phase = FeedbackPhase::Closed;
    frame(&ctx, &mut hub, vec![]);
    hub.begin_form(FeedbackKind::Bug);
    for _ in 0..5 {
        frame(&ctx, &mut hub, vec![]);
    }
    assert!(window_rect(&ctx, &hub).min.distance(moved.min) < 2.0);
}
