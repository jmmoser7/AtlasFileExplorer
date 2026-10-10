//! Picker wheel and the stop square.

use super::test_support::*;
use super::*;

#[test]
fn the_model_list_owns_the_wheel_and_stays_open_over_itself() {
    let mut h = board("model_menu_over");
    let card = codex_card_with_models(&mut h);
    open_model_menu(&mut h, card, 1.0);
    let out = h.frame_output(|_| {});
    let over = painted_at(&out)
        .into_iter()
        .find(|(t, _)| t == "Terra")
        .map(|(_, r)| r.center())
        .unwrap();
    let z0 = h.app.tab().cam.z;
    wheel_sweep(&mut h, over, 12, |_, out, step, z| {
        assert!(menu_items_painted(out), "vanished at step {step}");
        assert_eq!(z, z0, "a navigable menu owns the wheel (P0.10)");
    });
}

#[test]
fn the_model_list_is_a_popup_above_canvas_chrome() {
    let mut h = board("model_menu_layer");
    let card = codex_card_with_models(&mut h);
    open_model_menu(&mut h, card, 1.0);
    let out = h.frame_output(|_| {});
    let terra = painted_at(&out)
        .into_iter()
        .find(|(t, _)| t == "Terra")
        .map(|(_, r)| r.center())
        .unwrap();
    let layer = h.ctx.layer_id_at(terra).expect("hit-testable");
    assert_eq!(
        layer,
        egui::LayerId::new(
            atlas_shell::selection_tools::POPUP_ORDER,
            Id::new(("agent-model-menu", card.0)),
        ),
        "the model list paints and takes the pointer above canvas chrome"
    );
}

/// A Codex card choosing among `n` saved conversations.
fn codex_chat_picker(h: &mut super::super::tests::Harness, n: usize) -> NodeId {
    let card = codex_card_with_models(h);
    let chats = (0..n)
        .map(|i| CursorChat {
            id: format!("thread-{i}"),
            title: format!("Conversation {i}"),
            updated_at: i as u64,
        })
        .collect();
    h.app.present_agent_picker(card, chats);
    center_on(h, card);
    h.frame();
    h.frame();
    card
}

#[test]
fn the_conversation_picker_stays_painted_while_the_wheel_zooms_over_it() {
    for n in [2, 5, 12] {
        let mut h = board("chat_picker_wheel");
        let card = codex_chat_picker(&mut h, n);
        let out = h.frame_output(|_| {});
        let at = painted_at(&out)
            .into_iter()
            .find(|(t, _)| t == "Conversation 0")
            .map(|(_, r)| r.center())
            .expect("the picker lists conversations");
        let mut zooms = Vec::new();
        // In and back out again: below ~25% the rows are too small to
        // draw and drop by LOD (P0.9), which is not a flicker.
        wheel_sweep(&mut h, at, 10, |h, out, step, z| {
            zooms.push(z);
            assert!(
                h.app
                    .agents
                    .chat_picker
                    .as_ref()
                    .is_some_and(|p| p.portal == card),
                "the picker closed at step {step} ({n} chats)"
            );
            assert!(
                painted_at(out)
                    .iter()
                    .any(|(t, _)| t.starts_with("Conversation ")),
                "the picker vanished at step {step}, zoom {z} ({n} chats): {zooms:?}"
            );
        });
    }
}

/// Where each conversation row's title is painted, relative to the card's
/// top-left and divided by the zoom: world units, so zoom alone moves
/// nothing. `xf` is the camera the frame painted with; the wheel moves
/// the camera after the board paints.
fn picker_rows(
    h: &super::super::tests::Harness,
    xf: &BoardXf,
    out: &egui::FullOutput,
    card: NodeId,
    n: usize,
) -> Vec<Option<egui::Vec2>> {
    let origin = xf.rect_w2s(h.app.doc().scene.node(card).unwrap().rect).min;
    let texts = painted_at(out);
    (0..n)
        .map(|i| {
            let title = format!("Conversation {i}");
            texts
                .iter()
                .find(|(t, _)| *t == title)
                .map(|(_, r)| (r.min - origin) / xf.z)
        })
        .collect()
}

#[test]
fn the_conversation_picker_rows_keep_their_columns_while_ctrl_wheel_zooms() {
    for n in [12, 20] {
        let mut h = board("chat_picker_columns");
        let card = codex_chat_picker(&mut h, n);
        let xf = h.app.board_xf();
        let out = h.frame_output(|_| {});
        let base: Vec<egui::Vec2> = picker_rows(&h, &xf, &out, card, n)
            .into_iter()
            .map(|r| r.expect("every row is painted"))
            .collect();
        for row in base.chunks(3) {
            assert!(
                row.iter().all(|p| (p.y - row[0].y).abs() < 0.5),
                "three columns share each row ({n} chats): {base:?}"
            );
            assert!(
                row.windows(2).all(|w| w[1].x > w[0].x + 40.0),
                "columns read left to right ({n} chats): {base:?}"
            );
        }
        let at = h
            .app
            .board_xf()
            .rect_w2s(h.app.doc().scene.node(card).unwrap().rect)
            .min
            + base[1] * h.app.tab().cam.z
            + egui::vec2(4.0, 4.0);
        let mut zooms = Vec::new();
        for step in 0..20 {
            let delta = if step < 10 { 40.0 } else { -40.0 };
            let xf = h.app.board_xf();
            let out = h.frame_output(|i| {
                i.events.push(egui::Event::PointerMoved(at));
                i.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, delta),
                    modifiers: egui::Modifiers::CTRL,
                });
            });
            let z = xf.z;
            zooms.push(h.app.tab().cam.z);
            for (i, (now, want)) in picker_rows(&h, &xf, &out, card, n)
                .into_iter()
                .zip(&base)
                .enumerate()
            {
                let now = now.unwrap_or_else(|| {
                    panic!("row {i} vanished at step {step}, zoom {z} ({n} chats)")
                });
                assert!(
                    ((now - *want) * z).length() <= 2.5,
                    "row {i} moved from {want:?} to {now:?} at step {step}, zoom {z} ({n} chats)"
                );
            }
        }
        assert!(
            zooms.iter().cloned().fold(0.0, f32::max) > 1.5,
            "Ctrl+wheel zoomed the board: {zooms:?}"
        );
    }
}

#[test]
fn the_project_picker_stays_single_column() {
    let mut h = board("project_picker_column");
    let card = codex_card_with_models(&mut h);
    h.app.agents.provider_recents.insert(
        "codex".into(),
        (0..12)
            .map(|i| RecentEntry {
                path: PathBuf::from(format!("C:/projects/p{i}")),
                title: format!("Project {i}"),
                opened_at: i,
                cover: None,
            })
            .collect(),
    );
    h.app.agents.recents_started = true;
    h.app.agents.recents_rx = None;
    h.app.agents.project_picker = Some(card);
    center_on(&mut h, card);
    h.frame();
    let out = h.frame_output(|_| {});
    let lefts: Vec<f32> = painted_at(&out)
        .into_iter()
        .filter(|(t, _)| t.starts_with("Project "))
        .map(|(_, r)| r.left())
        .collect();
    assert!(lefts.len() >= 2, "the project list is painted");
    assert!(
        lefts.iter().all(|x| (x - lefts[0]).abs() < 0.5),
        "one column of projects: {lefts:?}"
    );
}

#[test]
fn the_project_picker_stays_painted_while_the_wheel_moves_over_it() {
    let mut h = board("project_picker_wheel");
    let card = codex_card_with_models(&mut h);
    h.app.agents.provider_recents.insert(
        "codex".into(),
        (0..4)
            .map(|i| RecentEntry {
                path: PathBuf::from(format!("C:/projects/p{i}")),
                title: format!("Project {i}"),
                opened_at: i,
                cover: None,
            })
            .collect(),
    );
    h.app.agents.recents_started = true;
    h.app.agents.recents_rx = None;
    h.app.agents.project_picker = Some(card);
    center_on(&mut h, card);
    h.frame();
    let out = h.frame_output(|_| {});
    let at = painted_at(&out)
        .into_iter()
        .find(|(t, _)| t == "Project 0")
        .map(|(_, r)| r.center())
        .expect("the picker lists projects");
    wheel_sweep(&mut h, at, 12, |h, out, step, z| {
        assert_eq!(h.app.agents.project_picker, Some(card));
        assert!(
            painted_at(out)
                .iter()
                .any(|(t, _)| t.starts_with("Project ")),
            "the project list vanished at step {step}, zoom {z}"
        );
    });
}

#[test]
fn a_streaming_card_turns_its_output_circle_into_the_one_stop() {
    let (mut h, tail, cancel) = streaming_tail("stop_on_output");
    let out = h.frame_output(|_| {});
    let at = output_circle(&h, tail);
    assert!(
        stop_square_at(&out, at).is_some(),
        "Stop is drawn on the output circle while the reply streams"
    );
    let field = h.app.agents.composer_rects[&tail];
    let old = field.right_bottom() - egui::vec2(5.5, 5.5);
    press(&mut h, old);
    assert!(
        !stop_requested(&cancel),
        "nothing at the bottom of the card stops the reply"
    );
    press(&mut h, at);
    assert!(
        stop_requested(&cancel),
        "a click on the output circle stops"
    );
    assert_eq!(
        h.app.board_sel.iter().copied().collect::<Vec<_>>(),
        vec![tail],
        "Stop acts on its own card"
    );
    assert!(
        !h.app
            .doc()
            .scene
            .nodes
            .iter()
            .any(|n| slate_doc::agent_chat::agent(n).is_some_and(|a| a.chat.draft)),
        "Stop does not continue the train"
    );
}

/// Any circle painted centered on `at`.
fn circle_at(output: &egui::FullOutput, at: Pos2) -> bool {
    fn walk(shape: &egui::Shape, at: Pos2) -> bool {
        match shape {
            egui::Shape::Circle(c) => c.center.distance(at) < 1.5,
            egui::Shape::Vec(v) => v.iter().any(|s| walk(s, at)),
            _ => false,
        }
    }
    output.shapes.iter().any(|c| walk(&c.shape, at))
}

#[test]
fn stop_is_a_bare_gray_square_that_scales_with_the_board() {
    let (mut h, tail, _) = streaming_tail("stop_bare_square");
    let mut sides = Vec::new();
    for z in [1.0, 2.0] {
        center_on(&mut h, tail);
        h.app.tab_mut().cam.z = z;
        h.frame();
        let out = h.frame_output(|_| {});
        let at = output_circle(&h, tail);
        let square = stop_square_at(&out, at).expect("Stop is a square on the output circle");
        assert!(
            !circle_at(&out, at),
            "no disc or ring around the Stop square at zoom {z}"
        );
        assert!(
            (square.width() - square.height()).abs() < 0.01,
            "a square: {square:?}"
        );
        sides.push(square.width() / z);
    }
    assert!(
        (sides[0] - sides[1]).abs() < 0.01,
        "the square scales with the board (P0.9): {sides:?}"
    );
}

#[test]
fn stop_holds_its_place_while_the_streaming_card_grows() {
    let (mut h, tail, cancel) = streaming_tail("stop_holds");
    let at = output_circle(&h, tail);
    let before = h.app.doc().scene.node(tail).unwrap().rect.h;
    let mut turns = h.app.agents.local_turns[&tail].clone();
    for step in 0..6 {
        turns.last_mut().unwrap().text += &" more words arrive and wrap onto new lines".repeat(4);
        h.app.agents.local_turns.insert(tail, turns.clone());
        h.app.agents.output_epoch += 1;
        let out = h.frame_output(|_| {});
        assert_eq!(output_circle(&h, tail), at, "the card grows downward");
        let square = stop_square_at(&out, at);
        assert!(
            square.is_some(),
            "Stop stays on the output circle at step {step}"
        );
    }
    assert!(
        h.app.doc().scene.node(tail).unwrap().rect.h > before,
        "the card grew while streaming"
    );
    press(&mut h, at);
    assert!(stop_requested(&cancel), "the unmoved Stop still stops");
}

#[test]
fn stop_lands_before_the_link_folder_is_published() {
    let (mut h, tail, cancel) = streaming_tail("stop_before_link");
    let at = output_circle(&h, tail);
    let _ = std::fs::remove_dir_all(cancel.parent().unwrap());
    press(&mut h, at);
    assert!(
        stop_requested(&cancel),
        "Stop is never lost to a missing folder"
    );
}
