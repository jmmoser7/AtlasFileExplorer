//! Chevron ownership, partial-card wheel, and the shared text column.

use super::paint_probe::painted_text;
use super::test_support::*;
use super::*;
use eframe::egui::Pos2;

fn chat_with_text(tag: &str) -> (super::super::tests::Harness, NodeId) {
    let mut h = board(tag);
    let id = train(&mut h, Pos2::ZERO, "cursor");
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.title = "Cursor".into();
        }
    });
    let paragraph = "The courtyard keeps its plane trees along the north wall. ";
    h.app.agents.local_turns.insert(
        id,
        (0..12)
            .map(|i| AgentTurn {
                role: if i % 2 == 0 { "user" } else { "assistant" }.into(),
                text: format!("{paragraph}{paragraph}Line {i}."),
                at: i,
            })
            .collect(),
    );
    h.app.agents.output_epoch = h.app.agents.output_epoch.wrapping_add(1);
    (h, id)
}

impl SlateApp {
    pub(crate) fn review_chat_lines(&mut self, id: NodeId, lines: &[(&str, &str)]) {
        self.agents.local_turns.insert(
            id,
            lines
                .iter()
                .enumerate()
                .map(|(i, (role, text))| AgentTurn {
                    role: (*role).into(),
                    text: (*text).into(),
                    at: i as u64,
                })
                .collect(),
        );
        self.agents.output_epoch = self.agents.output_epoch.wrapping_add(1);
    }

    pub(crate) fn review_chat_fold(&mut self, id: NodeId, collapsed: bool, partial: bool) {
        self.patch_nodes(&[id], |n| {
            if let NodeKind::Portal(p) = &mut n.kind {
                let chat = &mut p.agent.as_mut().unwrap().chat;
                chat.collapsed = collapsed;
                chat.partial = partial;
                chat.size = None;
            }
        });
        self.agents.fit_revision = None;
    }

    pub(crate) fn review_dismiss_picker(&mut self) {
        self.agents.project_picker = None;
    }

    pub(crate) fn review_chat_streaming(&mut self, id: NodeId, tokens: u64) {
        self.agents
            .awaiting
            .insert(id, AgentAwait::Responding { req_at: 1 });
        let mut session = self
            .agents
            .sessions
            .get(&id)
            .map(|s| (**s).clone())
            .unwrap_or(AgentSession {
                approval: None,
                conversation: String::new(),
                artifacts: Vec::new(),
                status: AgentStatus::Thinking,
                provider: "cursor".into(),
                turns: Vec::new(),
                updated_at: 0,
                bundle: Default::default(),
                request: String::new(),
                usage: None,
            });
        session.usage = Some(tokens);
        self.agents
            .sessions
            .insert(id, std::sync::Arc::new(session));
    }
}

fn fold(h: &mut super::super::tests::Harness, id: NodeId, collapsed: bool, partial: bool) {
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let chat = &mut p.agent.as_mut().unwrap().chat;
            chat.collapsed = collapsed;
            chat.partial = partial;
            chat.size = None;
        }
    });
    h.app.agents.fit_revision = None;
}

#[test]
fn chevron_press_steps_and_does_not_resize() {
    let (mut h, id) = chat_with_text("ct_chevron");
    fold(&mut h, id, false, false);
    h.app.board_sel.insert(id);
    center_on(&mut h, id);
    h.frame();
    h.frame();
    let width = h.app.doc().scene.node(id).unwrap().rect.w;
    let hits = h.app.agents.chevron_hits.clone();
    assert!(
        hits.len() >= 2,
        "a maximized card paints a double and a single chevron"
    );
    // Open offers the double first, then the single (one level → partial).
    let single = hits[1].center();
    for point in [hits[0].center(), single] {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(point)));
        assert!(
            h.app.chevron_owns(point),
            "the chevron hit owns the pointer"
        );
        assert!(
            !matches!(
                h.app.board_hover_hit,
                Some(super::super::board_handles::BoardHitTarget::Resize(_))
            ),
            "no resize target over the chevron: {:?}",
            h.app.board_hover_hit
        );
    }
    let texts = painted_text(&mut h, Some(single));
    assert!(
        !texts.iter().any(|t| {
            t.contains("Expand fully")
                || t.contains("partial expand")
                || t.contains("Expand one")
                || t.contains("Collapse one")
        }),
        "chevron hover is a highlight only: {texts:?}"
    );
    press(&mut h, single);
    let after = h.app.doc().scene.node(id).unwrap().rect;
    assert!((after.w - width).abs() < 1.0, "a chevron press does not resize");
    let chat = chat(&h, id);
    assert!(
        chat.partial && !chat.collapsed,
        "a single chevron on a maximized card steps to partial: {chat:?}"
    );
}

#[test]
fn every_fold_transition_is_one_or_two_steps() {
    let (mut h, id) = chat_with_text("ct_steps");
    h.app.board_sel.insert(id);
    let step = |h: &mut super::super::tests::Harness, detail: &str| {
        h.app.agent_fold(&h.ctx, Some(detail));
        let chat = chat(h, id);
        (chat.collapsed, chat.partial)
    };
    fold(&mut h, id, false, false);
    assert_eq!(step(&mut h, "step-collapse"), (false, true));
    fold(&mut h, id, false, false);
    assert_eq!(step(&mut h, "jump-collapse"), (true, false));
    fold(&mut h, id, true, false);
    assert_eq!(step(&mut h, "step-expand"), (false, true));
    fold(&mut h, id, true, false);
    assert_eq!(step(&mut h, "jump-expand"), (false, false));
    fold(&mut h, id, false, true);
    assert_eq!(step(&mut h, "step-expand"), (false, false));
    fold(&mut h, id, false, true);
    assert_eq!(step(&mut h, "step-collapse"), (true, false));
}

#[test]
fn a_partial_card_scrolls_and_the_board_does_not_zoom() {
    let (mut h, id) = chat_with_text("ct_wheel");
    fold(&mut h, id, false, true);
    center_on(&mut h, id);
    h.frame();
    h.frame();
    let rect = h.app.board_xf().rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let point = rect.center();
    let zoom = h.app.tab().cam.z;
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(point));
        i.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            modifiers: Default::default(),
        });
    });
    assert!(
        (h.app.tab().cam.z - zoom).abs() < 0.01,
        "the board zoomed over a partial card: {} -> {}",
        zoom,
        h.app.tab().cam.z
    );
    let offset = h
        .app
        .agents
        .transcript_scroll
        .get(&id)
        .copied()
        .unwrap_or(0.0);
    assert!(
        offset.is_finite() && offset > 1.0,
        "the transcript did not scroll: {offset}"
    );
}
