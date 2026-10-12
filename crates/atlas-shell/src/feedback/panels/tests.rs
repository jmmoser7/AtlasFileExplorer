use super::*;

#[test]
fn every_feedback_window_centers_on_the_canvas_even_after_reopening_and_resize() {
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
            for _ in 0..5 {
                let _ = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1100.0, 850.0),
                        )),
                        ..Default::default()
                    },
                    |ctx| {
                        show(ctx, &Palette::light(), &mut hub, "center-test", "0");
                    },
                );
            }
            let id = Id::new((
                "feedback-dialog",
                "center-test",
                phase == FeedbackPhase::KindPicker,
                kind == Some(FeedbackKind::Bug),
            ));
            let actual = ctx
                .memory(|m| m.area_rect(id))
                .expect("visible feedback window");
            assert!(
                (actual.center() - canvas.center()).length() <= 1.5,
                "{phase:?}: {actual:?} vs {canvas:?}"
            );
            assert!(
                canvas.contains_rect(actual),
                "{actual:?} escapes {canvas:?}"
            );
            if phase == FeedbackPhase::KindPicker {
                assert!(actual.height() < 380.0, "chooser should fit its content");
            } else if canvas.height() >= 700.0 {
                assert!(
                    actual.height() > 500.0,
                    "form must use available canvas height"
                );
            }
            hub.phase = FeedbackPhase::Closed;
            let _ = ctx.run(Default::default(), |ctx| {
                show(ctx, &Palette::light(), &mut hub, "center-test", "0");
            });
        }
    }
}
