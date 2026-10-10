//! Fork, collapse, and the streaming tail.

use super::test_support::*;
use super::*;

#[test]
fn fork_is_one_undo_and_coding_agents_stay_linear() {
    let mut h = board("fork_chat");
    let id = train(&mut h, Pos2::ZERO, "ollama");
    h.app.agents.local_turns.insert(
        id,
        vec![AgentTurn {
            role: "user".into(),
            text: "hi".into(),
            at: 1,
        }],
    );
    h.app.board_sel.insert(id);
    let before = h.app.doc().scene.nodes.len();
    assert!(h.app.agent_fork_selected(None));
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1);
    let child = h.app.doc().scene.nodes.last().unwrap().id;
    let chat = slate_doc::agent_chat::agent(h.app.doc().scene.node(child).unwrap())
        .unwrap()
        .chat
        .clone();
    assert_eq!(chat.parent, Some(id));
    assert!(chat.draft);
    assert!(slate_doc::agent_chat::history_rails(&h.app.doc().scene)
        .iter()
        .any(|rail| rail.from == id && rail.to == child));
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.nodes.len(), before);
    h.app.board_redo();
    assert_eq!(h.app.doc().scene.nodes.len(), before + 1);

    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    assert!(h.app.agent_fork_selected(Some("[40,200]")));
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    assert!(h.app.agent_fork_selected(Some("[40,400]")));
    let forks = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .filter(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(id)))
        .count();
    assert_eq!(forks, 3);

    let linear = train(&mut h, Pos2::new(800.0, 0.0), "cursor");
    let count = h.app.doc().scene.nodes.len();
    h.app.board_sel.clear();
    h.app.board_sel.insert(linear);
    assert!(h.app.agent_fork_selected(None));
    assert_eq!(h.app.doc().scene.nodes.len(), count);
}

/// Streaming opens a collapsed card for display only: the document keeps
/// it collapsed and the capsule returns when the reply ends. A fold the
/// person makes meanwhile is theirs and stays.
#[test]
fn a_streaming_open_is_display_state_unless_the_person_folds_it() {
    let mut h = board("stream_open");
    let id = train(&mut h, Pos2::ZERO, "ollama");
    h.frame();
    let ctx = h.ctx.clone();
    let collapsed = |h: &super::super::tests::Harness| {
        slate_doc::agent_chat::agent(h.app.doc().scene.node(id).unwrap())
            .unwrap()
            .chat
            .collapsed
    };
    h.app.board_sel.clear();
    h.app.board_sel.insert(id);
    if !collapsed(&h) {
        assert!(h.app.agent_toggle_collapse(&ctx));
    }
    assert!(collapsed(&h));
    let responding = || AgentAwait::Responding { req_at: 1 };

    h.app.agents.awaiting.insert(id, responding());
    h.app.open_streaming_card(id);
    assert!(h.app.agents.stream_open.contains(&id));
    assert!(collapsed(&h), "the document keeps the card collapsed");
    h.app.agents.awaiting.remove(&id);
    h.app.settle_stream_open();
    assert!(h.app.agents.stream_open.is_empty(), "the capsule returns");
    assert!(collapsed(&h));

    h.app.agents.awaiting.insert(id, responding());
    h.app.open_streaming_card(id);
    assert!(h.app.agent_fold(&ctx, Some("one")));
    assert!(h.app.agents.stream_open.is_empty());
    assert!(
        !collapsed(&h),
        "one step up from the shown partial opens fully"
    );
    h.app.agents.awaiting.remove(&id);
    h.app.settle_stream_open();
    assert!(
        !collapsed(&h),
        "the person's fold survives the stream ending"
    );
}

#[test]
fn after_send_the_caret_is_already_on_the_new_tail() {
    let mut h = board("caret_follows");
    let first = train(&mut h, Pos2::ZERO, "cursor");
    let ws = h.base.join("ai-ws");
    std::fs::create_dir_all(&ws).unwrap();
    h.app.ai.config.workspace_dir = Some(ws);
    h.frame();
    h.app.patch_nodes(&[first], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.detail = slate_doc::agent_chat::Detail::Summary;
        }
    });
    *h.app.agents.prompt_mut(first) = "first question".into();
    h.app.agents.composer_editing = Some(first);
    h.app.send_agent_prompt(first);
    let tail = h
        .app
        .doc()
        .scene
        .nodes
        .iter()
        .find(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.parent == Some(first)))
        .map(|n| n.id)
        .expect("sending spawns the next card");
    let r = h.app.doc().scene.node(tail).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    for _ in 0..4 {
        h.frame();
    }
    assert!(
        h.ctx
            .memory(|m| m.has_focus(Id::new(("agent-composer", tail.0)))),
        "typing continues on the new card without a click"
    );
}

#[test]
fn pair_cards_show_their_first_line_while_streaming_and_after() {
    let mut h = board("pair_fit");
    let upstream = train(&mut h, Pos2::ZERO, "local");
    h.app.patch_nodes(&[upstream], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.detail = slate_doc::agent_chat::Detail::Pair;
        }
    });
    let tail = next_card(&mut h, upstream);
    h.frame();
    let ask = "can you take a look at your folder and output the newest html file for a dashboard right here?";
    let reply = "I'll find the newest dashboard in the project and register it. ".repeat(5);
    for (id, reply) in [(upstream, reply.clone()), (tail, "I'll".to_string())] {
        h.app.agents.local_turns.insert(
            id,
            vec![
                AgentTurn {
                    role: "user".into(),
                    text: ask.into(),
                    at: 0,
                },
                AgentTurn {
                    role: "assistant".into(),
                    text: reply,
                    at: 1,
                },
            ],
        );
    }
    h.app
        .agents
        .awaiting
        .insert(tail, AgentAwait::Responding { req_at: 1 });
    h.app.agents.output_epoch += 1;
    let r = h.app.doc().scene.node(upstream).unwrap().rect;
    h.app.tab_mut().cam.offset = egui::vec2(r.x + r.w, r.y + r.h * 0.5);
    h.app.tab_mut().cam.z = 1.0;
    for _ in 0..8 {
        h.frame();
    }
    for id in [upstream, tail] {
        assert_eq!(
            h.app
                .agents
                .transcript_scroll
                .get(&id)
                .copied()
                .unwrap_or(0.0),
            0.0,
            "the first line of {id:?} is not scrolled out of the top"
        );
    }
    let sent = h.app.doc().scene.node(upstream).unwrap().rect;
    let (_, content, _) = h.app.agents.content_heights[&upstream];
    assert!(
        (sent.h - (COMPOSER_TOP + content + COMPOSER_BOTTOM)).abs() <= 1.0,
        "a sent card ends at its text, with no composer band: {} vs {}",
        sent.h,
        content
    );
}

#[test]
fn wires_into_a_train_start_calm_gray_and_keep_a_chosen_color() {
    use slate_doc::scene::{ConnectorEnd, Rgba, Side};
    let mut h = board("chat_wire_gray");
    let card = train(&mut h, Pos2::ZERO, "cursor");
    let text = note(&mut h, Pos2::new(-400.0, 0.0), "context");
    h.app.board_colors.fg = Rgba([230, 60, 150, 255]);
    let wire = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: text,
            side: Side::Right,
            t: 0.5,
        },
        ConnectorEnd::Anchored {
            node: card,
            side: Side::Left,
            t: 0.5,
        },
    );
    let id = h.app.add_nodes(vec![wire])[0];
    let color = |h: &super::super::tests::Harness| match &h.app.doc().scene.node(id).unwrap().kind {
        NodeKind::Connector(c) => c.color,
        _ => unreachable!(),
    };
    assert_eq!(
        color(&h),
        Some(h.app.chat_wire_color()),
        "not the pink drawing color"
    );
    let chosen = Rgba([40, 120, 220, 255]);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Connector(c) = &mut n.kind {
            c.set_color(Some(chosen));
        }
    });
    for _ in 0..3 {
        h.frame();
    }
    assert_eq!(
        color(&h),
        Some(chosen),
        "a color the person picks is never reset"
    );
    let loose = h.app.build_connector(
        ConnectorEnd::Anchored {
            node: text,
            side: Side::Bottom,
            t: 0.5,
        },
        ConnectorEnd::Free {
            point: [0.0, 400.0],
        },
    );
    let NodeKind::Connector(c) = &loose.kind else {
        unreachable!()
    };
    assert_eq!(
        c.color, None,
        "other wires follow the theme, not the drawing color"
    );
}

#[test]
fn sent_cards_name_their_model_and_only_the_tail_offers_the_dropdown() {
    let mut h = board("agent_tail_model");
    let root = train(&mut h, Pos2::ZERO, "ollama");
    h.app.agents.models_started.insert("ollama".into());
    h.app.agents.models.insert(
        "ollama".into(),
        vec![atlas_ai::agent::AgentModel {
            id: "qwen3:8b".into(),
            name: "qwen3:8b".into(),
        }],
    );
    h.app.patch_nodes(&[root], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.title = "Review".into();
            p.agent.as_mut().unwrap().model = Some("llama3.1:8b".into());
        }
    });
    let tail = next_card(&mut h, root);
    h.frame();
    center_on(&mut h, root);
    h.frame();
    let label = {
        let node = h.app.doc().scene.node(root).unwrap();
        h.app
            .model_menu_label(slate_doc::agent_chat::agent(node).unwrap())
    };
    let texts = painted_text(&mut h, None);
    assert!(
        texts.iter().any(|t| *t == format!("Review · {label}")),
        "a sent card names its model as text: {texts:?}"
    );
    h.app.board_sel = [root].into_iter().collect();
    assert!(!h.app.agent_set_model("qwen3:8b"), "sent card refuses");
    h.app.board_sel = [tail].into_iter().collect();
    assert!(h.app.agent_set_model("qwen3:8b"), "the tail chooses");
}

#[test]
fn collapse_keeps_width_three_lines_and_the_tail_composer_with_one_undo() {
    let mut h = board("agent_collapse");
    let root = train(&mut h, Pos2::ZERO, "local");
    let tail = next_card(&mut h, root);
    h.frame();
    let long = "A reply long enough to wrap across many lines of the card. ".repeat(12);
    for id in [root, tail] {
        h.app.agents.local_turns.insert(
            id,
            vec![AgentTurn {
                role: "assistant".into(),
                text: long.clone(),
                at: 0,
            }],
        );
    }
    h.app.agents.output_epoch += 1;
    h.frame();
    center_on(&mut h, tail);
    h.frame();
    let open = h.app.doc().scene.node(root).unwrap().rect;
    let open_tail = h.app.doc().scene.node(tail).unwrap().rect;
    let text = |h: &super::super::tests::Harness, id| {
        h.app
            .visible_agent_turns(id)
            .iter()
            .map(|t| t.text.clone())
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    h.app.board_sel = [root].into_iter().collect();
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.agent.collapse"),
        None
    ));
    h.frame();
    let rect = h.app.doc().scene.node(root).unwrap().rect;
    assert!(chat(&h, root).collapsed);
    assert_eq!(rect.w, open.w, "a collapsed card keeps its width");
    let three = collapsed_card_height(&h.ctx, text(&h, root), rect.w);
    assert!((rect.h - three).abs() < 0.5, "{} vs {three}", rect.h);
    assert!(rect.h < open.h, "{} vs open {}", rect.h, open.h);

    h.app.board_sel = [tail].into_iter().collect();
    h.app.agent_toggle_collapse(&h.ctx);
    h.frame();
    h.frame();
    let rect = h.app.doc().scene.node(tail).unwrap().rect;
    let composer = composer_text_height(&h.ctx, "", composer_wrap(rect.w), 14.0);
    let expected =
        collapsed_card_height(&h.ctx, text(&h, tail), rect.w) + composer + COMPOSER_BOTTOM;
    assert!((rect.h - expected).abs() < 0.5, "{} vs {expected}", rect.h);
    assert!(
        h.app.agents.composer_rects.contains_key(&tail),
        "the collapsed tail keeps its composer"
    );
    h.app.board_undo();
    h.frame();
    assert!(!chat(&h, tail).collapsed, "one undo expands");
    assert_eq!(h.app.doc().scene.node(tail).unwrap().rect.h, open_tail.h);
}
