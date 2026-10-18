//! Real shared-shell input: menus overlay their transcript, never reflow it.
use super::*;

fn response_rect(f: &Fixture, id: Id) -> Rect {
    super::visual::screen_response(f, id).0
}
fn transcript_geometry(f: &Fixture, out: &egui::FullOutput) -> [Rect; 3] {
    [
        text_rect(out, &f.message_text).expect("actual transcript text"),
        response_rect(f, Id::new(("presence-composer", "dot"))),
        response_rect(f, Id::new(("presence-send", "dot"))),
    ]
}
fn escape(f: &mut Fixture) {
    f.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
}

#[test]
fn real_menu_triggers_overlay_without_moving_messages_or_composer() {
    for (trigger, popover, width) in [
        ("conversation-more", Popover::More, 211.0),
        ("provider-toggle", Popover::Provider, 238.0),
    ] {
        let mut f = Fixture::new();
        let out = f.open_chat("dot");
        let baseline = transcript_geometry(&f, &out);
        let panel = f.state.last_popup_rect;
        let trigger = response_rect(&f, Id::new((trigger, "dot"))).center();
        f.click(trigger);
        let out = f.settle();
        assert_eq!(f.state.popover, Some(popover));
        assert_eq!(transcript_geometry(&f, &out), baseline);
        assert_eq!(f.state.last_popup_rect, panel);
        let menu = f.state.last_popover_rect;
        assert!((menu.width() - width).abs() < 0.1, "compact menu: {menu:?}");
        assert!((menu.right() - (panel.right() - 8.0)).abs() < 0.1);
        assert!(menu.intersects(panel));
        let layer = egui::LayerId::new(
            crate::selection_tools::POPUP_ORDER,
            Id::new(("presence-options", "dot")),
        );
        assert_eq!(f.ctx.layer_id_at(menu.center()), Some(layer));
        // The same trigger closes on its real release, rather than reopening
        // after an outside-press dismissal on the preceding frame.
        f.click(trigger);
        let out = f.settle();
        assert_eq!(f.state.popover, None);
        assert_eq!(f.state.active_chat(), Some("dot"));
        assert_eq!(transcript_geometry(&f, &out), baseline);
        f.click(trigger);
        f.settle();
        f.click(baseline[1].center());
        let out = f.settle();
        assert_eq!(
            f.state.popover, None,
            "composer click dismisses only the menu"
        );
        assert_eq!(f.state.active_chat(), Some("dot"));
        assert_eq!(transcript_geometry(&f, &out), baseline);
        assert_eq!(
            f.ctx.memory(|memory| memory.focused()),
            Some(Id::new(("presence-composer", "dot")))
        );
        f.click(trigger);
        f.settle();
        escape(&mut f);
        assert_eq!(f.state.popover, None);
        assert_eq!(f.state.active_chat(), Some("dot"));
    }
}

#[test]
fn overlay_rows_keep_their_actual_actions_and_current_harness_is_inert() {
    let mut f = Fixture::new();
    f.open_chat("dot");
    f.click(response_rect(&f, Id::new(("conversation-more", "dot"))).center());
    let out = f.settle();
    let full = text_rect(&out, "Full conversation").unwrap();
    assert_eq!(
        f.click(full.center()),
        vec![PresenceAction::ShowConversation { id: "dot".into() }]
    );
    assert_eq!(f.state.popover, None);
    f.settle();
    f.click(response_rect(&f, Id::new(("provider-toggle", "dot"))).center());
    let out = f.settle();
    let current = text_rect(&out, "Codex")
        .filter(|rect| f.state.last_popover_rect.contains(rect.center()))
        .unwrap_or_else(|| {
            out.shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == "Codex"
                            && f.state.last_popover_rect.contains(text.pos) =>
                    {
                        Some(Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .unwrap()
        });
    assert!(f.click(current.center()).is_empty());
    assert_eq!(f.state.popover, Some(Popover::Provider));
    let out = f.settle();
    let alternative = text_rect(&out, "Cursor").unwrap();
    assert_eq!(
        f.click(alternative.center()),
        vec![PresenceAction::SetProvider {
            id: "dot".into(),
            provider: "cursor".into(),
        }]
    );
    assert_eq!(f.state.popover, None);
}

#[test]
fn short_identity_is_intrinsic_and_agent_name_stays_in_chat() {
    let mut f = Fixture::new();
    f.open_chat("dot");
    let mut widths = Vec::new();
    for name in [
        "Dot",
        "Agent",
        "A collaborator with a deliberately long display name",
    ] {
        f.agent_name = name.into();
        f.settle();
        widths.push(response_rect(&f, Id::new(("conversation-profile", "dot"))).width());
    }
    assert!(widths[0] < widths[1] && widths[1] < widths[2]);
    assert!(
        (60.0..80.0).contains(&widths[2]),
        "compact agent identity leaves room for task, hat, harness, More and Close: {widths:?}"
    );
    let controls = [
        Id::new(("dot", "Find on board")),
        Id::new(("dot", "Pause")),
        Id::new(("dot", "Stop task")),
        Id::new(("hat-toggle", "dot")),
        Id::new(("provider-toggle", "dot")),
        Id::new(("conversation-more", "dot")),
        Id::new(("conversation-close", "dot")),
    ]
    .map(|id| response_rect(&f, id));
    for (index, rect) in controls.iter().enumerate() {
        for other in controls.iter().skip(index + 1) {
            assert!(
                !rect.intersects(*other),
                "compact header controls overlap: {controls:?}"
            );
        }
    }
    let name = response_rect(&f, Id::new(("conversation-profile", "dot")));
    f.click(name.center());
    f.settle();
    assert_eq!(f.state.active_chat(), Some("dot"));
    assert_eq!(f.ctx.memory(|memory| memory.focused()), None);
}

#[test]
fn attached_overlay_body_and_hits_keep_the_opener_world_transform_at_every_zoom() {
    for (popover, width, label) in [
        (Popover::More, 211.0, "Full conversation"),
        (Popover::Provider, 238.0, "Cursor"),
    ] {
        let mut reference: Option<[f32; 3]> = None;
        for zoom in [0.25, 0.8, 1.0, 2.0] {
            let mut f = Fixture::new();
            f.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1200.0));
            f.state.open_chat_at("dot", Pos2::new(400.0, 100.0), zoom);
            let out = f.settle();
            let baseline = transcript_geometry(&f, &out);
            // The attached header exposes Close, not a new menu command. Seed
            // the existing popover state to test this same renderer's scaling.
            f.state.popover = Some(popover);
            let out = f.settle();
            assert_eq!(transcript_geometry(&f, &out), baseline);
            let menu = f.state.last_popover_rect;
            assert!(
                (menu.width() / zoom - width).abs() < 0.1,
                "world menu width: {menu:?}"
            );
            let popup_layer = egui::LayerId::new(
                crate::selection_tools::POPUP_ORDER,
                Id::new(("presence-options", "dot")),
            );
            let header_layer = f
                .ctx
                .read_response(Id::new(("conversation-profile", "dot")))
                .unwrap()
                .layer_id;
            assert_eq!(
                f.ctx.layer_transform_to_global(popup_layer),
                f.ctx.layer_transform_to_global(header_layer)
            );
            let body = text_rect(&out, label).expect("the real scaled menu row text");
            assert!(menu.contains_rect(body));
            let measured = [
                menu.height() / zoom,
                body.width() / zoom,
                body.height() / zoom,
            ];
            if let Some(reference) = reference {
                for (actual, expected) in measured.into_iter().zip(reference) {
                    assert!(
                        (actual - expected).abs() < 0.2,
                        "menu body and type scale without floors: {actual} vs {expected}"
                    );
                }
            } else {
                reference = Some(measured);
            }
            f.frame(vec![
                egui::Event::PointerMoved(menu.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(0.0, -1.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            assert!(crate::menu_wheel::wheel_owned(&f.ctx));
            assert!(f.state.captures_pointer(&f.ctx));
            let out = f.settle();
            let row = text_rect(&out, label).unwrap();
            let actions = f.click(row.center());
            let expected = if popover == Popover::More {
                PresenceAction::ShowConversation { id: "dot".into() }
            } else {
                PresenceAction::SetProvider {
                    id: "dot".into(),
                    provider: "cursor".into(),
                }
            };
            assert_eq!(actions, vec![expected], "global menu input at {zoom}");
            assert_eq!(f.state.popover, None);
            f.state.popover = Some(popover);
            f.settle();
            let panel = f.state.last_popup_rect;
            let dismiss = Pos2::new(panel.left() + panel.width() * 0.1, panel.center().y);
            assert!(panel.contains(dismiss));
            assert!(
                !f.state.last_popover_rect.contains(dismiss),
                "dismissal point is outside the scaled popover at {zoom}"
            );
            // The attached menu can cover the composer at some zooms. Click
            // the exposed left side of the actual transformed chat instead.
            f.click(dismiss);
            f.settle();
            assert_eq!(f.state.popover, None);
            assert_eq!(f.state.canvas_chat(), Some("dot"));
            f.state.popover = Some(popover);
            f.settle();
            escape(&mut f);
            assert_eq!(f.state.popover, None);
            assert_eq!(f.state.canvas_chat(), Some("dot"));
        }
    }
}

#[test]
fn long_human_identity_keeps_the_approved_larger_bound_and_clear_profile_target() {
    let mut f = Fixture::new();
    f.human_name = "A human collaborator with a deliberately long display name".into();
    let out = f.open_chat("self");
    let profile = response_rect(&f, Id::new(("conversation-profile", "self")));
    let close = response_rect(&f, Id::new(("conversation-close", "self")));
    let notes = text_rect(&out, "Personal notes").unwrap();
    assert!(
        (profile.width() - 225.0).abs() < 0.1,
        "220px human name plus inset: {profile:?}"
    );
    assert!(profile.right() < notes.left() && notes.right() <= close.left());
    assert!(f.state.last_popup_rect.contains_rect(profile));
    f.click(profile.center());
    f.settle();
    assert_eq!(f.state.expansion, Some(Expansion::Profile("self".into())));
    let edit = Id::new(("presence-name", "self"));
    f.click(response_rect(&f, edit).center());
    assert_eq!(f.ctx.memory(|memory| memory.focused()), Some(edit));
}

#[test]
fn model_dropdown_reuses_shared_selector_above_its_parent_without_mutating_model_data() {
    for attached in [false, true] {
        for zoom in if attached {
            vec![0.25, 0.8, 1.0, 2.0]
        } else {
            vec![1.0]
        } {
            let mut f = Fixture::new();
            f.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1200.0));
            f.model = "model-a".into();
            f.models = vec![
                ProviderOption {
                    id: "model-a",
                    label: "Model A",
                },
                ProviderOption {
                    id: "model-b",
                    label: "Model B",
                },
            ];
            if attached {
                f.state.open_chat_at("dot", Pos2::new(400.0, 100.0), zoom);
            } else {
                f.state.open_chat("dot");
            }
            f.state.popover = Some(Popover::Provider);
            f.settle();
            let id = Id::new(("presence-model", "dot"));
            let trigger = response_rect(&f, id).center();
            f.click(trigger);
            let out = f.settle();
            let child = f.state.last_model_popup_rect;
            let child_layer =
                egui::LayerId::new(crate::selection_tools::POPUP_ORDER, id.with("pop"));
            let parent_layer = egui::LayerId::new(
                crate::selection_tools::POPUP_ORDER,
                Id::new(("presence-options", "dot")),
            );
            assert!(f.state.model_menu_open && child.is_positive());
            assert_eq!(
                f.ctx.layer_transform_to_global(child_layer),
                f.ctx.layer_transform_to_global(parent_layer)
            );
            assert!((child.width() / zoom - 168.0).abs() < 0.1);
            let current = text_rect(&out, "Model A").unwrap();
            assert_eq!(f.ctx.layer_id_at(current.center()), Some(child_layer));
            assert!(
                f.click(current.center()).is_empty(),
                "current model is inert"
            );
            assert!(!f.state.model_menu_open);
            assert_eq!(f.state.popover, Some(Popover::Provider));
            f.settle();
            f.click(response_rect(&f, id).center());
            let out = f.settle();
            let alternative = text_rect(&out, "Model B").unwrap();
            assert_eq!(
                f.click(alternative.center()),
                vec![PresenceAction::SetModel {
                    id: "dot".into(),
                    model: "model-b".into()
                }]
            );
            assert_eq!(
                f.model, "model-a",
                "selection returns an action; only its application changes the model"
            );
            assert_eq!(f.state.popover, Some(Popover::Provider));
            f.settle();
            f.click(response_rect(&f, id).center());
            f.settle();
            let child = f.state.last_model_popup_rect;
            f.frame(vec![
                egui::Event::PointerMoved(child.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: Vec2::new(0.0, -1.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            assert!(crate::menu_wheel::wheel_owned(&f.ctx));
            assert!(f.state.captures_pointer(&f.ctx));
            escape(&mut f);
            assert!(!f.state.model_menu_open);
            assert_eq!(f.state.popover, Some(Popover::Provider));
            f.settle();
            f.click(response_rect(&f, id).center());
            f.settle();
            let parent = f.state.last_popover_rect;
            assert!(f.click(parent.min + Vec2::splat(15.0 * zoom)).is_empty());
            assert!(!f.state.model_menu_open);
            assert_eq!(f.state.popover, Some(Popover::Provider));
            f.settle();
            f.click(response_rect(&f, id).center());
            f.settle();
            assert!(f.state.model_menu_open);
            f.state.open_chat("self");
            assert!(!f.state.model_menu_open && !f.state.last_model_popup_rect.is_positive());
        }
    }
}

// Failure-only observations for the existing registered busy-harness regression.
// The original visible-text matcher and real timing-choice click stay intact.
pub(super) fn timing_choice(f: &Fixture, out: &egui::FullOutput) -> Rect {
    text_rect(out, "After this task").unwrap_or_else(|| {
        let labels: Vec<_> = out.shapes.iter().filter_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                Some((text.galley.text().chars().take(160).collect::<String>(),
                    text.visual_bounding_rect(), shape.clip_rect))
            } else { None }
        }).take(128).collect();
        panic!("explicit timing choice missing: active={:?}, popover={:?}, requested={:?}, menu={:?}, labels={labels:?}",
            f.state.active_chat(), f.state.popover, f.state.requested_provider,
            f.state.last_popover_rect);
    })
}
