//! Exercise the production empty form against a sibling property popup.
use super::*;

fn label(output: &egui::FullOutput, wanted: &str) -> egui::Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == wanted => {
                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {wanted}"))
}

#[test]
fn credential_modal_owns_overlap_cancel_outside_escape_and_empty_enter() {
    for exit in ["Cancel", "outside", "Escape", "Enter", "ready Enter"] {
        let ctx = egui::Context::default();
        let id = egui::Id::new("credential-overlap");
        let modal_layer = egui::LayerId::new(atlas_shell::selection_tools::POPUP_ORDER, id);
        let ready = exit == "ready Enter";
        let mut draft = if ready {
            "SYNTHETIC_NOT_A_KEY".into()
        } else {
            String::new()
        };
        let mut action = CredentialAction::None;
        let mut underlying_clicks = 0;
        let mut at = egui::pos2(20.0, 20.0);
        let mut paint = |events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    action = credential_key_dialog(ctx, id, "OpenAI API", false, &mut draft);
                    // This is the production ordering: modal first, Agent popup later.
                    egui::Area::new(egui::Id::new("underlying-agent"))
                        .order(atlas_shell::selection_tools::POPUP_ORDER)
                        .fixed_pos(egui::Pos2::ZERO)
                        .fade_in(false)
                        .show(ctx, |ui| {
                            ui.set_min_size(egui::vec2(1440.0, 900.0));
                            if ui
                                .put(
                                    egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(1440.0, 900.0),
                                    ),
                                    egui::Button::new("Underlying Agent action"),
                                )
                                .clicked()
                            {
                                underlying_clicks += 1;
                            }
                        });
                },
            )
        };
        let mut output = paint(vec![]);
        for _ in 0..3 {
            output = paint(vec![]);
        }
        if exit == "Cancel" {
            at = label(&output, "Cancel").center();
        }
        assert_eq!(ctx.layer_id_at(at), Some(modal_layer));
        if matches!(exit, "Escape" | "Enter" | "ready Enter") {
            let key = if exit == "Escape" {
                egui::Key::Escape
            } else {
                egui::Key::Enter
            };
            paint(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        } else {
            paint(vec![egui::Event::PointerMoved(at)]);
            for pressed in [true, false] {
                paint(vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }]);
            }
        }
        drop(paint);
        assert_eq!(
            underlying_clicks, 0,
            "{exit} reached the sibling Agent popup"
        );
        assert_eq!(draft.is_empty(), !ready);
        assert!(
            if ready {
                matches!(action, CredentialAction::Save)
            } else if exit == "Enter" {
                matches!(action, CredentialAction::None)
            } else {
                matches!(action, CredentialAction::Cancel)
            },
            "{exit}"
        );
        if exit == "Escape" {
            assert!(
                !ctx.input(|input| input.key_pressed(egui::Key::Escape)),
                "modal consumes Escape before the field and board"
            );
        }
    }
}

#[test]
fn existing_credential_modal_stays_below_a_nested_modal_until_it_closes() {
    for nonempty in [false, true] {
        struct Dialogs {
            credential: bool,
            upper: bool,
            upper_two: bool,
            draft: String,
            upper_text: String,
            upper_clicks: usize,
            focus_upper: bool,
        }
        let ctx = egui::Context::default();
        let lower = egui::Id::new("nested-credential");
        let upper = egui::Id::new("nested-upper");
        let upper_field = upper.with("field");
        let upper_two = egui::Id::new("nested-upper-two");
        let upper_two_layer =
            egui::LayerId::new(atlas_shell::selection_tools::POPUP_ORDER, upper_two);
        let upper_layer = egui::LayerId::new(atlas_shell::selection_tools::POPUP_ORDER, upper);
        let mut dialogs = Dialogs {
            credential: true,
            upper: false,
            upper_two: false,
            draft: String::new(),
            upper_text: String::new(),
            upper_clicks: 0,
            focus_upper: true,
        };
        let paint = |dialogs: &mut Dialogs, events: Vec<egui::Event>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    if dialogs.credential {
                        let action = credential_key_dialog(
                            ctx,
                            lower,
                            "OpenAI API",
                            false,
                            &mut dialogs.draft,
                        );
                        assert!(!matches!(action, CredentialAction::Save));
                        if matches!(action, CredentialAction::Cancel) {
                            dialogs.credential = false;
                        }
                    }
                    if dialogs.upper {
                        let upper_escape = ctx
                            .memory(|memory| memory.top_modal_layer() == Some(upper_layer))
                            && ctx.input_mut(|input| {
                                input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            });
                        let response = egui::Modal::new(upper)
                            .area(
                                egui::Modal::default_area(upper)
                                    .order(atlas_shell::selection_tools::POPUP_ORDER),
                            )
                            .show(ctx, |ui| {
                                ui.set_min_size(egui::vec2(520.0, 380.0));
                                ui.label("Upper modal");
                                let field = ui.add(
                                    egui::TextEdit::singleline(&mut dialogs.upper_text)
                                        .id(upper_field),
                                );
                                if dialogs.focus_upper {
                                    field.request_focus();
                                    dialogs.focus_upper = false;
                                }
                                if ui.button("Upper action").clicked() {
                                    dialogs.upper_clicks += 1;
                                }
                            });
                        if response.should_close() || upper_escape {
                            dialogs.upper = false;
                        }
                    }
                    if dialogs.upper_two {
                        let response = egui::Modal::new(upper_two)
                            .area(egui::Modal::default_area(upper_two).order(upper_two_layer.order))
                            .show(ctx, |ui| {
                                ui.set_min_size(egui::vec2(600.0, 440.0));
                                ui.label("Second upper modal");
                            });
                        if response.should_close() {
                            dialogs.upper_two = false;
                        }
                    }
                },
            )
        };
        let mut output = paint(&mut dialogs, vec![]);
        for _ in 0..4 {
            output = paint(&mut dialogs, vec![]);
        }
        let lower_cancel = label(&output, "Cancel").center();
        let lower_focus = ctx
            .memory(|memory| memory.focused())
            .expect("real credential field took focus");
        if nonempty {
            dialogs.draft = "SYNTHETIC_NOT_A_KEY".into();
        }
        dialogs.upper = true;
        dialogs.focus_upper = false;
        paint(&mut dialogs, vec![]);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(lower_focus));
        let immediate_escape = egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut admission_events = Vec::new();
        if nonempty {
            admission_events.push(egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        admission_events.push(immediate_escape);
        paint(&mut dialogs, admission_events);
        if nonempty {
            assert!(
                dialogs.credential && dialogs.upper,
                "nested Enter must not cancel the lower form"
            );
            assert_eq!(
                dialogs.draft, "SYNTHETIC_NOT_A_KEY",
                "nested Enter must not save or clear the lower draft"
            );
            dialogs.draft.clear();
        }
        assert!(
            dialogs.credential && dialogs.upper,
            "admission-pass Escape must not dismiss the focused lower credential"
        );
        assert!(
            ctx.input(|input| input.key_pressed(egui::Key::Escape)),
            "lower field and should_close must leave deferred upper Escape unconsumed"
        );
        paint(
            &mut dialogs,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        dialogs.focus_upper = true;
        for _ in 0..5 {
            output = paint(&mut dialogs, vec![]);
            assert!(dialogs.credential && dialogs.upper);
        }
        for _ in 0..3 {
            output = paint(&mut dialogs, vec![]);
            assert_eq!(
                ctx.memory(|memory| memory.top_modal_layer()),
                Some(upper_layer)
            );
            assert_eq!(
                ctx.layer_id_at(lower_cancel),
                Some(upper_layer),
                "established upper modal owns the lower Cancel position"
            );
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(upper_field));
        }
        paint(&mut dialogs, vec![egui::Event::Text("upper only".into())]);
        assert_eq!(dialogs.upper_text, "upper only");
        assert!(dialogs.draft.is_empty());
        for pressed in [true, false] {
            paint(
                &mut dialogs,
                vec![
                    egui::Event::PointerMoved(lower_cancel),
                    egui::Event::PointerButton {
                        pos: lower_cancel,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(
            dialogs.credential && dialogs.upper,
            "clicking the covered credential Cancel reaches neither lower control nor upper backdrop"
        );
        let action_at = label(&output, "Upper action").center();
        for pressed in [true, false] {
            paint(
                &mut dialogs,
                vec![
                    egui::Event::PointerMoved(action_at),
                    egui::Event::PointerButton {
                        pos: action_at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(dialogs.upper_clicks, 1);
        let escape = |pressed| egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        dialogs.upper_two = true;
        for _ in 0..4 {
            paint(&mut dialogs, vec![]);
        }
        assert_eq!(
            ctx.memory(|memory| memory.top_modal_layer()),
            Some(upper_two_layer)
        );
        assert_eq!(ctx.layer_id_at(lower_cancel), Some(upper_two_layer));
        paint(&mut dialogs, vec![escape(true)]);
        assert!(
            !dialogs.upper_two && dialogs.upper && dialogs.credential,
            "Escape closes only the second upper modal"
        );
        paint(&mut dialogs, vec![escape(false)]);
        for _ in 0..3 {
            paint(&mut dialogs, vec![]);
        }
        assert_eq!(
            ctx.memory(|memory| memory.top_modal_layer()),
            Some(upper_layer)
        );
        paint(&mut dialogs, vec![escape(true)]);
        assert!(
            !dialogs.upper && dialogs.credential,
            "first Escape belongs to the canonical upper modal"
        );
        paint(&mut dialogs, vec![escape(false)]);
        for _ in 0..3 {
            paint(&mut dialogs, vec![]);
        }
        paint(&mut dialogs, vec![escape(true)]);
        assert!(
            !dialogs.credential,
            "credential receives its later independent Escape"
        );
        assert!(dialogs.draft.is_empty());
    }
}

#[test]
fn credential_focus_waits_for_eligible_pass_and_resets_after_skips_and_reopen() {
    let ctx = egui::Context::default();
    let id = egui::Id::new("credential-reopen");
    let mut draft = String::new();
    let paint = |show: bool, draft: &mut String, events: Vec<egui::Event>| {
        let mut action = CredentialAction::None;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                if show {
                    action = credential_key_dialog(ctx, id, "OpenAI API", false, draft);
                }
            },
        );
        (action, output)
    };
    let (action, _) = paint(true, &mut draft, vec![]);
    assert!(matches!(action, CredentialAction::None));
    assert!(!ctx
        .data(|data| data.get_temp::<bool>(id.with("focus")))
        .unwrap_or(false));
    let (_, output) = paint(true, &mut draft, vec![]);
    assert!(
        ctx.memory(|memory| memory.focused()).is_some(),
        "first eligible visible pass focuses the real field"
    );
    let at = label(&output, "Cancel").center();
    paint(true, &mut draft, vec![egui::Event::PointerMoved(at)]);
    paint(
        true,
        &mut draft,
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let (action, _) = paint(
        true,
        &mut draft,
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(
        matches!(action, CredentialAction::Cancel),
        "first real click closes the visible empty form"
    );
    assert!(ctx
        .data(|data| data.get_temp::<CredentialTopSample>(id.with("ordered-top")))
        .is_none());
    assert!(ctx
        .data(|data| data.get_temp::<bool>(id.with("focus")))
        .is_none());
    for _ in 0..3 {
        paint(false, &mut draft, vec![]);
    }
    paint(true, &mut draft, vec![]);
    assert!(
        ctx.memory(|memory| memory.focused()).is_none(),
        "invisible retained area IDs do not make a reopened form eligible"
    );
    paint(true, &mut draft, vec![]);
    assert!(ctx.memory(|memory| memory.focused()).is_some());
    // Omit the form without Save/Cancel, as a caller can on a tab switch.
    for _ in 0..3 {
        paint(false, &mut draft, vec![]);
    }
    paint(true, &mut draft, vec![]);
    assert!(
        !ctx.data(|data| data.get_temp::<bool>(id.with("focus")))
            .unwrap_or(false),
        "skipped passes invalidate the old focus mark"
    );
    assert!(ctx.memory(|memory| memory.focused()).is_none());
    paint(true, &mut draft, vec![]);
    assert!(
        ctx.memory(|memory| memory.focused()).is_some(),
        "eligible ownership restores interaction after a skipped pass"
    );
    let escape = egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let (action, _) = paint(true, &mut draft, vec![escape]);
    assert!(matches!(action, CredentialAction::Cancel));
    assert!(draft.is_empty());
    assert!(ctx
        .data(|data| data.get_temp::<CredentialTopSample>(id.with("ordered-top")))
        .is_none());
}
