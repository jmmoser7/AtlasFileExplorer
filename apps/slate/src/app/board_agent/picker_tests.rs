//! Resize, pocket, and model-list zoom.

use super::test_support::*;
use super::*;

#[test]
fn a_resize_records_its_size_and_fit_to_text_forgets_it() {
    let mut h = board("agent_resize");
    let root = train(&mut h, Pos2::ZERO, "local");
    h.frame();
    h.app.agents.local_turns.insert(
        root,
        vec![AgentTurn {
            role: "assistant".into(),
            text: "Short reply".into(),
            at: 0,
        }],
    );
    h.app.agents.output_epoch += 1;
    h.frame();
    h.app.board_sel = [root].into_iter().collect();
    h.app.agent_toggle_collapse(&h.ctx);
    h.frame();
    let fitted = h.app.doc().scene.node(root).unwrap().rect;
    let before = h.app.doc().scene.node(root).unwrap().clone();
    let depth = h.app.tab().journal.undo_depth();
    {
        let live = h.app.doc_mut().scene.node_mut(root).unwrap();
        live.rect.w = 520.0;
        live.rect.h = 90.0;
    }
    h.app.board_drag = Some(super::super::board::BoardDrag::Resize {
        id: root,
        before,
        handle: 0,
        dup: false,
    });
    h.app
        .end_gesture_for_test(Pos2::ZERO, None, egui::Modifiers::NONE);
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one undo step");
    let view = chat(&h, root);
    assert_eq!(view.size, Some([520.0, 90.0]));
    assert!(!view.collapsed, "resizing opens a collapsed card");
    h.frame();
    h.frame();
    let rect = h.app.doc().scene.node(root).unwrap().rect;
    assert_eq!((rect.w, rect.h), (520.0, 90.0), "fit keeps the size");

    assert!(h
        .app
        .dispatch(&h.ctx, atlas_commands::CommandId("portal.agent.fit"), None));
    h.frame();
    assert!(chat(&h, root).size.is_none());
    assert_ne!(h.app.doc().scene.node(root).unwrap().rect.w, 520.0);
    h.app.board_undo();
    h.app.board_undo();
    assert_eq!(h.app.doc().scene.node(root).unwrap().rect.w, fitted.w);
    assert!(chat(&h, root).collapsed, "undo restores the collapsed card");
}

#[test]
fn clicking_each_trains_message_area_focuses_its_own_composer() {
    let mut h = board("agent_two_trains");
    let first = train(&mut h, Pos2::new(-520.0, -260.0), "local");
    let second = train(&mut h, Pos2::new(120.0, 140.0), "local");
    h.app.tab_mut().cam.offset = egui::vec2(0.0, 0.0);
    h.app.tab_mut().cam.z = 1.0;
    h.frame();
    h.frame();
    for id in [first, second, first] {
        let before = h.app.doc().scene.node(id).unwrap().rect;
        let field = *h
            .app
            .agents
            .composer_rects
            .get(&id)
            .expect("each tail paints its Message field");
        press(&mut h, field.left_center() + egui::vec2(8.0, 0.0));
        h.frame();
        assert!(
            h.ctx
                .memory(|m| m.has_focus(Id::new(("agent-composer", id.0)))),
            "the clicked train's composer has the caret"
        );
        assert!(h.app.board_drag.is_none(), "the press did not start a move");
        assert_eq!(h.app.doc().scene.node(id).unwrap().rect, before);
        assert_eq!(h.app.agents.composer_editing, Some(id));
    }
}

#[test]
fn the_tail_deletes_by_key_unless_text_is_being_typed() {
    let mut h = board("agent_delete_tail");
    let root = train(&mut h, Pos2::ZERO, "local");
    let tail = next_card(&mut h, root);
    h.frame();
    center_on(&mut h, tail);
    h.frame();

    // Typed text keeps Delete and Backspace in the field.
    h.app.board_sel = [tail].into_iter().collect();
    h.app.agents.composer_focus = Some(tail);
    h.frame();
    h.frame();
    h.frame_with(|i| i.events.push(egui::Event::Text("draft".into())));
    assert_eq!(h.app.agents.composer_editing, Some(tail));
    key(&mut h, egui::Key::Delete);
    key(&mut h, egui::Key::Backspace);
    assert!(h.app.doc().scene.node(tail).is_some(), "typing is editing");
    assert_eq!(h.app.agents.prompts[&tail], "draf");

    // An idle composer: the selected tail deletes, with its unsent text.
    h.ctx
        .memory_mut(|m| m.surrender_focus(Id::new(("agent-composer", tail.0))));
    h.frame();
    h.frame();
    assert_eq!(h.app.agents.composer_editing, None);
    h.app.agent_focus(tail);
    h.app.board_sel = [tail].into_iter().collect();
    let depth = h.app.tab().journal.undo_depth();
    key(&mut h, egui::Key::Delete);
    assert!(
        h.app.doc().scene.node(tail).is_none(),
        "Delete removes the tail"
    );
    assert!(!h.app.agents.prompts.contains_key(&tail), "draft discarded");
    assert!(!h.app.agent_has_child(root), "the parent is the tail again");
    h.frame();
    h.frame();
    assert!(
        h.app.agents.composer_rects.contains_key(&root),
        "the parent regains the composer"
    );
    h.app.board_undo();
    assert!(h.app.doc().scene.node(tail).is_some(), "one undo restores");
    assert_eq!(h.app.tab().journal.undo_depth(), depth);

    // An empty, focused composer lets Delete through.
    h.app.board_sel = [tail].into_iter().collect();
    h.app.agents.composer_focus = Some(tail);
    h.frame();
    h.frame();
    assert_eq!(h.app.agents.composer_editing, Some(tail));
    key(&mut h, egui::Key::Backspace);
    assert!(
        h.app.doc().scene.node(tail).is_some(),
        "Backspace stays text"
    );
    key(&mut h, egui::Key::Delete);
    assert!(h.app.doc().scene.node(tail).is_none(), "empty composer");
}

#[test]
fn deleting_sent_context_pockets_it_and_the_handle_restores_it() {
    let mut h = board("agent_pocket");
    let card = train(&mut h, Pos2::ZERO, "local");
    let used = note(&mut h, Pos2::new(-400.0, 0.0), "site plan");
    let fresh = note(&mut h, Pos2::new(-400.0, 300.0), "not sent");
    let wire = sent_wire(&mut h, used, card);
    h.frame();

    h.app.board_sel = [fresh].into_iter().collect();
    h.app.delete_board_nodes(&[fresh]);
    assert!(h.app.doc().scene.node(fresh).is_none(), "unsent deletes");

    let depth = h.app.tab().journal.undo_depth();
    h.app.delete_board_nodes(&[used]);
    let n = h.app.doc().scene.node(used).expect("same node id");
    assert!(n.hidden, "pocketed, not removed");
    assert!(h.app.doc().scene.node(wire).is_some(), "the wire stays");
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1);
    assert_eq!(h.app.agents.retract_ghosts.len(), 1);
    let left = slate_doc::connector_anchor_on(
        h.app.doc().scene.node(card).unwrap(),
        slate_doc::scene::Side::Left,
        0.5,
    );
    assert_eq!(
        h.app.agents.retract_ghosts[0].1,
        Pos2::new(left[0], left[1])
    );
    assert!(h.app.agent_has_pocket(card));

    h.app.board_undo();
    assert!(
        !h.app.doc().scene.node(used).unwrap().hidden,
        "undo shows it"
    );
    h.app.board_redo();
    assert!(h.app.doc().scene.node(used).unwrap().hidden);

    let card_rect = h.app.doc().scene.node(card).unwrap().rect;
    assert!(h.app.dispatch(
        &h.ctx,
        atlas_commands::CommandId("portal.agent.pocket"),
        Some(card.0.to_string())
    ));
    let shown = h.app.doc().scene.node(used).unwrap();
    assert!(!shown.hidden, "a handle click shows it");
    assert!(
        shown.rect.x + shown.rect.w <= card_rect.x,
        "beside the card"
    );
    let ghosts = h.app.agents.retract_ghosts.len();
    assert!(h.app.agent_toggle_pocket(Some(&card.0.to_string())));
    assert!(h.app.doc().scene.node(used).unwrap().hidden, "second click");
    assert_eq!(h.app.agents.retract_ghosts.len(), ghosts + 1);
}

#[test]
fn shared_context_pockets_into_every_train_that_used_it() {
    let mut h = board("agent_pocket_shared");
    let a = train(&mut h, Pos2::ZERO, "local");
    let b = train(&mut h, Pos2::new(0.0, 500.0), "local");
    let used = note(&mut h, Pos2::new(-400.0, 250.0), "shared brief");
    let to_a = sent_wire(&mut h, used, a);
    let to_b = sent_wire(&mut h, used, b);
    h.frame();
    let before = h.app.doc().scene.nodes.len();
    h.app.delete_board_nodes(&[used]);
    assert_eq!(h.app.doc().scene.nodes.len(), before, "nothing removed");
    assert_eq!(
        h.app.doc().scene.nodes.iter().filter(|n| n.hidden).count(),
        1,
        "one hidden node"
    );
    assert_eq!(h.app.agents.retract_ghosts.len(), 2, "one ghost per train");
    assert!(h.app.agent_has_pocket(a) && h.app.agent_has_pocket(b));

    assert!(h.app.agent_toggle_pocket(Some(&b.0.to_string())));
    assert!(!h.app.doc().scene.node(used).unwrap().hidden);
    for wire in [to_a, to_b] {
        assert!(
            h.app.doc().scene.node(wire).is_some(),
            "still wired to both"
        );
    }
    assert!(!h.app.agent_has_pocket(a), "visible to every train");
    h.app.delete_board_nodes(&[used]);
    assert!(h.app.doc().scene.node(used).unwrap().hidden);
    assert_eq!(h.app.agents.retract_ghosts.len(), 4);
    assert!(
        h.app.agent_toggle_pocket(Some(&a.0.to_string())),
        "from either"
    );
    assert!(!h.app.doc().scene.node(used).unwrap().hidden);
}

#[test]
fn deleting_the_last_consuming_card_removes_its_pocket_in_the_same_step() {
    let mut h = board("agent_pocket_orphan");
    let card = train(&mut h, Pos2::ZERO, "local");
    let other = train(&mut h, Pos2::new(0.0, 500.0), "local");
    let lone = note(&mut h, Pos2::new(-400.0, 0.0), "only here");
    let shared = note(&mut h, Pos2::new(-400.0, 300.0), "also elsewhere");
    let lone_wire = sent_wire(&mut h, lone, card);
    sent_wire(&mut h, shared, card);
    sent_wire(&mut h, shared, other);
    h.app.delete_board_nodes(&[lone, shared]);
    assert!(h.app.doc().scene.node(lone).unwrap().hidden);
    assert!(h.app.doc().scene.node(shared).unwrap().hidden);
    let depth = h.app.tab().journal.undo_depth();
    h.app.board_sel = [card].into_iter().collect();
    h.app.delete_board_nodes(&[card]);
    assert!(h.app.doc().scene.node(card).is_none());
    assert!(
        h.app.doc().scene.node(lone).is_none(),
        "no invisible orphan"
    );
    assert!(h.app.doc().scene.node(lone_wire).is_none());
    assert!(
        h.app.doc().scene.node(shared).is_some_and(|n| n.hidden),
        "still pocketed in the other train"
    );
    assert_eq!(h.app.tab().journal.undo_depth(), depth + 1, "one commit");
    h.app.board_undo();
    assert!(h.app.doc().scene.node(card).is_some());
    assert!(h.app.doc().scene.node(lone).is_some_and(|n| n.hidden));
    assert!(h.app.doc().scene.node(lone_wire).is_some());
}

/// The list hangs under the title's model name; it never drifts off it.
fn menu_under_title(output: &egui::FullOutput) -> bool {
    let texts = painted_at(output);
    let (Some(title), Some(first)) = (
        texts.iter().find(|(t, _)| t == "Astra").map(|(_, r)| *r),
        texts.iter().find(|(t, _)| t == "Default").map(|(_, r)| *r),
    ) else {
        return false;
    };
    // Slack for the menu frame's and row's padding.
    let gap = first.top() - title.bottom();
    gap >= 0.0
        && gap < title.height() * 3.0 + 16.0
        && (first.left() - title.left()).abs() < title.height() + 16.0
}

#[test]
fn the_model_list_stays_open_while_the_wheel_zooms_the_board() {
    let mut h = board("model_menu_wheel");
    let card = codex_card_with_models(&mut h);
    open_model_menu(&mut h, card, 1.0);
    let r = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(card).unwrap().rect);
    // Left of the card on empty board, clear of the list below the
    // title: the wheel zooms about the card and keeps it in view.
    let beside = Pos2::new(r.left() - 24.0, r.center().y);
    let mut zooms = Vec::new();
    wheel_sweep(&mut h, beside, 24, |_, out, step, z| {
        zooms.push(z);
        assert!(
            menu_items_painted(out),
            "the model list vanished at step {step}, zoom {z}: {:?}",
            painted_at(out)
        );
        assert!(
            menu_under_title(out),
            "the model list left its title at step {step}, zoom {z}: {:?}",
            painted_at(out)
        );
    });
    assert!(
        zooms.iter().cloned().fold(0.0, f32::max) > 2.0,
        "the board zoomed in: {zooms:?}"
    );
}

#[test]
fn the_model_list_opened_zoomed_in_stays_whole_while_zooming_out_and_back() {
    let mut h = board("model_menu_zoomed");
    let card = codex_card_with_models(&mut h);
    open_model_menu(&mut h, card, 2.5);
    let r = h
        .app
        .board_xf()
        .rect_w2s(h.app.doc().scene.node(card).unwrap().rect);
    let beside = Pos2::new(r.left() - 24.0, r.center().y);
    let mut zooms = Vec::new();
    for delta in [-40.0, 40.0] {
        for step in 0..12 {
            let out = h.frame_output(|i| {
                i.events.push(egui::Event::PointerMoved(beside));
                i.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, delta),
                    modifiers: Default::default(),
                });
            });
            let z = h.app.tab().cam.z;
            zooms.push(z);
            assert!(
                menu_items_painted(&out) && menu_under_title(&out),
                "the model list broke at step {step} ({delta}), zoom {z}: {:?}",
                painted_at(&out)
            );
        }
    }
    assert!(
        zooms.iter().cloned().fold(f32::MAX, f32::min) < 1.5,
        "the board zoomed out: {zooms:?}"
    );
}
