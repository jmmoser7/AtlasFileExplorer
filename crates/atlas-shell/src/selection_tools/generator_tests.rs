use super::*;

#[path = "generator_attached_tests.rs"]
mod attached;

#[test]
fn generator_datum_remains_attached_without_overlap_at_each_board_zoom() {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    let chrome = GeneratorChrome::default();
    for width in [160.0, 360.0, 900.0] {
        for settings in [false, true] {
            let settings = settings.then_some("Settings");
            let height = generator_layout(
                &ctx,
                width,
                "Auto",
                settings,
                GeneratorContent::new(1, 0, Some("Preparing…"), None, &chrome),
            )
            .height;
            let unscaled = datum_layout(
                &ctx,
                Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height)).shrink(12.0),
                1.0,
                "Auto",
                settings,
            );
            for zoom in [0.5, 1.0, 2.0] {
                let inner =
                    Rect::from_min_size(Pos2::new(23.0, 31.0), Vec2::new(width, height) * zoom)
                        .shrink(12.0 * zoom);
                let datum = datum_layout(&ctx, inner, zoom, "Auto", settings);
                let controls: Vec<_> = [Some(datum.model), datum.settings, Some(datum.play)]
                    .into_iter()
                    .flatten()
                    .collect();
                for (index, control) in controls.iter().enumerate() {
                    assert!(inner.contains_rect(*control), "width={width}, zoom={zoom}");
                    for other in controls.iter().skip(index + 1) {
                        assert!(!control.intersects(*other));
                    }
                }
                assert!(
                    ((datum.bottom - inner.top()) / zoom - (unscaled.bottom - 12.0)).abs() < 0.001
                );
            }
        }
    }
}

fn play_frame(ctx: &egui::Context, running: bool, events: Vec<egui::Event>) -> bool {
    let mut clicked = false;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(400.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                clicked = generator_play_control(
                    ui,
                    Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::splat(20.0)),
                    running,
                    false,
                    ui.id().with("play-test"),
                    1.0,
                    Palette::dark(),
                );
            });
        },
    );
    clicked
}

#[test]
fn shared_play_target_accepts_stop_without_changing_its_hit_region() {
    let ctx = egui::Context::default();
    let pos = Pos2::new(60.0, 60.0);
    for running in [false, true, false] {
        play_frame(&ctx, running, vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            assert_eq!(
                play_frame(
                    &ctx,
                    running,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }]
                ),
                !pressed
            );
        }
    }
}

#[test]
fn measured_generators_use_datum_history_space_before_adding_a_row() {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    let chrome = GeneratorChrome::default();
    for image in [false, true] {
        let measure = |width, history| {
            let content = GeneratorContent::new(1, history, None, None, &chrome);
            if image {
                image_agent_layout(&ctx, width, content)
            } else {
                generator_layout(&ctx, width, "Codex · default", Some("Settings"), content)
            }
        };
        let empty = measure(900.0, 0);
        let preserved = measure(900.0, 12);
        assert_eq!(
            empty.height, 94.0,
            "12 inset + 24 datum + 6 gap + 40 prompt + 12 inset"
        );
        assert_eq!(
            preserved.height, empty.height,
            "history fits the datum lane"
        );
        assert!(preserved.history.unwrap().bottom() < preserved.prompt.top());
        assert_eq!(preserved.prompt.height(), 40.0);
    }
    let empty = image_agent_layout(
        &ctx,
        384.0,
        GeneratorContent::new(1, 0, None, None, &chrome),
    );
    let preserved = image_agent_layout(
        &ctx,
        384.0,
        GeneratorContent::new(1, 12, None, None, &chrome),
    );
    assert_eq!(
        preserved.height - empty.height,
        32.0,
        "a narrow free lane uses one paged row"
    );
    let single = image_agent_layout(
        &ctx,
        480.0,
        GeneratorContent::new(1, 1, None, None, &chrome),
    );
    let overflow = image_agent_layout(
        &ctx,
        480.0,
        GeneratorContent::new(1, 12, None, None, &chrome),
    );
    assert_eq!(
        overflow.height - single.height,
        32.0,
        "a chip-only lane cannot strand the overflow navigation controls"
    );
    assert!(history::fits(overflow.history.unwrap().width(), 12));
}

#[test]
fn errors_and_approval_stay_visible_with_one_activity_header_at_every_fold() {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    let text = "The public activity describes the generated dashboard. ".repeat(80);
    let entries = [activity::ActivityEntry {
        id: "event",
        label: "Agent",
        text: &text,
    }];
    let description = "Write the generated HTML into the Artifact staging folder.";
    for fold in [
        activity::CardFold::Collapsed,
        activity::CardFold::Partial,
        activity::CardFold::Open,
    ] {
        let mut state = activity::ActivityState {
            fold,
            ..Default::default()
        };
        let chrome = GeneratorChrome {
            activity: Some(activity::ActivityView {
                entries: &entries,
                state: &mut state,
                running: true,
            }),
            ..Default::default()
        };
        let measured = generator_layout(
            &ctx,
            360.0,
            "Codex · default",
            Some("Settings"),
            GeneratorContent::new(
                1,
                0,
                Some("Waiting for your approval…"),
                Some(description),
                &chrome,
            ),
        );
        assert_eq!(measured.summary.unwrap().height(), 24.0);
        let approval = measured.approval.unwrap();
        assert!(approval.top() >= measured.summary.unwrap().bottom() + 6.0);
        assert!(measured.prompt.top() >= approval.bottom() + 6.0);
        assert_eq!(
            measured.activity.map(|rect| rect.height()),
            match fold {
                activity::CardFold::Collapsed => None,
                activity::CardFold::Partial => Some(96.0),
                activity::CardFold::Open => Some(480.0),
            }
        );
        assert_eq!(measured.prompt.height(), 40.0);
    }
}
