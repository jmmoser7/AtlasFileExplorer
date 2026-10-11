//! Overnight polish sheets for the chat train card. The test entry is
//! `review::chat_train::ct_polish_sheets`; this file holds the shots so
//! `chat_train.rs` stays within the line cap.

use super::chat_train::{aim, card_screen, fold, prepare, settle, shoot};
use super::*;

/// Each harness is a new egui context. A galley cached from the previous
/// context keeps font-atlas coordinates that do not exist in this one, and
/// the title rasterizes as a speck. Drop the cache before every scenario.
fn fresh() {
    atlas_shell::canvas_text::clear_world_layout_cache();
}

/// The context dot is painted on a card that already has a parent, and only
/// after its fade. A hidden parent keeps the crop to this one card.
fn with_parent(h: &mut Harness, id: NodeId) {
    h.app.place_agent_portal_at(Pos2::new(-4000.0, 0.0));
    let parent = h.app.doc().scene.nodes.last().unwrap().id;
    h.app.set_agent_program(parent, "cursor");
    h.app.review_dismiss_picker();
    h.app.patch_nodes(&[parent], |n| n.hidden = true);
    h.app.patch_nodes(&[id], |n| {
        if let NodeKind::Portal(p) = &mut n.kind {
            p.agent.as_mut().unwrap().chat.parent = Some(parent);
        }
    });
}

/// Hover `at` while egui time advances, so a fade-in finishes, then crop the card.
fn shoot_settled(h: &mut Harness, raster: &mut FrameRaster, id: NodeId, at: Pos2, state: &str) {
    let start = h.ctx.input(|i| i.time);
    let mut out = None;
    for frame in 0..18 {
        out = Some(capture_frame(h, raster, |i| {
            i.time = Some(start + frame as f64 / 30.0);
            i.events.push(egui::Event::PointerMoved(at));
        }));
    }
    let area = card_screen(h, id).expand(24.0 * h.app.tab().cam.z.max(1.0));
    review_shot_crop(h, raster, out.unwrap(), "CT-polish", state, area, 1);
}

pub(super) fn sheets() {
    fresh();
    let (mut h, id, mut raster) = prepare("polish_chevron", false, false);
    aim(&mut h, id, 2.0);
    for (name, index) in [("chevron-at-two-steps", 0), ("chevron-at-one-step", 1)] {
        let at = h.app.chevron_hover_point(index);
        shoot(&mut h, &mut raster, "CT-polish", name, at, id);
    }
    fresh();
    let (mut h, id, mut raster) = prepare("polish_model", false, false);
    aim(&mut h, id, 2.0);
    let at = h
        .ctx
        .data(|d| d.get_temp::<Rect>(egui::Id::new(("agent-model-hit", id.0))))
        .expect("the model name has a hit rect")
        .center();
    shoot(
        &mut h,
        &mut raster,
        "CT-polish",
        "model-name-hover",
        Some(at),
        id,
    );
    fresh();
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
        fresh();
        let (mut h, id, mut raster) = prepare(&format!("polish_{name}"), collapsed, partial);
        with_parent(&mut h, id);
        settle(&mut h);
        aim(&mut h, id, 2.0);
        let node = h.app.doc().scene.node(id).unwrap().clone();
        let screen = card_screen(&h, id);
        let at = h
            .app
            .chat_context_dot(&h.ctx, &node, screen, h.app.tab().cam.z);
        shoot_settled(&mut h, &mut raster, id, at, name);
    }
    fresh();
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
