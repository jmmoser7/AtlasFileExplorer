//! Real egui gestures through the shared native corner; backend work is outside
//! this fixture. Draft actions are acknowledged as the app's durable store does.
use super::super::{AvatarVisual, ConversationMessage};
use super::*;
use std::collections::HashMap;

struct Fixture {
    ctx: egui::Context,
    state: RosterState,
    painter: PresencePainter,
    time: f64,
    drafts: HashMap<String, String>,
    busy: bool,
    full_access: bool,
    inactive: bool,
    captured: bool,
    status: String,
    agent_name: String,
    agent_provider: String,
    parent_name: Option<String>,
    human_name: String,
    model: String,
    models: Vec<ProviderOption<'static>>,
    message_text: String,
    message_outgoing: bool,
    screen: Rect,
    proposals: Vec<(String, String, bool)>,
    applying_proposal: Option<String>,
    activity: Vec<(String, String)>,
    attachments: Vec<String>,
    message_attachments: usize,
}
impl Fixture {
    fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            state: RosterState::default(),
            painter: PresencePainter::default(),
            time: 0.0,
            drafts: HashMap::new(),
            busy: false,
            full_access: false,
            inactive: false,
            captured: false,
            status: "Ready in proposal mode".into(),
            agent_name: "Dot".into(),
            agent_provider: "codex".into(),
            parent_name: None,
            human_name: "You".into(),
            model: String::new(),
            models: Vec::new(),
            message_text: "I am arranging the images in this frame.".into(),
            message_outgoing: false,
            screen: Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0)),
            proposals: Vec::new(),
            applying_proposal: None,
            activity: Vec::new(),
            attachments: Vec::new(),
            message_attachments: 0,
        }
    }
    fn frame(&mut self, events: Vec<egui::Event>) -> (Vec<PresenceAction>, egui::FullOutput) {
        self.time += 1.0 / 60.0;
        let ctx = self.ctx.clone();
        let palette = Palette::light();
        let mut actions = vec![];
        let messages = [ConversationMessage {
            id: "reply",
            text: &self.message_text,
            at: 1,
            outgoing: self.message_outgoing,
            system: false,
            status: "",
            attachment_count: self.message_attachments,
        }];
        let empty = String::new();
        let activity: Vec<_> = self
            .activity
            .iter()
            .enumerate()
            .map(|(index, (label, text))| super::super::PublicActivity {
                label,
                text,
                at: index as u64 + 2,
            })
            .collect();
        let attachments: Vec<_> = self
            .attachments
            .iter()
            .map(|name| super::super::Attachment { name })
            .collect();
        let proposals: Vec<_> = self
            .proposals
            .iter()
            .map(
                |(id, label, recovery_required)| super::super::ProposalDecision {
                    id,
                    label,
                    recovery_required: *recovery_required,
                    applying: self.applying_proposal.as_deref() == Some(id.as_str()),
                },
            )
            .collect();
        let participants = [
            Participant {
                id: "self",
                name: &self.human_name,
                hat: "none",
                status: "Human participant",
                provider: "",
                human: true,
                inactive: false,
                paused: false,
                autonomous: false,
                full_access: false,
                file_inputs: false,
                own_chats: false,
                parent_name: None,
                chats: &[],
                local_profile: true,
                photo_texture: None,
                busy: false,
                messages: &[],
                copy_messages: None,
                activity: &activity,
                attachments: &attachments,
                message_total: 0,
                proposals: &[],
                draft: self.drafts.get("self").unwrap_or(&empty),
                queued_provider: None,
                model: "",
                models: &[],
            },
            Participant {
                id: "dot",
                name: &self.agent_name,
                hat: "cap",
                status: &self.status,
                provider: &self.agent_provider,
                human: false,
                inactive: self.inactive,
                paused: false,
                autonomous: false,
                full_access: self.full_access,
                file_inputs: false,
                own_chats: false,
                parent_name: self.parent_name.as_deref(),
                chats: &[],
                local_profile: false,
                photo_texture: None,
                busy: self.busy,
                messages: &messages,
                copy_messages: None,
                activity: &activity,
                attachments: &attachments,
                message_total: messages.len(),
                proposals: &proposals,
                draft: self.drafts.get("dot").unwrap_or(&empty),
                queued_provider: None,
                model: &self.model,
                models: &self.models,
            },
        ];
        let providers = [
            ProviderOption {
                id: "codex",
                label: "Codex",
            },
            ProviderOption {
                id: "cursor",
                label: "Cursor",
            },
            ProviderOption {
                id: "dot",
                label: "Dot",
            },
        ];
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(self.screen),
                time: Some(self.time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::Area::new(Id::new("presence-acceptance"))
                    .order(egui::Order::Foreground)
                    .pivot(Align2::RIGHT_TOP)
                    .fixed_pos(Pos2::new(self.screen.right() - 12.0, 20.0))
                    .show(ctx, |ui| {
                        ui.set_max_width(620.0);
                        actions = participant_capsules(
                            ui,
                            &palette,
                            &mut self.painter,
                            &mut self.state,
                            &participants,
                            &providers,
                            self.time,
                            false,
                        );
                        self.captured = self.state.captures_pointer(ctx);
                    });
            },
        );
        for action in &actions {
            if let PresenceAction::Draft { id, text } = action {
                self.drafts.insert(id.clone(), text.clone());
            }
        }
        (actions, output)
    }
    fn settle(&mut self) -> egui::FullOutput {
        let mut out = self.frame(vec![]).1;
        for _ in 0..8 {
            out = self.frame(vec![]).1;
        }
        out
    }
    fn click(&mut self, p: Pos2) -> Vec<PresenceAction> {
        self.frame(pointer(p, true));
        self.frame(pointer(p, false)).0
    }
    fn open_chat(&mut self, id: &str) -> egui::FullOutput {
        self.state.open_chat(id);
        self.settle()
    }
}
fn pointer(pos: Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}
fn text_rect(output: &egui::FullOutput, text: &str) -> Option<Rect> {
    output.shapes.iter().find_map(|shape| {
        if let egui::Shape::Text(t) = &shape.shape {
            let rect = Rect::from_min_size(t.pos, t.galley.size());
            (t.galley.text() == text && shape.clip_rect.contains(rect.center())).then_some(rect)
        } else {
            None
        }
    })
}

#[test]
fn agent_icon_and_name_open_the_same_conversation() {
    let mut f = Fixture::new();
    let out = f.settle();
    let name = text_rect(&out, "Dot").expect("Dot has a name target");
    f.click(Pos2::new(name.left() - 22.0, name.center().y));
    let chat = f.settle();
    assert_eq!(f.state.active_chat(), Some("dot"));
    assert!(text_rect(&chat, "I am arranging the images in this frame.").is_some());
    f.click(name.center());
    f.settle();
    assert_eq!(f.state.active_chat(), Some("dot"));
    assert!((340.0..=380.0).contains(&f.state.last_popup_rect.width()));
    assert!(f.state.last_popup_rect.bottom() < 720.0);
}

#[test]
fn chat_remains_open_after_an_outside_click_and_human_name_stays_profile() {
    let mut f = Fixture::new();
    f.open_chat("dot");
    let outside = Pos2::new(1100.0, 650.0);
    f.click(outside);
    assert_eq!(f.state.active_chat(), Some("dot"));
    let out = f.settle();
    let you = text_rect(&out, "You").expect("human name remains in roster");
    f.click(you.center());
    f.settle();
    assert_eq!(f.state.expansion, Some(Expansion::Profile("self".into())));
}

#[test]
fn a_profile_dismissal_claims_its_own_click_but_not_the_next_board_frame() {
    let mut f = Fixture::new();
    let out = f.settle();
    let name = text_rect(&out, "You").unwrap();
    f.click(name.center());
    let out = f.settle();
    let close = text_rect(&out, "×").expect("profile close button");
    f.click(close.center());
    assert!(!f.state.is_open());
    assert!(
        f.captured,
        "the close click must not continue to the board below"
    );
    f.frame(vec![]);
    assert!(
        !f.captured,
        "a dismissed panel cannot keep an invisible input shield"
    );
}

#[test]
fn stop_is_clickable_in_the_enclosing_conversation_header_at_720_pixels() {
    let mut f = Fixture::new();
    f.open_chat("dot");
    let panel = f.state.last_popup_rect;
    let point = visual::screen_response(&f, Id::new(("dot", "Stop task")))
        .0
        .center();
    assert!(panel.contains(point));
    f.frame(vec![egui::Event::PointerMoved(point)]);
    assert!(f.state.captures_pointer(&f.ctx));
    let actions = f.click(point);
    assert!(actions.contains(&PresenceAction::Cancel { id: "dot".into() }));
}

#[test]
fn busy_harness_selection_requires_a_timing_choice_and_same_harness_is_inert() {
    let mut f = Fixture::new();
    f.busy = true;
    f.open_chat("dot");
    f.state.popover = Some(Popover::Provider);
    let out = f.settle();
    let cursor = text_rect(&out, "Cursor").expect("real alternative harness");
    let actions = f.click(cursor.center());
    assert!(actions.is_empty());
    assert_eq!(f.state.requested_provider.as_deref(), Some("cursor"));
    let out = f.settle();
    let after = overlay::timing_choice(&f, &out);
    assert!(f.state.last_popover_rect.contains_rect(after));
    assert_eq!(
        f.click(after.center()),
        vec![PresenceAction::SwitchProvider {
            id: "dot".into(),
            provider: "cursor".into(),
            after_task: true
        }]
    );
}

#[test]
fn editing_emits_a_durable_draft_and_escape_does_not_discard_it() {
    let mut f = Fixture::new();
    f.open_chat("dot");
    let composer = Id::new(("presence-composer", "dot"));
    f.click(visual::screen_response(&f, composer).0.center());
    assert_eq!(f.ctx.memory(|memory| memory.focused()), Some(composer));
    let actions = f
        .frame(vec![egui::Event::Text("Keep these images together".into())])
        .0;
    assert!(actions.iter().any(|a|matches!(a,PresenceAction::Draft{id,text}if id=="dot"&&text=="Keep these images together")));
    f.frame(vec![egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(!f.state.is_open());
    f.open_chat("dot");
    assert_eq!(f.state.edits["dot"].message, "Keep these images together");
}

#[test]
fn a_double_click_anchor_stays_in_world_space_when_camera_zoom_changes() {
    let mut state = RosterState::default();
    state.open_chat_at("dot", Pos2::new(400.0, 300.0), 0.25);
    assert_eq!(state.canvas_chat(), Some("dot"));
    state.update_chat_anchor(Pos2::new(200.0, 180.0), 2.0);
    assert_eq!(state.canvas_anchor, Some((Pos2::new(200.0, 180.0), 2.0)));
    state.open_chat("dot");
    assert!(state.canvas_anchor.is_none());
}

fn avatar_bounds(zoom: f32, hat: &str, prop: &str) -> Rect {
    let ctx = egui::Context::default();
    let mut painter = PresencePainter::default();
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(1000.0))),
            ..Default::default()
        },
        |ctx| {
            painter.paint_world_avatar(
                &ctx.layer_painter(egui::LayerId::new(
                    egui::Order::Middle,
                    Id::new("world-scale-test"),
                )),
                AvatarVisual {
                    id: "dot",
                    center: Pos2::new(500.0, 500.0),
                    expression: "neutral",
                    orientation: "forward",
                    hat,
                    prop,
                    opacity: 1.0,
                },
                0.0,
                true,
                zoom,
            );
        },
    );
    ctx.tessellate(output.shapes, output.pixels_per_point)
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
        })
}
#[test]
fn head_hat_and_animated_book_all_scale_with_the_camera_without_a_floor() {
    for (hat, prop) in [("none", "none"), ("cap", "none"), ("cap", "book")] {
        let base = avatar_bounds(1.0, hat, prop);
        for zoom in [0.25, 0.5, 2.0] {
            let actual = avatar_bounds(zoom, hat, prop);
            assert!(
                (actual.width() - base.width() * zoom).abs() < 0.01,
                "{hat}/{prop}/{zoom}"
            );
            assert!(
                (actual.height() - base.height() * zoom).abs() < 0.01,
                "{hat}/{prop}/{zoom}"
            );
        }
    }
}

#[test]
fn conversation_keeps_status_feedback_and_composer_visible_at_720_pixels() {
    let mut f = Fixture::new();
    f.status = "History could not be saved · your draft is retained".into();
    let out = f.open_chat("dot");
    let status = text_rect(&out, &f.status).expect("persistence failure is visible in the chat");
    let message = text_rect(&out, "Message").expect("composer remains reachable");
    let header = text_rect(&out, "Dot").expect("conversation identity");
    assert!(status.top() > header.bottom());
    assert!(message.top() > status.bottom());
    assert!(message.bottom() < 720.0);
    assert!(f.state.last_popup_rect.contains(status.center()));
    assert!(text_rect(&out, "I am arranging the images in this frame.").is_some());
}

#[test]
fn status_changes_are_visible_without_discarding_conversation_drafts() {
    let mut f = Fixture::new();
    f.drafts
        .insert("dot".into(), "Preserve this unsent text".into());
    f.open_chat("dot");
    f.status = "Harness unavailable · reconnect before sending".into();
    let out = f.settle();
    assert!(text_rect(&out, &f.status).is_some());
    assert_eq!(f.state.edits["dot"].message, "Preserve this unsent text");
    assert!(text_rect(&out, "Preserve this unsent text").is_some());
}

#[test]
fn own_created_chat_permission_is_an_explicit_advanced_action() {
    let mut f = Fixture::new();
    f.open_chat("dot");
    f.click(
        visual::screen_response(&f, Id::new(("conversation-more", "dot")))
            .0
            .center(),
    );
    let menu = f.settle();
    let settings = text_rect(&menu, "Agent settings").expect("settings remain inside chat");
    f.click(settings.center());
    let out = f.settle();
    assert!(
        visual::screen_response(&f, Id::new(("presence-name", "dot")))
            .0
            .is_positive()
    );
    let checkbox = text_rect(&out, "Allow chats created by this agent")
        .expect("the advanced scope choice is visible");
    let actions = f.click(checkbox.center());
    assert!(actions.contains(&PresenceAction::SetOwnChats {
        id: "dot".into(),
        enabled: true,
    }));
    assert!(!actions.iter().any(|action| matches!(
        action,
        PresenceAction::SetAutonomy { .. } | PresenceAction::SetFileInputs { .. }
    )));
}

#[path = "roster_added_tests.rs"]
mod added_tests;

#[path = "roster_visual_tests.rs"]
mod visual;

#[path = "roster_proposal_tests.rs"]
mod proposals;

#[path = "roster_overlay_tests.rs"]
mod overlay;
