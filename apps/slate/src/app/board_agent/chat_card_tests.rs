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
    let lines: Vec<(String, String)> = (0..12)
        .map(|i| {
            (
                if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                format!("{paragraph}{paragraph}Line {i}."),
            )
        })
        .collect();
    let lines: Vec<(&str, &str)> = lines
        .iter()
        .map(|(r, t)| (r.as_str(), t.as_str()))
        .collect();
    h.app.review_chat_lines(id, &lines);
    (h, id)
}

impl SlateApp {
    /// Session sync drops turns, sessions, and awaits for a card whose binding
    /// it has not recorded yet, so seeding records it first.
    fn review_bind(&mut self, id: NodeId) {
        if let Some(a) = self
            .doc()
            .scene
            .node(id)
            .and_then(slate_doc::agent_chat::agent)
        {
            let session = a.session.clone();
            self.agents.bindings.insert(id, session);
        }
    }

    pub(crate) fn review_chat_lines(&mut self, id: NodeId, lines: &[(&str, &str)]) {
        self.review_bind(id);
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
        self.review_bind(id);
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
    h.app.review_chat_fold(id, collapsed, partial);
}

fn wheel(h: &mut super::super::tests::Harness, at: Pos2, dy: f32) {
    h.frame_with(|i| {
        i.events.push(egui::Event::PointerMoved(at));
        i.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            modifiers: Default::default(),
        });
    });
    for _ in 0..12 {
        h.frame_with(|i| i.events.push(egui::Event::PointerMoved(at)));
    }
}

fn scroll_of(h: &super::super::tests::Harness, id: NodeId) -> f32 {
    h.app
        .agents
        .transcript_scroll
        .get(&id)
        .copied()
        .unwrap_or(0.0)
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
    assert!(
        (after.w - width).abs() < 1.0,
        "a chevron press does not resize"
    );
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
    center_on(&mut h, id);
    h.frame();
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
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let point = rect.center();
    let zoom = h.app.tab().cam.z;
    wheel(&mut h, point, -120.0);
    assert!(
        (h.app.tab().cam.z - zoom).abs() < 0.01,
        "the board zoomed over a partial card: {} -> {}",
        zoom,
        h.app.tab().cam.z
    );
    let offset = scroll_of(&h, id);
    assert!(
        offset.is_finite() && offset > 1.0,
        "the transcript did not scroll: {offset}"
    );
}

/// Where the first transcript line lands relative to the card, its type
/// size, wrap width, and first row.
fn first_line(h: &mut super::super::tests::Harness, id: NodeId) -> (f32, f32, f32, f32, String) {
    fn walk(shape: &egui::Shape, out: &mut Vec<egui::epaint::TextShape>) {
        match shape {
            egui::Shape::Text(t) => out.push(t.clone()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let card = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1440.0, 900.0))),
        ..Default::default()
    };
    let ctx = h.ctx.clone();
    let app = &mut h.app;
    let output = ctx.run(input, |c| app.update_app(c));
    let mut texts = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut texts);
    }
    let t = texts
        .iter()
        .find(|t| t.galley.text().starts_with("The courtyard"))
        .expect("the first message is painted");
    let font = t.galley.job.sections[0].format.font_id.size;
    (
        t.pos.x - card.left(),
        t.pos.y - card.top(),
        font,
        t.galley.job.wrap.max_width,
        t.galley.rows[0].text(),
    )
}

#[test]
fn the_text_column_is_the_same_in_every_fold() {
    let (mut h, id) = chat_with_text("ct_column");
    center_on(&mut h, id);
    let mut seen = Vec::new();
    for (name, collapsed, partial) in [
        ("collapsed", true, false),
        ("partial", false, true),
        ("maximized", false, false),
    ] {
        fold(&mut h, id, collapsed, partial);
        for _ in 0..4 {
            h.frame();
        }
        center_on(&mut h, id);
        h.frame();
        seen.push((name, first_line(&mut h, id)));
    }
    let (_, base) = &seen[0];
    let (inset_l, _, _) = text_column(h.app.doc().scene.node(id).unwrap().rect.w);
    assert!((base.0 - inset_l).abs() < 0.5, "left inset {}", base.0);
    for (name, line) in &seen[1..] {
        assert!(
            (line.0 - base.0).abs() < 0.5,
            "{name} left inset {} vs {}",
            line.0,
            base.0
        );
        assert!(
            (line.1 - base.1).abs() < 0.5,
            "{name} text top {} vs {}",
            line.1,
            base.1
        );
        assert!(
            (line.2 - base.2).abs() < 0.01,
            "{name} type {} vs {}",
            line.2,
            base.2
        );
        assert!(
            (line.3 - base.3).abs() < 0.5,
            "{name} wrap {} vs {}",
            line.3,
            base.3
        );
        assert_eq!(line.4, base.4, "{name} breaks the first line elsewhere");
    }
}

#[test]
fn a_streaming_partial_card_follows_until_the_reader_scrolls_up() {
    let (mut h, id) = chat_with_text("ct_follow");
    fold(&mut h, id, false, true);
    h.app.review_chat_streaming(id, 1200);
    center_on(&mut h, id);
    for _ in 0..4 {
        h.frame();
    }
    let bottom = scroll_of(&h, id);
    let overflow = h.app.agents.card_overflow.get(&id).copied().unwrap_or(0.0);
    assert!(overflow > 20.0, "the transcript overflows: {overflow}");
    assert!(
        (bottom - overflow).abs() < 2.0,
        "a streaming partial card is pinned to the bottom: {bottom} vs {overflow}"
    );
    let rect = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(id).unwrap().rect);
    wheel(&mut h, rect.center(), 160.0);
    let up = scroll_of(&h, id);
    assert!(
        up < bottom - 20.0,
        "the reader scrolled up: {up} vs {bottom}"
    );
    h.app
        .agents
        .local_turns
        .get_mut(&id)
        .unwrap()
        .push(AgentTurn {
            role: "assistant".into(),
            text: "More streamed text arrives while the reader looks back.".into(),
            at: 99,
        });
    h.app.agents.output_epoch += 1;
    for _ in 0..4 {
        h.frame();
    }
    assert!(
        (scroll_of(&h, id) - up).abs() < 2.0,
        "new text does not pull the reader back down"
    );
}
