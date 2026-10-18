use super::*;

#[test]
fn collapsed_or_empty_activity_reserves_no_space() {
    let ctx = egui::Context::default();
    let entry = [ActivityEntry {
        id: "tool-1",
        label: "Running",
        text: "Writing the dashboard",
    }];
    assert_eq!(height(&ctx, &entry, 300.0, CardFold::Collapsed), 0.0);
    assert_eq!(height(&ctx, &[], 300.0, CardFold::Open), 0.0);
}

#[test]
fn retained_activity_adds_one_summary_row_only_when_lifecycle_status_is_absent() {
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    let entry = [ActivityEntry {
        id: "completed",
        label: "Completed",
        text: "Created the dashboard",
    }];
    let mut state = ActivityState::default();
    let chrome = GeneratorChrome {
        activity: Some(ActivityView {
            entries: &entry,
            state: &mut state,
            running: false,
        }),
        ..Default::default()
    };
    let measure = |status, chrome: &GeneratorChrome<'_>| {
        generator_layout(
            &ctx,
            300.0,
            "Auto",
            None,
            GeneratorContent::new(1, 0, status, None, chrome),
        )
        .height
    };
    let base = measure(None, &GeneratorChrome::default());
    assert_eq!(measure(None, &chrome), base + 30.0);
    assert_eq!(measure(Some("Ready"), &chrome), base + 30.0);
}

fn summary_frame(
    ctx: &egui::Context,
    state: &mut ActivityState,
    events: Vec<egui::Event>,
) -> GeneratorResponse {
    let mut response = GeneratorResponse::default();
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut prompt = String::new();
                let entry = [ActivityEntry {
                    id: "completed",
                    label: "Completed",
                    text: "Created output/index.html",
                }];
                let mut chrome = GeneratorChrome {
                    activity: Some(ActivityView {
                        entries: &entry,
                        state,
                        running: false,
                    }),
                    ..Default::default()
                };
                let measured = generator_layout(
                    ctx,
                    400.0,
                    "Auto",
                    None,
                    GeneratorContent::new(1, 0, None, None, &chrome),
                );
                response = generator_body(
                    ui,
                    measured.at(
                        Rect::from_min_size(
                            Pos2::new(100.0, 58.0),
                            Vec2::new(400.0, measured.height),
                        ),
                        1.0,
                    ),
                    GeneratorBody {
                        prompt: &mut prompt,
                        hint: "Describe the dashboard",
                        take_focus: false,
                        history: &[],
                        selected_history: None,
                        status: None,
                        running: false,
                        approval: None,
                        approval_interactive: false,
                        activity: chrome.activity.take(),
                    },
                    1.0,
                    Palette::dark(),
                    "retained-activity",
                );
            });
        },
    );
    response
}

#[test]
fn completed_activity_can_expand_without_a_lifecycle_status() {
    let ctx = egui::Context::default();
    let mut state = ActivityState::default();
    let point = Pos2::new(150.0, 113.0);
    summary_frame(&ctx, &mut state, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] {
        let response = summary_frame(
            &ctx,
            &mut state,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(
            response.activity_fold,
            (!pressed).then_some(CardFold::Partial)
        );
    }
    assert_eq!(state.fold, CardFold::Partial);
}

#[test]
fn follow_holds_the_readers_position_until_they_return_to_the_bottom() {
    let mut state = FollowScroll::default();
    let requested = requested_offset(&state, 200.0, 80.0);
    assert_eq!(requested, 120.0);
    settle_follow(&mut state, requested, 40.0, 200.0, 80.0);
    assert!(!state.follow);
    assert_eq!(requested_offset(&state, 300.0, 80.0), 40.0);
    settle_follow(&mut state, 40.0, 220.0, 300.0, 80.0);
    assert!(state.follow);
    assert_eq!(state.offset, 220.0);
}

#[test]
fn selectable_public_transcript_keeps_its_world_wrapping_across_zoom() {
    let ctx = egui::Context::default();
    let mut heights = Vec::new();
    for zoom in [0.45, 0.525, 0.9, 1.0, 1.25, 2.0] {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(20.0, 20.0), Vec2::new(240.0, 300.0) * zoom);
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                let row = transcript_row(&mut child, Id::new("public-transcript"), "Preparing the reference image, then writing an interactive gradient with editable domain and title.", TEXT_UNITS, zoom, Palette::dark().ink, true, false);
                heights.push(row.height() / zoom);
            });
        });
    }
    for height in &heights {
        assert!((height - heights[0]).abs() < 0.03, "{heights:?}");
    }
}

fn fold_frame(
    ctx: &egui::Context,
    state: &mut ActivityState,
    events: Vec<egui::Event>,
) -> Option<CardFold> {
    let mut chosen = None;
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::new(400.0, 24.0));
                chosen = paint_fold_controls(ui, rect, state.fold, 1.0, Palette::dark());
                if let Some(fold) = chosen {
                    state.fold = fold;
                }
            });
        },
    );
    chosen
}

#[test]
fn activity_chevrons_dispatch_the_shared_fold_states() {
    let ctx = egui::Context::default();
    let mut state = ActivityState {
        fold: CardFold::Partial,
        ..Default::default()
    };
    let point = Pos2::new(488.0, 112.0);
    fold_frame(&ctx, &mut state, vec![egui::Event::PointerMoved(point)]);
    for pressed in [true, false] {
        let choice = fold_frame(
            &ctx,
            &mut state,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert_eq!(choice, (!pressed).then_some(CardFold::Collapsed));
    }
    assert_eq!(state.fold, CardFold::Collapsed);
}
