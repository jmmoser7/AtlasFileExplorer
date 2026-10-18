use super::*;

#[test]
fn adapter_history_ranges_share_the_canonical_pagination_and_attached_limit() {
    let mut state = RosterState::default();
    state.open_chat("dot");
    assert_eq!(state.message_range("dot", 100_000), 99_936..100_000);
    state.history_offsets.insert("dot".into(), 64);
    assert_eq!(state.message_range("dot", 100_000), 99_872..99_936);
    state.open_chat_at("dot", Pos2::new(400.0, 300.0), 1.0);
    assert_eq!(state.message_range("dot", 100_000), 99_996..100_000);
    assert_eq!(state.message_range("dot", 0), 0..0);
}

#[test]
fn busy_chat_keeps_the_composer_live_and_shows_only_public_progress() {
    let mut f = Fixture::new();
    f.busy = true;
    f.activity = vec![("Placing image".into(), "North frame".into())];
    f.state.edits.entry("dot".into()).or_default().message = "Keep going".into();
    let out = f.open_chat("dot");
    assert!(text_rect(&out, "Placing image · North frame").is_some());
    let send = visual::screen_response(&f, Id::new(("presence-send", "dot")))
        .0
        .center();
    assert!(f.state.last_popup_rect.contains(send));
    assert!(f.click(send).contains(&PresenceAction::Send {
        id: "dot".into(),
        text: "Keep going".into(),
    }));
}

#[test]
fn composer_exposes_attachment_picker_and_removable_local_chips() {
    let mut f = Fixture::new();
    f.attachments = vec!["reference.png".into()];
    f.message_attachments = 2;
    f.drafts.insert("dot".into(), String::new());
    let out = f.open_chat("dot");
    assert!(text_rect(&out, "reference.png").is_some());
    assert!(text_rect(&out, "2 images attached").is_some());
    let attach = visual::screen_response(&f, Id::new(("presence-attach", "dot")))
        .0
        .center();
    assert!(f
        .click(attach)
        .contains(&PresenceAction::Attach { id: "dot".into() }));
    let send = visual::screen_response(&f, Id::new(("presence-send", "dot")))
        .0
        .center();
    assert!(f.click(send).contains(&PresenceAction::Send {
        id: "dot".into(),
        text: String::new(),
    }));
    let remove = visual::screen_response(&f, Id::new(("presence-remove-attachment", "dot", 0)))
        .0
        .center();
    assert!(f.click(remove).contains(&PresenceAction::RemoveAttachment {
        id: "dot".into(),
        index: 0,
    }));
}

#[test]
fn new_agent_dialog_only_picks_a_harness_then_emits_the_existing_add_action() {
    let mut f = Fixture::new();
    f.state.toggle(
        Expansion::Add,
        Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::splat(34.0)),
    );
    let out = f.settle();
    assert!(text_rect(&out, "Add an agent").is_some());
    assert!(text_rect(&out, "Codex").is_some());
    assert!(text_rect(&out, "Cursor").is_some());
    assert!(text_rect(&out, "Dot").is_some());
    assert!(text_rect(&out, "Name").is_none());
    assert!(text_rect(&out, "Add and open chat").is_none());
    assert!(text_rect(&out, "Set AI workspace…").is_none());
    let cursor = text_rect(&out, "Cursor").unwrap();
    assert_eq!(
        f.click(cursor.center()),
        vec![PresenceAction::Add {
            name: "Cursor".into(),
            hat: "cap".into(),
            provider: "cursor".into(),
        }]
    );
    assert!(!f.state.is_open());
}

#[test]
fn workspace_setup_affordance_only_appears_when_agent_add_is_blocked() {
    let mut f = Fixture::new();
    f.state.set_workspace_ready(false);
    f.state.toggle(
        Expansion::Add,
        Rect::from_min_size(Pos2::new(100.0, 100.0), Vec2::splat(34.0)),
    );
    let out = f.settle();
    let setup =
        text_rect(&out, "Set AI workspace…").expect("blocked picker explains how to continue");
    assert_eq!(
        f.click(setup.center()),
        vec![PresenceAction::SetupWorkspace]
    );
}

#[test]
fn codex_full_access_is_a_provider_grant_separate_from_board_access() {
    let mut f = Fixture::new();
    f.open_chat("dot");
    f.click(
        visual::screen_response(&f, Id::new(("conversation-more", "dot")))
            .0
            .center(),
    );
    let menu = f.settle();
    let settings = text_rect(&menu, "Agent settings").unwrap();
    f.click(settings.center());
    let out = f.settle();
    let checkbox = text_rect(&out, "Codex full access")
        .expect("independent Codex provider control is available");
    assert!(text_rect(
        &out,
        "Provider tools can run commands and write files without approval; board access remains scoped."
    )
    .is_some());
    assert!(f
        .click(checkbox.center())
        .contains(&PresenceAction::SetFullAccess {
            id: "dot".into(),
            enabled: true,
        }));
}

#[test]
fn full_access_control_is_hidden_for_other_harnesses_and_helpers() {
    for (provider, parent) in [("cursor", None), ("codex", Some("Codex"))] {
        let mut f = Fixture::new();
        f.agent_provider = provider.into();
        f.parent_name = parent.map(str::to_owned);
        f.open_chat("dot");
        f.click(
            visual::screen_response(&f, Id::new(("conversation-more", "dot")))
                .0
                .center(),
        );
        let menu = f.settle();
        f.click(text_rect(&menu, "Agent settings").unwrap().center());
        let out = f.settle();
        assert!(text_rect(&out, "Codex full access").is_none());
    }
}

#[test]
fn cached_avatar_layers_inherit_the_attached_conversation_transform() {
    let ctx = egui::Context::default();
    let mut painter = PresencePainter::default();
    let pan = Vec2::new(250.0, 180.0);
    let zoom = 2.0;
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(1000.0))),
            ..Default::default()
        },
        |ctx| {
            let layer =
                egui::LayerId::new(egui::Order::Foreground, Id::new("attached-conversation"));
            ctx.set_transform_layer(layer, egui::emath::TSTransform::new(pan, zoom));
            painter.paint_world_avatar(
                &ctx.layer_painter(layer),
                AvatarVisual {
                    id: "dot",
                    center: Pos2::new(50.0, 50.0),
                    expression: "neutral",
                    orientation: "forward",
                    hat: "cap",
                    prop: "none",
                    opacity: 1.0,
                },
                0.0,
                true,
                1.0,
            );
        },
    );
    let bounds = ctx
        .tessellate(output.shapes, output.pixels_per_point)
        .iter()
        .fold(Rect::NOTHING, |mut bounds, primitive| {
            if let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive {
                for vertex in &mesh.vertices {
                    if vertex.color.a() > 0 {
                        bounds.extend_with(vertex.pos);
                    }
                }
            }
            bounds
        });
    assert!(
        bounds.left() > 300.0 && bounds.top() > 220.0,
        "cached head and hat must move and scale with the chat parent: {bounds:?}"
    );
}
