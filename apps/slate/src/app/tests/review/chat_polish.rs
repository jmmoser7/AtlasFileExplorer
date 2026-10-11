//! Overnight polish sheets for the chat train card. The test entry is
//! `review::chat_train::ct_polish_sheets`; this file holds the shots so
//! `chat_train.rs` stays within the line cap.

use super::chat_train::{aim, card_screen, fold, prepare, settle, shoot};
use super::*;

pub(super) fn sheets() {
    let (mut h, id, mut raster) = prepare("polish_chevron", false, false);
    aim(&mut h, id, 2.0);
    for (name, index) in [("chevron-at-two-steps", 0), ("chevron-at-one-step", 1)] {
        let at = h.app.chevron_hover_point(index);
        shoot(&mut h, &mut raster, "CT-polish", name, at, id);
    }
    let (mut h, id, mut raster) = prepare("polish_model", false, false);
    aim(&mut h, id, 2.0);
    let r = card_screen(&h, id);
    let at = Pos2::new(r.left() + 130.0, r.top() + 24.0);
    shoot(
        &mut h,
        &mut raster,
        "CT-polish",
        "model-name-hover",
        Some(at),
        id,
    );
    let (mut h, id, mut raster) = prepare("polish_tail", false, false);
    h.app.board_sel.insert(id);
    assert!(h.app.agent_spawn_command(Some("[720,80]")));
    let tail = *h.app.board_sel.iter().next().unwrap();
    settle(&mut h);
    aim(&mut h, tail, 2.0);
    shoot(
        &mut h,
        &mut raster,
        "CT-polish",
        "tail-card-dots",
        None,
        tail,
    );
    for (name, collapsed, partial) in [
        ("port-dot-collapsed", true, false),
        ("port-dot-expanded", false, false),
    ] {
        let (mut h, id, mut raster) = prepare(&format!("polish_{name}"), collapsed, partial);
        settle(&mut h);
        aim(&mut h, id, 2.0);
        let node = h.app.doc().scene.node(id).unwrap().clone();
        let screen = card_screen(&h, id);
        let at = h
            .app
            .chat_context_dot(&h.ctx, &node, screen, h.app.tab().cam.z);
        shoot(&mut h, &mut raster, "CT-polish", name, Some(at), id);
    }
    let (mut h, id, mut raster) = prepare("polish_scroll", false, true);
    let long = "The courtyard keeps its plane trees along the north wall. ";
    let lines: Vec<(String, String)> = (0..8)
        .map(|i| {
            (
                if i % 2 == 0 { "user" } else { "assistant" }.into(),
                format!("{long}{long}Line {i}."),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = lines
        .iter()
        .map(|(r, t)| (r.as_str(), t.as_str()))
        .collect();
    h.app.review_chat_lines(id, &borrowed);
    fold(&mut h, id, false, true);
    h.app.board_sel.insert(id);
    settle(&mut h);
    aim(&mut h, id, 1.0);
    let center = card_screen(&h, id).center();
    for _ in 0..8 {
        capture_frame(&mut h, &mut raster, |i| {
            i.events.push(egui::Event::PointerMoved(center));
            i.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -80.0),
                modifiers: Default::default(),
            });
        });
    }
    shoot(
        &mut h,
        &mut raster,
        "CT-polish",
        "partial-card-scrolled",
        Some(center),
        id,
    );
}
