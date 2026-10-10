//! Train boards, presentation shape, and streaming tails shared by the agent session tests.

use super::*;

/// The main line of the branched conversation, as a person reads it.
pub(super) const MAIN: [&str; 6] = [
    "Plan the courtyard?",
    "Start with the trees.",
    "Which trees?",
    "Two plane trees.",
    "Where?",
    "By the north wall.",
];

/// The fork after the first exchange.
pub(super) const FORK: [&str; 6] = [
    "Plan the courtyard?",
    "Start with the trees.",
    "Pave it instead?",
    "Granite setts.",
    "In what pattern?",
    "A fan pattern.",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Shape {
    Pairs,
    Train,
    Window,
}

pub(super) fn board(tag: &str) -> super::super::tests::Harness {
    let mut h = super::super::tests::Harness::new(tag);
    h.app.leave_home();
    h.app.ensure_work_tab();
    h.app.doc_mut().view.active_view = slate_doc::ViewKind::Board;
    h
}

/// A local train's first card at `at`.
pub(super) fn train(h: &mut super::super::tests::Harness, at: Pos2, provider: &str) -> NodeId {
    h.app.place_agent_portal_at(at);
    let id = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_agent_program(id, provider);
    h.app.agents.project_picker = None;
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            let a = p.agent.as_mut().unwrap();
            a.bundle = None;
            a.chat.train = true;
        }
    });
    id
}

/// The next card of `parent`'s train, which becomes the tail.
pub(super) fn next_card(h: &mut super::super::tests::Harness, parent: NodeId) -> NodeId {
    let original = h.app.doc().scene.node(parent).unwrap().clone();
    let mut child = h.app.doc_mut().scene.build_duplicate(&original, 420.0, 0.0);
    if let NodeKind::Portal(p) = &mut child.kind {
        p.agent.as_mut().unwrap().chat.parent = Some(parent);
    }
    let id = child.id;
    h.app.add_nodes(vec![child]);
    id
}

pub(super) fn press(h: &mut super::super::tests::Harness, at: Pos2) {
    h.frame_with(|i| i.events.push(egui::Event::PointerMoved(at)));
    for pressed in [true, false] {
        h.frame_with(|i| {
            i.events.push(egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        });
    }
}

pub(super) fn center_on(h: &mut super::super::tests::Harness, id: NodeId) {
    let r = h.app.doc().scene.node(id).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
}

/// A Codex train card whose model list holds three installed models.
pub(super) fn codex_card_with_models(h: &mut super::super::tests::Harness) -> NodeId {
    let card = train(h, Pos2::ZERO, "codex");
    let catalog = atlas_ai::agent::model_catalog("codex").to_string();
    h.app.agents.models_started.insert(catalog.clone());
    h.app.agents.models.insert(
        catalog,
        ["gpt-6-astra", "gpt-6-terra", "gpt-6-luna"]
            .into_iter()
            .map(|id| atlas_ai::agent::AgentModel {
                id: id.into(),
                name: id.into(),
            })
            .collect(),
    );
    h.app.patch_nodes(&[card], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.title = "Review".into();
            p.agent.as_mut().unwrap().model = Some("gpt-6-astra".into());
        }
    });
    card
}

/// A Cursor train whose tail is streaming a reply to request `req-stop`.
/// Returns (harness, tail, the file a Stop writes).
pub(super) fn streaming_tail(tag: &str) -> (super::super::tests::Harness, NodeId, PathBuf) {
    let mut h = board(tag);
    let ws = h.base.join("ai-ws");
    std::fs::create_dir_all(&ws).unwrap();
    h.app.ai.config.workspace_dir = Some(ws.clone());
    let root = train(&mut h, Pos2::ZERO, "cursor");
    let tail = next_card(&mut h, root);
    // Binding a card to its session starts it with no transcript.
    h.frame();
    let turn = |role: &str, text: &str, at| AgentTurn {
        role: role.into(),
        text: text.into(),
        at,
    };
    h.app.agents.local_turns.insert(
        root,
        vec![turn("user", "first", 0), turn("assistant", "one", 1)],
    );
    h.app.agents.local_turns.insert(
        tail,
        vec![
            turn("user", "first", 0),
            turn("assistant", "one", 1),
            turn("user", "second", 2),
            // Past one full line, so the card is at its width.
            turn("assistant", &"Streaming a reply ".repeat(4), 3),
        ],
    );
    h.app.agents.requests.insert(tail, "req-stop".into());
    h.app
        .agents
        .awaiting
        .insert(tail, AgentAwait::Responding { req_at: 2 });
    h.app.agents.output_epoch += 1;
    h.frame();
    center_on(&mut h, tail);
    h.frame();
    let session = slate_doc::agent_chat::agent(h.app.doc().scene.node(tail).unwrap())
        .unwrap()
        .session
        .clone();
    let cancel = atlas_ai::agent::agent_dir(&ws, &session).join("cancel.json");
    (h, tail, cancel)
}

pub(super) fn turns_of(texts: &[&str]) -> Vec<AgentTurn> {
    texts
        .iter()
        .enumerate()
        .map(|(i, text)| AgentTurn {
            role: if i % 2 == 0 { "user" } else { "assistant" }.into(),
            text: (*text).into(),
            at: i as u64,
        })
        .collect()
}

/// A local conversation in message pairs, as sending builds it: three
/// exchanges on the main line and a fork after the first. Every sent card
/// has its own session file holding the history it replayed. Returns
/// the cards: main line first, then the fork.
pub(super) fn branched_pairs(tag: &str) -> (super::super::tests::Harness, Vec<NodeId>) {
    use slate_doc::agent_chat::{ChatView, Detail};
    let mut h = board(tag);
    let ws = h.base.join("ai-ws");
    std::fs::create_dir_all(&ws).unwrap();
    h.app.ai.config.workspace_dir = Some(ws.clone());
    let root = train(&mut h, Pos2::ZERO, "local");
    let template = h.app.doc().scene.node(root).unwrap().clone();
    /// Parent index, start, end, and the history its session replayed.
    type Card<'a> = (Option<usize>, usize, Option<usize>, &'a [&'a str]);
    let cards: [Card; 5] = [
        (None, 0, Some(2), &MAIN[..2]),
        (Some(0), 2, Some(4), &MAIN[..4]),
        (Some(1), 4, None, &MAIN[..]),
        (Some(0), 2, Some(4), &FORK[..4]),
        (Some(3), 4, None, &FORK[..]),
    ];
    let mut ids: Vec<NodeId> = Vec::new();
    for (i, (parent, start, end, texts)) in cards.into_iter().enumerate() {
        let session = slate_doc::scene::new_agent_session_id();
        let dir = atlas_ai::agent::agent_dir(&ws, &session);
        std::fs::create_dir_all(&dir).unwrap();
        let state = atlas_ai::agent::AgentSession {
            usage: None,
            approval: None,
            conversation: String::new(),
            artifacts: vec![],
            status: atlas_ai::agent::AgentStatus::Idle,
            provider: "local".into(),
            turns: turns_of(texts),
            updated_at: 1,
            bundle: Default::default(),
            request: String::new(),
        };
        atlas_ai::agent::atomic_write_json(&dir.join("session.json"), &state).unwrap();
        let mut node = if i == 0 {
            template.clone()
        } else {
            h.app.doc_mut().scene.build_duplicate(&template, 0.0, 0.0)
        };
        node.rect = slate_doc::WorldRect::new(
            i as f32 * 416.0,
            if i >= 3 { 400.0 } else { 0.0 },
            slate_doc::agent_chat::CARD_WIDTH,
            slate_doc::agent_chat::PAIR_HEIGHT,
        );
        if let NodeKind::Portal(p) = &mut node.kind {
            p.title = "Courtyard".into();
            let a = p.agent.as_mut().unwrap();
            a.session = session;
            a.bundle = Some(slate_doc::SourceUri {
                locator: super::super::board_portal::source_locator(
                    None,
                    &dir.join("session.json"),
                ),
            });
            a.chat = ChatView {
                train: true,
                parent: parent.map(|j| ids[j]),
                start,
                end,
                detail: Detail::Pair,
                ..Default::default()
            };
        }
        if i == 0 {
            let before = h.app.doc().scene.node(root).unwrap().clone();
            h.app.commit_scene(vec![slate_doc::SceneCmd::Patch {
                before: Box::new(before),
                after: Box::new(node),
            }]);
            ids.push(root);
        } else {
            ids.extend(h.app.add_nodes(vec![node]));
        }
    }
    settle(&mut h);
    for (id, want) in ids.iter().zip([2, 2, 2, 2, 2]) {
        assert_eq!(
            shown(&h, *id).len(),
            want,
            "the fixture loaded its sessions"
        );
    }
    (h, ids)
}

/// Frames until every card's session file has loaded (a worker reads
/// it) and cards fit their text.
pub(super) fn settle(h: &mut super::super::tests::Harness) {
    for _ in 0..300 {
        h.frame();
        let loaded = h.app.doc().scene.nodes.iter().all(|n| {
            slate_doc::agent_chat::agent(n)
                .is_none_or(|a| a.bundle.is_none() || h.app.agents.sessions.contains_key(&n.id))
        });
        if loaded {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..12 {
        h.frame();
    }
}

pub(super) fn shown(h: &super::super::tests::Harness, id: NodeId) -> Vec<String> {
    h.app
        .visible_agent_turns(id)
        .into_iter()
        .map(|t| t.text)
        .collect()
}

pub(super) fn switch(h: &mut super::super::tests::Harness, card: NodeId, to: Shape) {
    h.app.board_sel = std::iter::once(card).collect();
    let command = match to {
        Shape::Pairs => "portal.agent.pairs",
        Shape::Train => "portal.agent.train",
        Shape::Window => "portal.agent.chat",
    };
    let ctx = h.ctx.clone();
    assert!(
        h.app
            .dispatch(&ctx, atlas_commands::CommandId(command), None),
        "{command} ran"
    );
    settle(h);
}

/// The whole conversation is in `shape`: both branches read whole and
/// once, every visible card has the shape's form, cards read left to
/// right without overlapping, and no provider ran.
pub(super) fn assert_projected(h: &super::super::tests::Harness, root: NodeId, shape: Shape) {
    assert_projected_with(h, root, shape, None);
}

/// [`assert_projected`] with one unsent draft card allowed in the train.
pub(super) fn assert_projected_with(
    h: &super::super::tests::Harness,
    root: NodeId,
    shape: Shape,
    draft: Option<NodeId>,
) {
    use slate_doc::agent_chat::{conversation, visible_parent, Detail};
    let scene = &h.app.doc().scene;
    let visible: Vec<&Node> = conversation(scene, root)
        .into_iter()
        .filter_map(|id| scene.node(id))
        .filter(|n| !n.hidden)
        .collect();
    let leaves: Vec<&Node> = visible
        .iter()
        .copied()
        .filter(|n| {
            !visible
                .iter()
                .any(|m| visible_parent(scene, m) == Some(n.id))
        })
        .collect();
    let mut read: Vec<Vec<String>> = leaves
        .iter()
        .map(|leaf| {
            let mut path = vec![leaf.id];
            let mut at = *leaf;
            while let Some(parent) = visible_parent(scene, at) {
                path.push(parent);
                at = scene.node(parent).unwrap();
            }
            path.iter().rev().flat_map(|id| shown(h, *id)).collect()
        })
        .collect();
    read.sort();
    let mut want: Vec<Vec<String>> = [MAIN, FORK]
        .iter()
        .map(|b| b.iter().map(|t| t.to_string()).collect())
        .collect();
    want.sort();
    assert_eq!(read, want, "{shape:?}: each branch reads whole, once");
    for n in visible.iter().filter(|n| Some(n.id) != draft) {
        let a = slate_doc::agent_chat::agent(n).unwrap();
        let turns = h.app.visible_agent_turns(n.id);
        assert!(!a.chat.draft, "{shape:?}: no stray draft card");
        match shape {
            Shape::Pairs => {
                assert!(a.chat.train && a.chat.detail == Detail::Pair, "{shape:?}");
                let roles: Vec<_> = turns.iter().map(|t| t.role.as_str()).collect();
                assert_eq!(
                    roles,
                    ["user", "assistant"],
                    "{shape:?}: one exchange per card"
                );
            }
            Shape::Train => {
                assert!(
                    a.chat.train && a.chat.detail == Detail::Summary,
                    "{shape:?}"
                );
                assert_eq!(turns.len(), 1, "{shape:?}: one message per card");
            }
            Shape::Window => {
                assert!(!a.chat.train && a.chat.detail == Detail::Full, "{shape:?}");
            }
        }
    }
    if shape == Shape::Window {
        assert_eq!(
            visible.len(),
            slate_doc::agent_chat::segments(scene, root).len(),
            "one window per unbranched run"
        );
    }
    for n in &visible {
        if let Some(parent) = visible_parent(scene, n).and_then(|p| scene.node(p)) {
            assert!(
                n.rect.x >= parent.rect.x + parent.rect.w,
                "{shape:?}: {:?} reads left to right after {:?}",
                n.id,
                parent.id
            );
        }
    }
    for (i, a) in visible.iter().enumerate() {
        for b in &visible[i + 1..] {
            let apart = a.rect.x + a.rect.w <= b.rect.x
                || b.rect.x + b.rect.w <= a.rect.x
                || a.rect.y + a.rect.h <= b.rect.y
                || b.rect.y + b.rect.h <= a.rect.y;
            assert!(apart, "{shape:?}: {:?} overlaps {:?}", a.id, b.id);
        }
    }
    assert!(
        h.app.agents.dispatched.is_empty(),
        "{shape:?}: no provider call"
    );
}

/// The visible sent card that ends the main line.
pub(super) fn main_tail(h: &super::super::tests::Harness, root: NodeId) -> NodeId {
    let scene = &h.app.doc().scene;
    slate_doc::agent_chat::conversation(scene, root)
        .into_iter()
        .find(|id| {
            let n = scene.node(*id).unwrap();
            !n.hidden
                && !slate_doc::agent_chat::agent(n).unwrap().chat.draft
                && shown(h, *id).last().map(String::as_str) == Some(MAIN[5])
        })
        .expect("the main line has a visible tail")
}
